import Foundation
import XCTest
import Xtriever
@testable import XtrieverWikiDemo

/// The full app on a device (spec US4 / SC-001, SC-005): `DemoModel` over the 20 measurement
/// queries, footprint against the 600 MB ceiling (ADR-0010), fused and re-ranked wall times
/// with medians and maxima, in the 007/008 run-record shape so `scripts/extract-device-run.py`
/// works unchanged. Skips unless the Wikipedia index is bundled; `untested` off a device.
@MainActor
final class DemoMeasurementTests: XCTestCase {
    static let ceilingBytes: UInt64 = 600 * 1_000_000

    struct RunRecord: Codable {
        struct Build: Codable {
            let configuration: String
            /// `RAYON_NUM_THREADS` if set, else the active processor count — what candle sizes its
            /// pool from when the variable is absent (004 research D3); the engine exposes no
            /// thread count on the wire, so this is the documented default, not a readback.
            let effectiveThreads: Int
            let threadSource: String
            let loadPath: String
            let isSimulator: Bool
        }
        struct IndexIdentity: Codable { let documents: UInt64; let formatVersion: UInt32; let embedderFingerprint: String; let rerankerModelId: String?; let bytes: UInt64 }
        struct Footprint: Codable {
            let baselineBytes: UInt64; let afterOpenBytes: UInt64; let sampledMaxBytes: UInt64; let ledgerPeakBytes: UInt64
            let peakBytes: UInt64; let peakMethod: String; let observedLimitBytes: UInt64; let ceilingBytes: UInt64; let verdict: String
        }
        struct QueryRun: Codable {
            let id: String; let fusedMs: UInt64; let rerankedMs: UInt64?; let engineFusedMs: UInt64; let engineRerankedMs: UInt64?
            let hits: Int; let rerankCandidates: UInt32?; let rerankScored: UInt32?; let footprintAfterBytes: UInt64
        }
        struct Latency: Codable { let medianFusedMs: Double; let maxFusedMs: UInt64; let medianRerankedMs: Double; let maxRerankedMs: UInt64; let medianTotalMs: Double; let maxTotalMs: UInt64 }
        let schemaVersion: Int; let feature: String; let corpus: String; let openedInPlace: Bool
        let device: String; let os: String; let thermalState: String; let recordedAt: String
        let build: Build; let index: IndexIdentity; let openMs: UInt64; let warmMs: UInt64?
        let embedderLoadMs: UInt64; let rerankerLoadMs: UInt64?
        let settings: Settings; let footprint: Footprint; let queries: [QueryRun]; let latency: Latency; let notes: [String]
    }

    private struct MeasurementQuery: Decodable { let id: String; let text: String }

