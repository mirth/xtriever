import XCTest
import Xtriever
@testable import XtrieverWikiDemo

/// `ChangeMark.compute` — a pure function of two ranked lists (data-model "ChangeMark").
final class ChangeMarkTests: XCTestCase {
    private func hit(_ id: String) -> Hit {
        Hit(externalId: id, text: id, score: 0, rerankScore: nil, chunk: nil, explain: nil)
    }
    private func hits(_ ids: [String]) -> [Hit] { ids.map(hit) }

    func testIdenticalListsAreAllSameAndNothingDropped() {
        let r = ChangeMark.compute(fused: hits(["a", "b", "c"]), reranked: hits(["a", "b", "c"]))
        XCTAssertEqual(r.marks, ["a": .same, "b": .same, "c": .same])
        XCTAssertTrue(r.dropped.isEmpty)
    }

    func testAHitRisingFourPlacesDisplacesTheOthersByOne() {
        let r = ChangeMark.compute(fused: hits(["a", "b", "c", "d", "e"]), reranked: hits(["e", "a", "b", "c", "d"]))
        XCTAssertEqual(r.marks["e"], .up(4))
        for id in ["a", "b", "c", "d"] { XCTAssertEqual(r.marks[id], .down(1), id) }
        XCTAssertTrue(r.dropped.isEmpty)
    }

    func testNewAndDropped() {
        let r = ChangeMark.compute(fused: hits(["a", "b", "c"]), reranked: hits(["a", "x", "b"]))
        XCTAssertEqual(r.marks["x"], .new)
        XCTAssertEqual(r.marks["a"], .same)
        XCTAssertEqual(r.marks["b"], .down(1))
        XCTAssertEqual(r.dropped.map(\.externalId), ["c"])
        XCTAssertEqual(Set(r.marks.keys), ["a", "x", "b"])
    }

    func testEmptyLists() {
        XCTAssertTrue(ChangeMark.compute(fused: [], reranked: []).marks.isEmpty)
        let r = ChangeMark.compute(fused: hits(["a"]), reranked: [])
        XCTAssertTrue(r.marks.isEmpty); XCTAssertEqual(r.dropped.map(\.externalId), ["a"])
        XCTAssertEqual(ChangeMark.compute(fused: [], reranked: hits(["a"])).marks, ["a": .new])
    }

    func testFixtureGoldensKeysAndDropped() throws {
        let (_, expected) = try Support.skipUnlessFixture()
        for q in expected.queries {
            let fused = hits(q.withoutReranker.hits.map(\.externalId))
            let reranked = hits(q.withReranker.hits.map(\.externalId))
            let r = ChangeMark.compute(fused: fused, reranked: reranked)
            XCTAssertEqual(Set(r.marks.keys), Set(reranked.map(\.externalId)), q.id)
            let expectedDropped = fused.map(\.externalId).filter { !reranked.map(\.externalId).contains($0) }
            XCTAssertEqual(r.dropped.map(\.externalId), expectedDropped, q.id)
        }
    }
}
