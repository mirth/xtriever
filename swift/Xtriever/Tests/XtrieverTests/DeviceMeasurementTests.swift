import Foundation
import XCTest
import Xtriever

/// US5 — the pipeline's on-device cost, measured (spec FR-013, FR-014; research D9).
///
/// Opens the SciFact index with both models, runs the 20 measurement queries at re-rank depths
/// 0 / 5 / 20, samples the process footprint around every call, checks parity against the
/// host's `expected-scifact.json`, and emits one run record (data-model "Device run record") on
/// the console and as an `XCTAttachment` for committing under `specs/007-ffi-surface/runs/`.
///
/// Runs wherever the resources are bundled (`scripts/build-ios-package.sh --with-models
/// --with-scifact`); the verdict is `PASS`/`FAIL` only on a physical device in Release with
/// valid `task_info` readings, and `untested` otherwise (a simulator number is not a device
/// number). Select the load path with `TEST_RUNNER_XTRIEVER_LOAD_PATH=buffered|mmap`.
final class DeviceMeasurementTests: XCTestCase {
    static let ceilingBytes: UInt64 = 300 * 1_000_000
    static let depths: [UInt32] = [0, 5, 20]
    static let toleranceAbs: Float = 1e-3

    // MARK: the run record

    struct RunRecord: Codable {
        struct Build: Codable {
            let configuration: String
            let rayonNumThreads: String?
            let loadPath: String
            let isSimulator: Bool
        }
        struct IndexIdentity: Codable {
            let documents: UInt64
            let formatVersion: UInt32
            let embedderFingerprint: String
            let rerankerModelId: String?
        }
        struct Footprint: Codable {
            let baselineBytes: UInt64
            let afterOpenBytes: UInt64
            let peakBytes: UInt64
            let peakMethod: String
            let sampledMaxBytes: UInt64
            let ledgerPeakBytes: UInt64
            let observedLimitBytes: UInt64
            let ceilingBytes: UInt64
            let verdict: String
        }
        struct QueryRun: Codable {
            let id: String
            let depth: UInt32
            let elapsedMs: UInt64
            let hits: Int
            let rerankCandidates: UInt32?
            let rerankScored: UInt32?
            let footprintAfterBytes: UInt64
        }
        struct Parity: Codable {
            let queriesCompared: Int
            let lexicalBitIdentical: Int
            let fusedOrderIdentical: Int
            let denseMaxAbsDiff: Float
            let rerankMaxAbsDiff: Float
            let toleranceAbs: Float
            let verdict: String
        }
        let schemaVersion: Int
        let feature: String
        let device: String
        let os: String
        let thermalState: String
        let recordedAt: String
        let build: Build
        let index: IndexIdentity
        let openMs: UInt64
        let embedderLoadMs: UInt64
        let rerankerLoadMs: UInt64?
        let footprint: Footprint
        let queries: [QueryRun]
        let perDepthMeanMs: [String: Double]
        let derivedPerPairMs: Double?
        let parity: Parity
        let notes: [String]
    }

    struct Truth: Decodable {
        struct Hit: Decodable {
            let externalId: String
            let scoreBits: String
            let rerankScoreBits: String?
            let bm25ScoreBits: String?
            let denseScoreBits: String?
            enum CodingKeys: String, CodingKey {
                case externalId = "external_id"
                case scoreBits = "score_bits"
                case rerankScoreBits = "rerank_score_bits"
                case bm25ScoreBits = "bm25_score_bits"
                case denseScoreBits = "dense_score_bits"
            }
        }
        struct Response: Decodable { let hits: [Hit] }
        struct Query: Decodable {
            let id: String
            let text: String
            let depths: [String: Response]
        }
        let queries: [Query]
    }

    struct MeasurementQuery: Decodable {
        let id: String
        let text: String
    }

    // MARK: the run

