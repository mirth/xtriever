import XCTest
import Xtriever

/// US2 scenarios 2–3 — a time budget yields a partial re-rank, never an error, unless strict (SC-002).
final class BudgetTests: XCTestCase {
    func testAShortBudgetYieldsAPartialRerankUnderASecond() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: true)
        var partial = 0
        for q in expected.queries {
            let start = Date()
            let r = try await index.search(q.text, options: SearchOptions(k: 20, rerankDepth: 20, maxTimeMs: 200, explain: true))
            XCTAssertLessThan(Date().timeIntervalSince(start), 1.0, q.id)
            XCTAssertLessThan(r.elapsedMs, 1000, q.id)
            XCTAssertFalse(r.stages.timeLimitIgnored, q.id)
            if let rr = r.stages.rerank, rr.skipped == nil, rr.scored > 0, rr.scored < rr.candidates {
                partial += 1
                let scoredPrefix = r.hits.prefix { $0.rerankScore != nil }.count
                XCTAssertEqual(UInt32(scoredPrefix), rr.scored, "\(q.id): scored hits come first")
            }
        }
        XCTAssertGreaterThanOrEqual(partial, 1, "no query was partially re-ranked under 200 ms")
    }

    func testAZeroBudgetDegradesByDefaultAndThrowsWhenStrict() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: true)
        let q = expected.queries[0]
        let r = try await index.search(q.text, options: SearchOptions(k: 10, rerankDepth: 5, maxTimeMs: 0, explain: true))
        XCTAssertNotNil(r.stages.degraded)
        XCTAssertNotNil(r.stages.rerank?.skipped)
        XCTAssertFalse(r.hits.isEmpty, "the lexical ranking still comes back")
        do {
            _ = try await index.search(q.text, options: SearchOptions(k: 10, rerankDepth: 5, maxTimeMs: 0, strict: true))
            XCTFail("strict mode must throw")
        } catch XtrieverError.BudgetExhausted(let message) {
            XCTAssertFalse(message.isEmpty)
        }
    }
}
