import XCTest
import Xtriever

/// US1 scenarios 2, 3, 5 — Swift hits equal the goldens the FFI minted on the host (SC-001).
final class ParityTests: XCTestCase {
    func testHitsEqualTheGoldensWithTheReranker() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: true)
        for q in expected.queries {
            let r = try await index.search(q.text, options: SearchOptions(k: q.k, rerankDepth: q.rerankDepth, explain: true))
            Support.assertParity(r, q.withReranker, "\(q.id) with re-ranker")
            XCTAssertNotNil(r.stages.rerank, q.id)
        }
    }

    func testHitsEqualTheGoldensWithoutTheReranker() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: false)
        for q in expected.queries {
            let r = try await index.search(q.text, options: SearchOptions(k: q.k, rerankDepth: q.rerankDepth, explain: true))
            Support.assertParity(r, q.withoutReranker, "\(q.id) without re-ranker")
            XCTAssertNil(r.stages.rerank, q.id)
            XCTAssertTrue(r.hits.allSatisfy { $0.rerankScore == nil }, q.id)
        }
    }

    func testExplanationCarriesTheSevenFeatureNames() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: true)
        let q = expected.queries[0]
        let r = try await index.search(q.text, options: SearchOptions(k: q.k, rerankDepth: q.rerankDepth, explain: true))
        let features = try XCTUnwrap(r.hits.first?.explain).features()
        XCTAssertEqual(features.map(\.name),
                       ["bm25.score", "bm25.rank", "dense.score", "dense.rank", "fused.score", "rerank.score", "rerank.rank"])
        let unexplained = try await index.search(q.text, options: SearchOptions(k: q.k, rerankDepth: q.rerankDepth, explain: false))
        XCTAssertEqual(unexplained.hits.map(\.externalId), r.hits.map(\.externalId), "explanation never changes hits")
        XCTAssertTrue(unexplained.hits.allSatisfy { $0.explain == nil })
    }
}
