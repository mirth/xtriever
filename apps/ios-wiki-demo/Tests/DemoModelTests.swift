import XCTest
import Xtriever
@testable import XtrieverWikiDemo

/// `DemoModel` against the 007 fixture and its goldens (spec US1, US2, US4; SC-002, SC-003,
/// SC-006). The engine is proven elsewhere; this proves the app sequences it faithfully.
@MainActor
final class DemoModelTests: XCTestCase {
    private func readyModel(settings: Settings = Settings()) async throws -> (DemoModel, Support.Expected) {
        let (_, expected) = try Support.skipUnlessFixture()
        let model = DemoModel(settings: settings, corpus: .fixture)
        await model.start()
        guard case .ready = model.preparation else {
            XCTFail("not ready: \(model.preparation)")
            throw Support.WaitTimeout()
        }
        return (model, expected)
    }

    private func waitDone(_ model: DemoModel) async throws {
        try await Support.waitUntil(120) {
            if let s = model.search {
                switch s.phase { case .done, .empty, .failed: return true; default: return false }
            }
            return false
        }
    }

    func testPreparationReachesReadyWithTimings() async throws {
        let (model, _) = try await readyModel()
        guard case .ready(let info) = model.preparation else { return XCTFail() }
        XCTAssertEqual(info.info.documents, 40)
        XCTAssertGreaterThan(info.openMs, 0)
        XCTAssertGreaterThan(info.warmMs, 0)
        XCTAssertNil(info.corpus, "the fixture has no corpus sidecar")
        XCTAssertNil(info.attribution)
        XCTAssertTrue(info.indexName.contains("fixture"))
        XCTAssertFalse(info.isWikipedia)
        XCTAssertEqual(info.title, "Fixture")
    }

    func testFusedThenRerankedEqualTheGoldens() async throws {
        // The goldens were minted at re-rank depth 5 (007): match it.
        let (model, expected) = try await readyModel(settings: Settings(rerankDepth: 5, budgetMs: nil, strict: false))
        for q in expected.queries {
            model.submit(q.text)
            if q.text.trimmingCharacters(in: .whitespaces).isEmpty {
                // The fixture's empty query (007 F-004): the app never calls the engine for it.
                XCTAssertEqual(model.search?.phase, .empty, q.id)
                continue
            }
            try await Support.waitUntil(60) { model.search?.fused != nil || model.search?.phase.isFailed == true }
            guard let fused = model.search?.fused else { return XCTFail("\(q.id): no fused response: \(String(describing: model.search?.phase))") }
            Support.assertHitsEqualGoldens(fused.hits, q.withoutReranker, "\(q.id) fused")
            try await waitDone(model)
            guard let s = model.search, case .done = s.phase, let reranked = s.reranked else { return XCTFail("\(q.id): \(String(describing: model.search?.phase))") }
            Support.assertHitsEqualGoldens(reranked.hits, q.withReranker, "\(q.id) re-ranked")
            XCTAssertEqual(s.marks.count, reranked.hits.count, q.id)
            XCTAssertEqual(s.query, q.text)
            XCTAssertNotNil(s.fusedMs); XCTAssertNotNil(s.rerankedMs); XCTAssertNotNil(s.footprintBytes)
        }
    }

    func testAnEmptyQueryIsEmptyNotAnError() async throws {
        let (model, expected) = try await readyModel()
        model.submit(expected.queries[0].text)
        try await waitDone(model)
        model.submit("   \n ")
        XCTAssertEqual(model.search?.phase, .empty)
        XCTAssertNil(model.search?.fused, "a previous result must not linger under an empty query")
    }

    func testDepthZeroRunsOnlyTheFusedSearch() async throws {
        let (model, expected) = try await readyModel(settings: Settings(rerankDepth: 0, budgetMs: nil, strict: false))
        model.submit(expected.queries[1].text)
        try await waitDone(model)
        guard let s = model.search else { return XCTFail() }
        XCTAssertEqual(s.phase, .done)
        XCTAssertNil(s.reranked)
        XCTAssertNil(s.fused?.stages.rerank, "depth 0: the engine reports no re-rank stage")
        Support.assertHitsEqualGoldens(s.fused?.hits ?? [], expected.queries[1].withoutReranker, "fused")
    }

    func testAStrictSpentBudgetIsTheEnginesErrorAndTheModelStaysUsable() async throws {
        let (model, expected) = try await readyModel(settings: Settings(rerankDepth: 20, budgetMs: 1, strict: true))
        model.submit(expected.queries[2].text)
        try await waitDone(model)
        guard let s = model.search, case .failed(let message) = s.phase else { return XCTFail("expected .failed, got \(String(describing: model.search?.phase))") }
        XCTAssertFalse(message.isEmpty)
        model.settings = Settings(rerankDepth: 5, budgetMs: nil, strict: false)
        model.submit(expected.queries[2].text)
        try await waitDone(model)
        XCTAssertEqual(model.search?.phase, .done)
    }

    func testANonStrictSpentBudgetDegradesInTheReport() async throws {
        let (model, expected) = try await readyModel(settings: Settings(rerankDepth: 20, budgetMs: 1, strict: false))
        model.submit(expected.queries[3].text)
        try await waitDone(model)
        guard let s = model.search, case .done = s.phase, let r = s.reranked else { return XCTFail() }
        XCTAssertTrue(r.stages.degraded != nil || r.stages.rerank?.skipped != nil, "\(r.stages)")
        XCTAssertFalse(r.hits.isEmpty, "the lexical ranking still comes back")
    }

    func testASecondSubmissionCancelsTheFirst() async throws {
        let (model, expected) = try await readyModel(settings: Settings(rerankDepth: 5, budgetMs: nil, strict: false))
        let (a, b) = (expected.queries[4], expected.queries[6])
        for i in 0..<20 {
            model.submit(a.text)
            model.submit(b.text)
            try await waitDone(model)
            guard let s = model.search else { return XCTFail("\(i)") }
            XCTAssertEqual(s.query, b.text, "iteration \(i)")
            XCTAssertEqual(s.phase, .done, "iteration \(i): phase")
            XCTAssertEqual(s.fused?.hits.map(\.externalId), b.withoutReranker.hits.map(\.externalId), "iteration \(i): fused must be b's")
            XCTAssertEqual(s.reranked?.hits.map(\.externalId), b.withReranker.hits.map(\.externalId), "iteration \(i): re-ranked must be b's")
        }
    }

    func testMainThreadKeepsItsCadenceDuringASearch() async throws {
        let (model, expected) = try await readyModel(settings: Settings(rerankDepth: 20, budgetMs: nil, strict: false))
        var ticks = 0
        let timer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { _ in ticks += 1 }
        let start = Date()
        var searches = 0
        repeat {
            model.submit(expected.queries[searches % expected.queries.count].text)
            try await waitDone(model)
            searches += 1
        } while Date().timeIntervalSince(start) < 1.0 && searches < 20
        let elapsed = Date().timeIntervalSince(start)
        timer.invalidate()
        if elapsed < 1.0 { throw XCTSkip("\(searches) searches took \(elapsed) s; a ≥ 1 s workload cannot be produced here") }
        let expectedTicks = Int(elapsed / 0.05)
        XCTAssertGreaterThanOrEqual(Double(ticks), 0.9 * Double(expectedTicks), "main thread ticked \(ticks) of \(expectedTicks) during \(elapsed) s of searches")
    }
}
