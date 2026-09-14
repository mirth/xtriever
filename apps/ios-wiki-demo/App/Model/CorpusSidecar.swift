import Foundation

/// `corpus.json` inside the 008 index directory (008 data-model "Corpus identity") — the
/// fields the About screen shows; unknown keys are ignored.
struct CorpusSidecar: Codable, Equatable {
    struct Snapshot: Codable, Equatable {
        let edition: String
        let snapshotDate: String
        enum CodingKeys: String, CodingKey { case edition; case snapshotDate = "snapshot_date" }
    }
    struct Counts: Codable, Equatable {
        let articles: Int
        let selected: Int
        let passages: Int
    }
    let schemaVersion: Int
    let corpusIdentity: String
    let snapshot: Snapshot
    let counts: Counts
    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case corpusIdentity = "corpus_identity"
        case snapshot, counts
    }

    static func load(from indexDir: URL) throws -> CorpusSidecar {
        try JSONDecoder().decode(CorpusSidecar.self, from: Data(contentsOf: indexDir.appendingPathComponent("corpus.json")))
    }
}
