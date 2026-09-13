import Foundation

/// A read-only, asynchronously searchable hybrid index (Feature 007).
///
/// Wraps the uniffi-generated `IndexHandle`. The Rust surface is synchronous on purpose
/// (research D2): uniffi 0.32.1 polls a Rust future on the awaiting task's thread, so a 2–3 s
/// search inside a Rust `async fn` would block Swift's cooperative pool. Instead every call runs
/// on a private **serial** dispatch queue and resumes the caller through a continuation:
///
/// - the caller's thread is never blocked (spec FR-005);
/// - calls on one instance run one at a time, in call order (FR-006). Two instances are
///   independent — each owns its Rust handle and mutex — so searches on different instances
///   can run concurrently, even over the same directory;
/// - cancelling the awaiting task is **cooperative** (FR-008): the Rust work runs to its budget,
///   then the caller gets `CancellationError` and the result is dropped; the instance stays
///   usable. The time budget (`SearchOptions.maxTimeMs`) is the bound.
///
/// `loadPath: .mmap` maps both models' weight files **and** the dense index's vectors
/// (`dense/index.bin`) read-only (ADR-0007, ADR-0009); the caller owns the precondition that no
/// other process modifies or truncates any of those files while the instance lives. The index
/// content is never modified (see ``open(indexDir:embedderDir:rerankerDir:loadPath:)`` for the
/// lock-file caveat).
public final class XtrieverIndex: @unchecked Sendable {
    /// Identity and configuration of the open index, read once at open.
    public let info: IndexInfo

    private let handle: IndexHandle
    private let queue = DispatchQueue(label: "dev.xtriever.index", qos: .utility)

    private init(handle: IndexHandle) {
        self.handle = handle
        self.info = handle.info()
    }

    /// Open a hybrid index read-only with the pinned embedder and, optionally, the pinned
    /// re-ranker. Model loading (~100 ms per model when mapped, more when buffered) runs off the
    /// caller's thread.
    ///
    /// **The index directory must be writable** even though its content is never modified: the
    /// lexical backend opens a zero-byte lock file (`lexical/.tantivy-meta.lock`) for writing at
    /// every open, and refuses a directory where it cannot (007 report F-001). An index shipped
    /// inside the app bundle — read-only on a device — must first be copied out with
    /// ``writableCopy(of:named:)``.
    public static func open(
        indexDir: URL,
        embedderDir: URL,
        rerankerDir: URL?,
        loadPath: LoadPath = .mmap
    ) async throws -> XtrieverIndex {
        let handle: IndexHandle = try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .utility).async {
                continuation.resume(with: Result {
                    try IndexHandle.open(
                        indexDir: indexDir.path,
                        embedderDir: embedderDir.path,
                        rerankerDir: rerankerDir?.path,
                        loadPath: loadPath
                    )
                })
            }
        }
        // Cooperative cancellation: the open ran to completion, but a cancelled caller must not
        // receive a handle it never asked to keep.
        try Task.checkCancellation()
        return XtrieverIndex(handle: handle)
    }

    /// One search, off the caller's thread; calls on one instance run one at a time.
    ///
    /// The default options ask for 10 explained hits with the index's default depths and no
    /// budget. Set `maxTimeMs` to bound the search's own work: the dense stage is skipped or the
    /// re-ranker stops early and `stages` says so; with `strict` those become thrown errors
    /// instead. The budget is measured from the moment the Rust side is entered (FR-007);
    /// time spent queued behind an earlier search on this instance is **not** counted — a
    /// queued search still gets its full budget rather than arriving with it spent and
    /// degrading to a lexical-only result. Queue depth is the caller's to control (FR-006).
    public func search(
        _ query: String,
        options: SearchOptions = SearchOptions(k: 10, explain: true)
    ) async throws -> SearchResponse {
        let handle = self.handle
        let response: SearchResponse = try await withCheckedThrowingContinuation { continuation in
            queue.async {
                continuation.resume(with: Result { try handle.search(query: query, options: options) })
            }
        }
        // The Rust work cannot be interrupted, so cancellation is observed here, after it: the
        // response is dropped and the caller gets `CancellationError` (FR-008).
        try Task.checkCancellation()
        return response
    }
}