    func testMeasureSciFactRun() async throws {
        guard HarnessResources.scifactIsBundled, HarnessResources.modelsAreBundled,
              let indexDir = HarnessResources.scifactIndexDirectory,
              let queriesURL = HarnessResources.scifactQueries,
              let truthURL = HarnessResources.scifactExpected,
              let embedderDir = HarnessResources.embedderDirectory,
              let rerankerDir = HarnessResources.rerankerDirectory
        else {
            throw XCTSkip("SciFact and both models must be bundled: scripts/build-ios-package.sh --with-models --with-scifact")
        }
        let env = ProcessInfo.processInfo.environment
        let loadPath: LoadPath = env["XTRIEVER_LOAD_PATH"] == "buffered" ? .buffered : .mmap
        let loadPathName = loadPath == .buffered ? "buffered" : "mmap"
        #if targetEnvironment(simulator)
        let isSimulator = true
        #else
        let isSimulator = false
        #endif
        var notes: [String] = []
        if Measure.isDebugBuild { notes.append("Debug build — numbers are not a measurement") }
        if isSimulator { notes.append("simulator — footprint and timings are not device numbers") }

        let queries = try JSONDecoder().decode([MeasurementQuery].self, from: Data(contentsOf: queriesURL))
        let truth = try JSONDecoder().decode(Truth.self, from: Data(contentsOf: truthURL))

        // --- baseline, open ------------------------------------------------------------------
        // The bundle is read-only on a device and the lexical backend needs to open its lock
        // file for writing (report F-001): copy the index out once. Not part of the timed open.
        let writableIndex = try XtrieverIndex.writableCopy(of: indexDir, named: "scifact-measurement")
        let baseline = Measure.snapshot()
        let openStart = Measure.nowNanos()
        let index = try await XtrieverIndex.open(indexDir: writableIndex, embedderDir: embedderDir,
                                                 rerankerDir: rerankerDir, loadPath: loadPath)
        let openMs = (Measure.nowNanos() &- openStart) / 1_000_000
        let afterOpen = Measure.snapshot()
        var samples: [Measure.Snapshot] = [baseline, afterOpen]

        // --- queries × depths ----------------------------------------------------------------
        var runs: [RunRecord.QueryRun] = []
        var responses: [String: [UInt32: SearchResponse]] = [:]
        for depth in Self.depths {
            for q in queries {
                let r = try await index.search(q.text, options: SearchOptions(k: 10, rerankDepth: depth, explain: true))
                let snap = Measure.snapshot()
                samples.append(snap)
                runs.append(RunRecord.QueryRun(
                    id: q.id, depth: depth, elapsedMs: r.elapsedMs, hits: r.hits.count,
                    rerankCandidates: r.stages.rerank?.candidates, rerankScored: r.stages.rerank?.scored,
                    footprintAfterBytes: snap.footprintBytes))
                responses[q.id, default: [:]][depth] = r
                if let rr = r.stages.rerank, rr.skipped != nil { notes.append("\(q.id)@\(depth): re-rank skipped \(rr.skipped!)") }
                if r.stages.degraded != nil { notes.append("\(q.id)@\(depth): dense degraded") }
            }
        }

        // --- parity vs the host (research D9): lexical bit-exact, dense/re-rank within 1e-3 --
        // Every host query, depth and hit must have a device counterpart, and a score present on
        // one side and absent on the other is a mismatch, not a skip: silence here would let an
        // incomplete run pass as parity.
        var compared = 0, lexicalOk = 0, fusedOk = 0
        var denseMax: Float = 0, rerankMax: Float = 0
        var incomplete: [String] = []
        for tq in truth.queries {
            guard let byDepth = responses[tq.id] else { incomplete.append("\(tq.id): not searched"); continue }
            compared += 1
            var lexicalIdentical = true
            var fusedIdentical = true
            for (depthKey, want) in tq.depths {
                guard let depth = UInt32(depthKey), let got = byDepth[depth] else {
                    incomplete.append("\(tq.id)@\(depthKey): no response"); continue
                }
                if got.hits.count != want.hits.count {
                    incomplete.append("\(tq.id)@\(depth): \(got.hits.count) hits, host has \(want.hits.count)")
                }
                if depth == 0 {
                    fusedIdentical = fusedIdentical && got.hits.map(\.externalId) == want.hits.map(\.externalId)
                }
                for (rank, (g, w)) in zip(got.hits, want.hits).enumerated() {
                    let gBm25 = g.explain?.bm25Score.map { String(format: "%08x", $0.bitPattern) }
                    if gBm25 != w.bm25ScoreBits { lexicalIdentical = false }
                    switch (w.denseScoreBits, g.explain?.denseScore) {
                    case let (wd?, gd?): denseMax = max(denseMax, abs(gd - Float(bitPattern: UInt32(wd, radix: 16) ?? 0)))
                    case (nil, nil): break
                    default: incomplete.append("\(tq.id)@\(depth) hit \(rank): dense score present on one side only")
                    }
                    switch (w.rerankScoreBits, g.rerankScore) {
                    case let (wr?, gr?): rerankMax = max(rerankMax, abs(gr - Float(bitPattern: UInt32(wr, radix: 16) ?? 0)))
                    case (nil, nil): break
                    default: incomplete.append("\(tq.id)@\(depth) hit \(rank): re-rank score present on one side only")
                    }
                }
            }
            if lexicalIdentical { lexicalOk += 1 }
            if fusedIdentical { fusedOk += 1 }
        }
        let parityOk = compared > 0 && compared == truth.queries.count && incomplete.isEmpty
            && lexicalOk == compared && fusedOk == compared
            && denseMax <= Self.toleranceAbs && rerankMax <= Self.toleranceAbs
        notes.append(contentsOf: incomplete.map { "parity: " + $0 })
        XCTAssertEqual(compared, truth.queries.count, "every host query must have been searched")
        XCTAssertTrue(incomplete.isEmpty, "parity comparison incomplete: \(incomplete)")
        XCTAssertEqual(lexicalOk, compared, "lexical scores must be bit-identical to the host's")
        XCTAssertEqual(fusedOk, compared, "the fused order at depth 0 must equal the host's")
        XCTAssertLessThanOrEqual(denseMax, Self.toleranceAbs, "dense scores within tolerance of the host's")
        XCTAssertLessThanOrEqual(rerankMax, Self.toleranceAbs, "re-rank scores within tolerance of the host's")

        // --- footprint verdict (001's rules: conservative peak, no verdict on invalid readings) -
        let valid = samples.allSatisfy(\.isValid)
        let sampledMax = samples.map(\.footprintBytes).max() ?? 0
        let ledgerPeak = samples.map(\.ledgerPeakBytes).max() ?? 0
        let peak = max(sampledMax, ledgerPeak)
        let verdict: String
        if !valid { verdict = "untested (task_info failed)" }
        else if isSimulator || Measure.isDebugBuild { verdict = "untested (\(isSimulator ? "simulator" : "debug"))" }
        else { verdict = peak <= Self.ceilingBytes ? "PASS" : "FAIL" }

        // --- per-depth means and the derived per-pair cost ---------------------------------
        var means: [String: Double] = [:]
        for depth in Self.depths {
            let ms = runs.filter { $0.depth == depth }.map { Double($0.elapsedMs) }
            means[String(depth)] = ms.reduce(0, +) / Double(max(ms.count, 1))
        }
        let perPair: Double? = {
            guard let d20 = means["20"], let d0 = means["0"] else { return nil }
            return (d20 - d0) / 20
        }()

        let formatter = ISO8601DateFormatter()
        let record = RunRecord(
            schemaVersion: 1, feature: "007-ffi-surface",
            device: Measure.deviceModel,
            os: ProcessInfo.processInfo.operatingSystemVersionString,
            thermalState: Measure.thermalState,
            recordedAt: formatter.string(from: Date()),
            build: .init(configuration: Measure.isDebugBuild ? "Debug" : "Release",
                         rayonNumThreads: env["RAYON_NUM_THREADS"], loadPath: loadPathName, isSimulator: isSimulator),
            index: .init(documents: index.info.documents, formatVersion: index.info.formatVersion,
                         embedderFingerprint: index.info.embedderFingerprint, rerankerModelId: index.info.rerankerModelId),
            openMs: openMs, embedderLoadMs: index.info.embedderLoadMs, rerankerLoadMs: index.info.rerankerLoadMs,
            footprint: .init(baselineBytes: baseline.footprintBytes, afterOpenBytes: afterOpen.footprintBytes,
                             peakBytes: peak, peakMethod: ledgerPeak >= sampledMax ? "ledger" : "sampled",
                             sampledMaxBytes: sampledMax, ledgerPeakBytes: ledgerPeak,
                             observedLimitBytes: afterOpen.observedLimitBytes, ceilingBytes: Self.ceilingBytes, verdict: verdict),
            queries: runs, perDepthMeanMs: means, derivedPerPairMs: perPair,
            parity: .init(queriesCompared: compared, lexicalBitIdentical: lexicalOk, fusedOrderIdentical: fusedOk,
                          denseMaxAbsDiff: denseMax, rerankMaxAbsDiff: rerankMax, toleranceAbs: Self.toleranceAbs,
                          verdict: parityOk ? "PASS" : "FAIL"),
            notes: notes)
        try emit(record)
        if verdict == "FAIL" {
            XCTFail("peak footprint \(peak) B exceeds the 300 MB ceiling — stop and report (Rule 6)")
        }
    }

    private func emit(_ record: RunRecord) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let json = try encoder.encode(record)
        print("XTRIEVER_DEVICE_RUN_BEGIN\n\(String(decoding: json, as: UTF8.self))\nXTRIEVER_DEVICE_RUN_END")
        let attachment = XCTAttachment(data: json, uniformTypeIdentifier: "public.json")
        attachment.name = "device-run.json"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