    func testMeasureTheDemo() async throws {
        try Support.skipUnlessWikipedia()
        let queriesURL = try XCTUnwrap(HarnessResources.wikipediaQueries)
        let queries = try JSONDecoder().decode([MeasurementQuery].self, from: Data(contentsOf: queriesURL))
        #if targetEnvironment(simulator)
        let isSimulator = true
        #else
        let isSimulator = false
        #endif
        var notes: [String] = []
        if Measure.isDebugBuild { notes.append("Debug build — numbers are not a measurement") }
        if isSimulator { notes.append("simulator — footprint and timings are not device numbers") }

        let baseline = Measure.snapshot()
        let model = DemoModel(settings: Settings())
        await model.start()
        guard case .ready(let info) = model.preparation else { return XCTFail("\(model.preparation)") }
        let afterOpen = Measure.snapshot()
        var samples = [baseline, afterOpen]
        var runs: [RunRecord.QueryRun] = []
        for q in queries {
            model.submit(q.text)
            try await Support.waitUntil(120) { if case .done = model.search?.phase { return true }; if case .failed = model.search?.phase { return true }; return false }
            // Only a completed, re-ranked query is a data point; anything else is a failed run,
            // never a query that silently contributes zero to the totals.
            guard let s = model.search, case .done = s.phase, let fused = s.fused, s.reranked != nil, s.rerankedMs != nil else {
                return XCTFail("\(q.id): expected .done with a re-ranked response, got \(String(describing: model.search?.phase))")
            }
            let snap = Measure.snapshot(); samples.append(snap)
            runs.append(.init(id: q.id, fusedMs: s.fusedMs ?? 0, rerankedMs: s.rerankedMs, engineFusedMs: fused.elapsedMs,
                              engineRerankedMs: s.reranked?.elapsedMs, hits: (s.reranked ?? fused).hits.count,
                              rerankCandidates: s.reranked?.stages.rerank?.candidates, rerankScored: s.reranked?.stages.rerank?.scored,
                              footprintAfterBytes: snap.footprintBytes))
            if let d = s.reranked?.stages.degraded { notes.append("\(q.id): degraded \(d)") }
            if let sk = s.reranked?.stages.rerank?.skipped { notes.append("\(q.id): re-rank skipped \(sk)") }
        }
        let valid = samples.allSatisfy(\.isValid)
        let sampledMax = samples.map(\.footprintBytes).max() ?? 0
        let ledgerPeak = samples.map(\.ledgerPeakBytes).max() ?? 0
        let peak = max(sampledMax, ledgerPeak)
        let verdict: String
        if !valid { verdict = "untested (task_info failed)" }
        else if isSimulator || Measure.isDebugBuild { verdict = "untested (\(isSimulator ? "simulator" : "debug"))" }
        else { verdict = peak <= Self.ceilingBytes ? "PASS" : "FAIL" }

        func median(_ xs: [UInt64]) -> Double {
            let s = xs.sorted(); guard !s.isEmpty else { return 0 }
            return s.count % 2 == 1 ? Double(s[s.count / 2]) : Double(s[s.count / 2 - 1] + s[s.count / 2]) / 2
        }
        let fusedMs = runs.map(\.fusedMs), rerankedMs = runs.compactMap(\.rerankedMs)
        let totals = runs.map { $0.fusedMs + ($0.rerankedMs ?? 0) }
        let latency = RunRecord.Latency(medianFusedMs: median(fusedMs), maxFusedMs: fusedMs.max() ?? 0,
                                        medianRerankedMs: median(rerankedMs), maxRerankedMs: rerankedMs.max() ?? 0,
                                        medianTotalMs: median(totals), maxTotalMs: totals.max() ?? 0)
        let env = ProcessInfo.processInfo.environment
        let record = RunRecord(
            schemaVersion: 2, feature: "009-ios-wiki-demo",
            // Without its sidecar a run cannot say how much of the edition it searched.
            corpus: info.corpus?.runLabel ?? "wikipedia, no corpus sidecar (extent unknown)", openedInPlace: true,
            device: Measure.deviceModel, os: ProcessInfo.processInfo.operatingSystemVersionString,
            thermalState: Measure.thermalState, recordedAt: ISO8601DateFormatter().string(from: Date()),
            build: .init(configuration: Measure.isDebugBuild ? "Debug" : "Release",
                         effectiveThreads: env["RAYON_NUM_THREADS"].flatMap(Int.init) ?? ProcessInfo.processInfo.activeProcessorCount,
                         threadSource: env["RAYON_NUM_THREADS"] != nil ? "RAYON_NUM_THREADS" : "activeProcessorCount (candle's default when unset)",
                         loadPath: "mmap", isSimulator: isSimulator),
            index: .init(documents: info.info.documents, formatVersion: info.info.formatVersion, embedderFingerprint: info.info.embedderFingerprint,
                         rerankerModelId: info.info.rerankerModelId, bytes: info.indexBytes),
            openMs: info.openMs, warmMs: info.warmMs, embedderLoadMs: info.info.embedderLoadMs, rerankerLoadMs: info.info.rerankerLoadMs,
            settings: model.settings,
            footprint: .init(baselineBytes: baseline.footprintBytes, afterOpenBytes: afterOpen.footprintBytes, sampledMaxBytes: sampledMax,
                             ledgerPeakBytes: ledgerPeak, peakBytes: peak, peakMethod: ledgerPeak >= sampledMax ? "ledger" : "sampled",
                             observedLimitBytes: afterOpen.observedLimitBytes, ceilingBytes: Self.ceilingBytes, verdict: verdict),
            queries: runs, latency: latency, notes: notes)
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let json = try encoder.encode(record)
        print("XTRIEVER_DEVICE_RUN_BEGIN\n\(String(decoding: json, as: UTF8.self))\nXTRIEVER_DEVICE_RUN_END")
        let attachment = XCTAttachment(data: json, uniformTypeIdentifier: "public.json")
        attachment.name = "demo-run.json"; attachment.lifetime = .keepAlways
        add(attachment)
        if verdict == "FAIL" { XCTFail("peak footprint \(peak) B exceeds the 600 MB ceiling (ADR-0010) — stop and report (Rule 6)") }
        if !isSimulator && !Measure.isDebugBuild {
            XCTAssertLessThanOrEqual(latency.medianFusedMs, 1_000, "SC-001: median fused ≤ 1 s")
            XCTAssertLessThanOrEqual(latency.medianTotalMs, 3_000, "SC-001: median total ≤ 3 s")
        }
    }
}
