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
    /// The article limit of a slice (`xtriever wiki build --limit N`); absent on the whole edition.
    let partial: Int?
    let counts: Counts
    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case corpusIdentity = "corpus_identity"
        case snapshot, partial, counts
    }

    /// About's line for a slice; nil on the whole edition.
    var sliceNote: String? { partial.map { "first \($0) articles, not the whole edition" } }

    /// A measured run's `corpus`: a slice's numbers must never read as the whole edition's.
    var runLabel: String { partial.map { "wikipedia, first \($0) articles" } ?? "wikipedia" }

    static func load(from indexDir: URL) throws -> CorpusSidecar {
        try JSONDecoder().decode(CorpusSidecar.self, from: Data(contentsOf: indexDir.appendingPathComponent("corpus.json")))
    }
}