public extension XtrieverIndex {
    /// Copy a bundled index directory into Application Support (once), returning the writable
    /// location to pass to ``open(indexDir:embedderDir:rerankerDir:loadPath:)``.
    ///
    /// The copy is keyed by `name` and the source descriptor's bytes: a bundle that ships a
    /// newer index (a different `xtriever-pipeline.json`) replaces the old copy; the same index
    /// is not copied twice. Models need no copy — they are opened read-only.
    ///
    /// `name` is one path component (no separators, not `.` or `..`, not empty); anything else
    /// throws `CocoaError.fileWriteInvalidFileName`, since the copy's directory is removed and
    /// replaced under that name. Calls are serialised process-wide.
    static func writableCopy(of source: URL, named name: String) throws -> URL {
        guard !name.isEmpty, name != ".", name != "..",
              !name.contains("/"), !name.contains("\\"), !name.contains("\0")
        else { throw CocoaError(.fileWriteInvalidFileName) }
        copyLock.lock(); defer { copyLock.unlock() }

        let fm = FileManager.default
        let base = try fm.url(for: .applicationSupportDirectory, in: .userDomainMask,
                              appropriateFor: nil, create: true)
            .appendingPathComponent("Xtriever", isDirectory: true)
            .appendingPathComponent(name, isDirectory: true)
        let descriptor = "xtriever-pipeline.json"
        let sourceDescriptor = try Data(contentsOf: source.appendingPathComponent(descriptor))
        if let existing = try? Data(contentsOf: base.appendingPathComponent(descriptor)),
           existing == sourceDescriptor {
            return base
        }
        // Copy into a uniquely named sibling first and swap it in only once it is complete: a
        // disk-full error or a kill mid-copy leaves the previous complete copy untouched, never
        // a partial tree whose descriptor the check above would take for a finished one.
        // `replaceItemAt` keeps the old tree until the swap; the staging tree is removed by it
        // (or by the move) on success and by the `defer` on failure.
        let staging = base.deletingLastPathComponent()
            .appendingPathComponent("\(name).staging-\(UUID().uuidString)", isDirectory: true)
        try fm.createDirectory(at: base.deletingLastPathComponent(), withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: staging) }
        try fm.copyItem(at: source, to: staging)
        if fm.fileExists(atPath: base.path) {
            _ = try fm.replaceItemAt(base, withItemAt: staging)
        } else {
            try fm.moveItem(at: staging, to: base)
        }
        return base
    }

    private static let copyLock = NSLock()
}

public extension HitExplain {
    /// The explanation under the pipeline's seven feature names, `.nan` where a stage did not
    /// see the hit — the same names and order as `xtriever_pipeline::HitExplain::features()`.
    func features() -> [(name: String, value: Float)] {
        [
            ("bm25.score", bm25Score ?? .nan),
            ("bm25.rank", bm25Rank.map(Float.init) ?? .nan),
            ("dense.score", denseScore ?? .nan),
            ("dense.rank", denseRank.map(Float.init) ?? .nan),
            ("fused.score", Float(fused)),
            ("rerank.score", rerankScore ?? .nan),
            ("rerank.rank", rerankRank.map(Float.init) ?? .nan),
        ]
    }
}

public extension XtrieverError {
    /// The engine's message for this error, whichever case it is — for logs and UIs.
    var message: String {
        switch self {
        case .Schema(let message), .InvalidQuery(let message), .Corrupt(let message),
             .BudgetExhausted(let message), .Io(let message), .Backend(let message):
            return message
        case .UnknownField(let field):
            return "unknown field `\(field)`"
        case .DimensionMismatch(let expected, let actual):
            return "dimension mismatch: expected \(expected), got \(actual)"
        case .NotFound(let id):
            return "document \(id) not found"
        case .Model(let model, let message):
            return "model `\(model)`: \(message)"
        case .FingerprintMismatch(let index, let current):
            return "embedder fingerprint mismatch: index has `\(index)`, embedder is `\(current)`"
        }
    }
}

/// Where the build script stages resources inside the library bundle.
public enum HarnessResources {
    static var data: URL? { Bundle.module.url(forResource: "XtrieverData", withExtension: nil) }

    /// `XtrieverData/models/{embedder,reranker}`.
    public static var embedderDirectory: URL? { data?.appendingPathComponent("models/embedder") }
    public static var rerankerDirectory: URL? { data?.appendingPathComponent("models/reranker") }
    /// `XtrieverData/fixtures/{index,expected.json}`.
    public static var fixtureIndexDirectory: URL? { data?.appendingPathComponent("fixtures/index") }
    public static var fixtureExpected: URL? { data?.appendingPathComponent("fixtures/expected.json") }
    /// `XtrieverData/scifact/{index,queries.json,expected-scifact.json}`.
    public static var scifactIndexDirectory: URL? { data?.appendingPathComponent("scifact/index") }
    public static var scifactQueries: URL? { data?.appendingPathComponent("scifact/queries.json") }
    public static var scifactExpected: URL? { data?.appendingPathComponent("scifact/expected-scifact.json") }

    public static var modelsAreBundled: Bool {
        guard let e = embedderDirectory, let r = rerankerDirectory else { return false }
        return FileManager.default.fileExists(atPath: e.appendingPathComponent("model.safetensors").path)
            && FileManager.default.fileExists(atPath: r.appendingPathComponent("model.safetensors").path)
    }

    public static var fixtureIsBundled: Bool {
        guard let i = fixtureIndexDirectory, let e = fixtureExpected else { return false }
        return FileManager.default.fileExists(atPath: i.appendingPathComponent("xtriever-pipeline.json").path)
            && FileManager.default.fileExists(atPath: e.path)
    }

    public static var scifactIsBundled: Bool {
        guard let i = scifactIndexDirectory, let q = scifactQueries else { return false }
        return FileManager.default.fileExists(atPath: i.appendingPathComponent("xtriever-pipeline.json").path)
            && FileManager.default.fileExists(atPath: q.path)
    }
}
