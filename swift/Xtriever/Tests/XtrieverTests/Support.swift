import Foundation
import XCTest
import Xtriever

/// Resource lookup and golden parsing shared by the Feature 007 Swift suite.
///
/// Every resource is staged into the library bundle by `scripts/build-ios-package.sh`; a test
/// that needs one it cannot find skips with the flag that would have staged it, so a missing
/// resource is never a silent pass.
enum Support {
    static func models() throws -> (embedder: URL, reranker: URL) {
        guard HarnessResources.modelsAreBundled,
              let e = HarnessResources.embedderDirectory,
              let r = HarnessResources.rerankerDirectory
        else { throw XCTSkip("models not bundled — rebuild with scripts/build-ios-package.sh --with-models") }
        return (e, r)
    }

    static func fixture() throws -> (index: URL, expected: Expected) {
        guard HarnessResources.fixtureIsBundled,
              let i = HarnessResources.fixtureIndexDirectory,
              let e = HarnessResources.fixtureExpected
        else { throw XCTSkip("fixture index not bundled — rebuild with scripts/build-ios-package.sh --with-fixtures") }
        let data = try Data(contentsOf: e)
        return (i, try JSONDecoder().decode(Expected.self, from: data))
    }

    /// Open the fixture index with both models (buffered — the simulator has no reason to map).
    static func openFixture(withReranker: Bool = true, loadPath: LoadPath = .buffered) async throws -> (XtrieverIndex, Expected) {
        let m = try models()
        let f = try fixture()
        // Bundles are read-only on a device (report F-001); the copy is a no-op on re-runs.
        let writable = try XtrieverIndex.writableCopy(of: f.index, named: "fixture")
        let index = try await XtrieverIndex.open(
            indexDir: writable, embedderDir: m.embedder,
            rerankerDir: withReranker ? m.reranker : nil, loadPath: loadPath)
        return (index, f.expected)
    }

    /// A writable copy of a directory tree, for tamper tests.
    static func copy(_ src: URL) throws -> URL {
        let dst = FileManager.default.temporaryDirectory
            .appendingPathComponent("xtriever-\(UUID().uuidString)")
        try FileManager.default.copyItem(at: src, to: dst)
        return dst
    }

    // MARK: expected.json (data-model "Parity goldens")

    struct Expected: Decodable {
        struct Info: Decodable {
            let documents: UInt64
            let formatVersion: UInt32
            let embedderFingerprint: String
            let rerankerModelId: String?
            let candidateDepth: UInt32
            let rerankDepth: UInt32
            let rrfK: UInt32
            enum CodingKeys: String, CodingKey {
                case documents
                case formatVersion = "format_version"
                case embedderFingerprint = "embedder_fingerprint"
                case rerankerModelId = "reranker_model_id"
                case candidateDepth = "candidate_depth"
                case rerankDepth = "rerank_depth"
                case rrfK = "rrf_k"
            }
        }
        struct GoldenHit: Decodable {
            let externalId: String
            let scoreBits: String
            let rerankScoreBits: String?
            let rerankRank: UInt32?
            let bm25ScoreBits: String?
            let denseScoreBits: String?
            enum CodingKeys: String, CodingKey {
                case externalId = "external_id"
                case scoreBits = "score_bits"
                case rerankScoreBits = "rerank_score_bits"
                case rerankRank = "rerank_rank"
                case bm25ScoreBits = "bm25_score_bits"
                case denseScoreBits = "dense_score_bits"
            }
        }
        struct GoldenRerank: Decodable {
            let candidates: UInt32
            let scored: UInt32
        }
        struct GoldenStages: Decodable {
            let lexicalCandidates: UInt32
            let denseCandidates: UInt32?
            let degraded: Bool
            let rerank: GoldenRerank?
            enum CodingKeys: String, CodingKey {
                case lexicalCandidates = "lexical_candidates"
                case denseCandidates = "dense_candidates"
                case degraded, rerank
            }
        }
        struct GoldenResponse: Decodable {
            let hits: [GoldenHit]
            let stages: GoldenStages
        }
        struct Query: Decodable {
            let id: String
            let text: String
            let k: UInt32
            let rerankDepth: UInt32
            let withReranker: GoldenResponse
            let withoutReranker: GoldenResponse
            enum CodingKeys: String, CodingKey {
                case id, text, k
                case rerankDepth = "rerank_depth"
                case withReranker = "with_reranker"
                case withoutReranker = "without_reranker"
            }
        }
        let generatedBy: String
        let info: Info
        let queries: [Query]
        enum CodingKeys: String, CodingKey {
            case generatedBy = "generated_by"
            case info, queries
        }
    }

    /// Compare a live response with a golden one: ids and order exactly, scores by bit pattern.
    static func assertParity(_ got: SearchResponse, _ want: Expected.GoldenResponse, _ label: String,
                             file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(got.hits.map(\.externalId), want.hits.map(\.externalId), "\(label): ids/order", file: file, line: line)
        for (g, w) in zip(got.hits, want.hits) {
            XCTAssertEqual(String(format: "%016llx", g.score.bitPattern), w.scoreBits, "\(label) \(w.externalId): score bits", file: file, line: line)
            XCTAssertEqual(g.rerankScore.map { String(format: "%08x", $0.bitPattern) }, w.rerankScoreBits, "\(label) \(w.externalId): rerank bits", file: file, line: line)
            XCTAssertEqual(g.explain?.rerankRank, w.rerankRank, "\(label) \(w.externalId): rerank rank", file: file, line: line)
            XCTAssertEqual(g.explain?.bm25Score.map { String(format: "%08x", $0.bitPattern) }, w.bm25ScoreBits, "\(label) \(w.externalId): bm25 bits", file: file, line: line)
            XCTAssertEqual(g.explain?.denseScore.map { String(format: "%08x", $0.bitPattern) }, w.denseScoreBits, "\(label) \(w.externalId): dense bits", file: file, line: line)
        }
        XCTAssertEqual(got.stages.lexicalCandidates, want.stages.lexicalCandidates, label, file: file, line: line)
        XCTAssertEqual(got.stages.denseCandidates, want.stages.denseCandidates, label, file: file, line: line)
        XCTAssertEqual(got.stages.degraded != nil, want.stages.degraded, label, file: file, line: line)
        XCTAssertEqual(got.stages.rerank?.candidates, want.stages.rerank?.candidates, label, file: file, line: line)
        XCTAssertEqual(got.stages.rerank?.scored, want.stages.rerank?.scored, label, file: file, line: line)
    }
}
