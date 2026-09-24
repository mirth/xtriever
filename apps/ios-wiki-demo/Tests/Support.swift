import Foundation
import XCTest
import Xtriever
@testable import XtrieverWikiDemo

/// Resource lookup, golden decoding and polling helpers for the demo app's tests (Feature 009).
/// The goldens are the 007 fixture's (`XtrieverData/fixtures/expected.json`); the decoding
/// structs are copied from the package's test support — the package's test target is not a
/// product this app can import.
enum Support {
    static func skipUnlessFixture() throws -> (index: URL, expected: Expected) {
        guard HarnessResources.modelsAreBundled, HarnessResources.fixtureIsBundled,
              let i = HarnessResources.fixtureIndexDirectory, let e = HarnessResources.fixtureExpected
        else { throw XCTSkip("fixture + models not bundled — scripts/build-ios-package.sh --with-models --with-fixtures --demo") }
        return (i, try JSONDecoder().decode(Expected.self, from: Data(contentsOf: e)))
    }

    static func skipUnlessWikipedia() throws {
        guard HarnessResources.modelsAreBundled, HarnessResources.wikipediaIsBundled
        else { throw XCTSkip("Wikipedia index + models not bundled — scripts/build-ios-package.sh --with-models --with-wiki (or --with-wiki-slice) --demo") }
    }

    /// Poll a main-actor condition until it holds or the timeout passes.
    @MainActor
    static func waitUntil(_ timeout: TimeInterval = 60, _ condition: @MainActor () -> Bool) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition() {
            if Date() > deadline { throw WaitTimeout() }
            try await Task.sleep(nanoseconds: 20_000_000)
        }
    }
    struct WaitTimeout: Error {}

    /// Ids, order and score bits exactly (SC-003): the app must not transform the engine's hits.
    static func assertHitsEqualGoldens(_ hits: [Hit], _ want: Expected.GoldenResponse, _ label: String,
                                       file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(hits.map(\.externalId), want.hits.map(\.externalId), "\(label): ids/order", file: file, line: line)
        for (g, w) in zip(hits, want.hits) {
            XCTAssertEqual(String(format: "%016llx", g.score.bitPattern), w.scoreBits, "\(label) \(w.externalId): score bits", file: file, line: line)
            XCTAssertEqual(g.rerankScore.map { String(format: "%08x", $0.bitPattern) }, w.rerankScoreBits, "\(label) \(w.externalId): rerank bits", file: file, line: line)
            XCTAssertEqual(g.explain?.bm25Score.map { String(format: "%08x", $0.bitPattern) }, w.bm25ScoreBits, "\(label) \(w.externalId): bm25 bits", file: file, line: line)
            XCTAssertEqual(g.explain?.denseScore.map { String(format: "%08x", $0.bitPattern) }, w.denseScoreBits, "\(label) \(w.externalId): dense bits", file: file, line: line)
        }
    }

    struct Expected: Decodable {
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
        struct GoldenRerank: Decodable { let candidates: UInt32; let scored: UInt32 }
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
        struct GoldenResponse: Decodable { let hits: [GoldenHit]; let stages: GoldenStages }
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
        let queries: [Query]
    }
}
