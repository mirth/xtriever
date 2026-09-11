import Foundation

/// Thin Swift wrapper over the three provisional FFI operations.
///
/// This type is the durable half of the harness (FR-031). The Rust binding's *shape* is
/// deliberately throwaway, but the ability to drive it repeatably from a test — and later to wrap
/// each call in a measurement — is what a device run needs, so it lives here rather than inside a
/// test file.
public enum SpikeHarness {

    /// Resolve a directory: an explicit environment variable wins, otherwise the bundled copy.
    ///
    /// On the Simulator the host filesystem is reachable, so `XTRIEVER_*_DIR` lets the Swift tests
    /// read exactly the same bytes the Rust tests do rather than a copy that could drift. A device
    /// has no such luxury, so the same paths are staged into the test bundle and found here.
    private static func directory(env: String, bundled: String) -> URL? {
        if let path = ProcessInfo.processInfo.environment[env] {
            return URL(fileURLWithPath: path)
        }
        return bundleResources?.appendingPathComponent(bundled)
    }

    /// The staged `XtrieverData` directory inside this module's resource bundle, if present.
    ///
    /// `Bundle.module` is generated per-target and only for targets that declare resources, which
    /// is why they are declared on the library target rather than the test target — this code
    /// lives here.
    ///
    /// The directory is deliberately **not** named `Resources`: a directory with that name inside a
    /// generated `.bundle` makes codesign reject it as "bundle format unrecognized, invalid, or
    /// unsuitable", because it reads as a malformed bundle layout.
    private static var bundleResources: URL? {
        Bundle.module.url(forResource: "XtrieverData", withExtension: nil)
    }

    /// Where the committed golden fixtures live.
    public static var fixturesDirectory: URL? {
        directory(env: "XTRIEVER_FIXTURES_DIR", bundled: "fixtures")
    }

    /// Where the pinned model weights live (87.1 MiB, never committed to git).
    public static var modelDirectory: URL? {
        directory(env: "XTRIEVER_MODEL_DIR", bundled: "model")
    }

    /// Whether the weights are actually present wherever ``modelDirectory`` points.
    public static var modelIsAvailable: Bool {
        guard let dir = modelDirectory else { return false }
        return FileManager.default.fileExists(
            atPath: dir.appendingPathComponent("model.safetensors").path
        )
    }

    /// One document as the FFI expects it.
    public struct Document: Decodable {
        public let externalId: String
        public let text: String

        private enum CodingKeys: String, CodingKey {
            case externalId = "external_id"
            case text
        }
    }

    /// The parts of `corpus.json` this harness uses.
    public struct Corpus: Decodable {
        public let query: String
        public let sentence: String
        public let documents: [Document]
    }

    /// Load `corpus.json` from ``fixturesDirectory``.
    public static func loadCorpus() throws -> Corpus {
        guard let dir = fixturesDirectory else {
            throw HarnessError.missingEnvironment("XTRIEVER_FIXTURES_DIR")
        }
        let data = try Data(contentsOf: dir.appendingPathComponent("corpus.json"))
        return try JSONDecoder().decode(Corpus.self, from: data)
    }

    /// A scratch directory for one index, removed when the returned handle is discarded.
    public static func makeIndexDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("xtriever-spike-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    /// Problems originating in the harness itself, distinct from `SpikeError` from Rust.
    public enum HarnessError: Error, CustomStringConvertible {
        case missingEnvironment(String)

        public var description: String {
            switch self {
            case .missingEnvironment(let name):
                return "\(name) is not set — pass it via the xcodebuild test action's environment"
            }
        }
    }
}
