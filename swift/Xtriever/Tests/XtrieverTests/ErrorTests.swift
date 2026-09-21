import XCTest
import Xtriever

/// US3 scenarios 1–5 — every failure class arrives as its mapped case with the engine's message,
/// and none crashes the process (SC-003).
final class ErrorTests: XCTestCase {
    private func open(_ index: URL, embedder: URL, reranker: URL? = nil) async throws -> XtrieverIndex {
        try await XtrieverIndex.open(indexDir: index, embedderDir: embedder, rerankerDir: reranker, loadPath: .buffered)
    }

    func testAMissingDirectoryIsCorrupt() async throws {
        let m = try Support.models()
        let missing = FileManager.default.temporaryDirectory.appendingPathComponent("no-such-\(UUID().uuidString)")
        do {
            _ = try await open(missing, embedder: m.embedder)
            XCTFail("must throw")
        } catch XtrieverError.Corrupt(let message) {
            XCTAssertTrue(message.contains(missing.lastPathComponent), message)
        }
    }

    func testAVersionOneDirectoryIsCorruptNamingBothVersions() async throws {
        let m = try Support.models()
        let f = try Support.fixture()
        let copy = try Support.copy(f.index)
        defer { try? FileManager.default.removeItem(at: copy) }
        let desc = copy.appendingPathComponent("xtriever-pipeline.json")
        let text = try String(contentsOf: desc, encoding: .utf8)
        try text.replacingOccurrences(of: "\"format_version\": 2", with: "\"format_version\": 1")
            .write(to: desc, atomically: true, encoding: .utf8)
        do {
            _ = try await open(copy, embedder: m.embedder)
            XCTFail("must throw")
        } catch XtrieverError.Corrupt(let message) {
            XCTAssertTrue(message.contains("1") && message.contains("2") && message.contains("rebuild"), message)
        }
    }

    func testAnInterruptedCommitIsRefused() async throws {
        let m = try Support.models()
        let f = try Support.fixture()
        let copy = try Support.copy(f.index)
        defer { try? FileManager.default.removeItem(at: copy) }
        try "3".write(to: copy.appendingPathComponent("commit.pending"), atomically: true, encoding: .utf8)
        do {
            _ = try await open(copy, embedder: m.embedder)
            XCTFail("must throw")
        } catch XtrieverError.Corrupt(let message) {
            XCTAssertTrue(message.contains("interrupted commit"), message)
        }
    }

    func testTheWrongModelAsEmbedderIsAModelError() async throws {
        let m = try Support.models()
        let f = try Support.fixture()
        do {
            _ = try await open(f.index, embedder: m.reranker)
            XCTFail("must throw")
        } catch XtrieverError.Model(let model, let message) {
            XCTAssertFalse(model.isEmpty)
            XCTAssertTrue(message.contains("bytes") || message.contains("sha256"), message)
        }
    }

    func testATamperedWeightsFileNamesTheFile() async throws {
        let m = try Support.models()
        let f = try Support.fixture()
        let copy = try Support.copy(m.embedder)
        defer { try? FileManager.default.removeItem(at: copy) }
        // Whichever pinned artefact the package bundles (Feature 026: the eight-bit GGUF by
        // default, the float file when built with the float manifest): the engine names the
        // file and its pinned size.
        let names = try FileManager.default.contentsOfDirectory(atPath: copy.path)
        guard let weightsName = names.first(where: { $0 == "model.safetensors" || $0.hasSuffix(".gguf") }) else {
            return XCTFail("no weights file in \(names)")
        }
        let weights = copy.appendingPathComponent(weightsName)
        let pinnedBytes = try FileManager.default.attributesOfItem(atPath: weights.path)[.size] as? UInt64 ?? 0
        let handle = try FileHandle(forWritingTo: weights)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data([0]))
        try handle.close()
        do {
            _ = try await open(f.index, embedder: copy)
            XCTFail("must throw")
        } catch XtrieverError.Model(_, let message) {
            XCTAssertTrue(message.contains(weightsName), message)
            XCTAssertTrue(message.contains(String(pinnedBytes)), message)
        }
    }

    func testStrictModeDistinguishesASpentBudget() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: true)
        do {
            _ = try await index.search(expected.queries[0].text, options: SearchOptions(k: 10, maxTimeMs: 0, strict: true))
            XCTFail("must throw")
        } catch XtrieverError.BudgetExhausted(let message) {
            XCTAssertFalse(message.isEmpty)
        }
    }
}
