import XCTest
import Xtriever

/// US2 scenarios 1, 4, 5 — searches never block the caller, serialise per handle, and survive
/// cancellation (SC-002).
final class AsyncTests: XCTestCase {
    /// A search of ≥ 1 s at full depth must not stall the main thread: a 50 ms timer on the main
    /// run loop keeps ≥ 90 % of its expected ticks.
    @MainActor
    func testMainThreadTimerKeepsItsCadenceDuringASearch() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: true)
        let q = expected.queries[0]
        var ticks = 0
        let timer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { _ in ticks += 1 }
        let start = Date()
        var searches = 0
        // Full-depth searches back to back until at least a second of Rust work has run on the
        // queue — the fixture is small, so one search may finish in well under a second.
        repeat {
            _ = try await index.search(q.text, options: SearchOptions(k: 20, rerankDepth: 20, explain: false))
            searches += 1
        } while Date().timeIntervalSince(start) < 1.0 && searches < 20
        let elapsed = Date().timeIntervalSince(start)
        timer.invalidate()
        XCTAssertGreaterThanOrEqual(elapsed, 1.0, "\(searches) searches did not reach a second of work")
        let expectedTicks = Int(elapsed / 0.05)
        XCTAssertGreaterThanOrEqual(Double(ticks), 0.9 * Double(expectedTicks),
                                    "main thread ticked \(ticks) of \(expectedTicks) during a \(elapsed) s search")
    }

    func testBackToBackSearchesOnOneHandleSerialiseAndReturnTheirOwnHits() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: false)
        let (a, b) = (expected.queries[0], expected.queries[1])
        async let ra = index.search(a.text, options: SearchOptions(k: a.k, rerankDepth: a.rerankDepth, explain: true))
        async let rb = index.search(b.text, options: SearchOptions(k: b.k, rerankDepth: b.rerankDepth, explain: true))
        let (resA, resB) = try await (ra, rb)
        XCTAssertEqual(resA.hits.map(\.externalId), a.withoutReranker.hits.map(\.externalId))
        XCTAssertEqual(resB.hits.map(\.externalId), b.withoutReranker.hits.map(\.externalId))
    }

    func testACancelledTaskLeavesTheHandleUsable() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: true)
        let q = expected.queries[0]
        let task = Task { try await index.search(q.text, options: SearchOptions(k: 20, rerankDepth: 20)) }
        try await Task.sleep(nanoseconds: 10_000_000)
        task.cancel()
        _ = await task.result   // whatever it produced is discarded
        let r = try await index.search(q.text, options: SearchOptions(k: q.k, rerankDepth: q.rerankDepth, explain: true))
        Support.assertParity(r, q.withReranker, "after cancellation")
    }
}
