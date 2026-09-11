import XCTest
// Plain `import`, not `@testable`: testability requires `-enable-testing`, which is a Debug-only
// default, and these measurements are meaningless unless built `-configuration Release`
// (research D11). Everything the harness exposes is `public`, so nothing is lost.
import XtrieverSpike

/// User Story 3 — the on-device measurement run (FR-017 – FR-022).
///
/// Runs on a simulator too, but **simulator numbers are not results**: Apple documents that the
/// Simulator has no memory limit and never issues memory-pressure terminations, so a footprint
/// measured there says nothing about a device (research D9). The emitted record carries
/// `isSimulator` so a reader can never mistake one for the other.
///
/// Everything is emitted as one JSON blob on the console and as an `XCTAttachment`, so a run can be
/// pasted into `specs/001-ios-build-spike/report.md` without retyping numbers.
final class DeviceMeasurementTests: XCTestCase {

    /// The constitution's default on-device ceiling (Principle III, FR-020).
    private static let ceilingBytes: UInt64 = 300 * 1000 * 1000

    /// One full pass: index → query → model load (both paths) → embed, each measured separately.
    func testMeasureFullRun() throws {
        let corpus = try SpikeHarness.loadCorpus()
        var measurements: [Measure.Measurement] = []
        var notes: [String] = []

        // Every condition a *valid device measurement* requires. Any unmet entry makes the run
        // UNTESTED rather than PASS — a run that measured the wrong thing, or only part of it, must
        // never serialize as a result (FR-028).
        var unmet: [String] = []

        if Measure.isDebugBuild {
            unmet.append("build is Debug — rerun with -configuration Release (research D11)")
        }
        #if targetEnvironment(simulator)
        unmet.append("running on the Simulator — Apple enforces no memory limit here, so the "
                     + "footprint is not a device result (research D9)")
        let isSimulator = true
        #else
        let isSimulator = false
        #endif

        // We can validate the *input* but not candle's effective thread count: reading that would
        // need a fourth exported function, and FR-006 caps the FFI at three. Note the variable —
        // candle 0.9.2 reads RAYON_NUM_THREADS, not CANDLE_NUM_THREADS (research D15).
        let rayonThreads = ProcessInfo.processInfo.environment["RAYON_NUM_THREADS"] ?? "<unset>"
        if rayonThreads != "1" {
            unmet.append("RAYON_NUM_THREADS is \(rayonThreads), expected 1 — candle would size its "
                         + "pool from the core count, perturbing footprint and summation order")
        }

        let baseline = Measure.snapshot()

        // --- index -------------------------------------------------------------------------
        let indexDir = try SpikeHarness.makeIndexDirectory()
        defer { try? FileManager.default.removeItem(at: indexDir) }
        let documents = corpus.documents.map {
            SpikeDocument(externalId: $0.externalId, text: $0.text)
        }
        let (outcome, indexM) = try Measure.measure("index") {
            try spikeIndex(indexDir: indexDir.path, documents: documents)
        }
        measurements.append(indexM)
        XCTAssertEqual(outcome.documentsIndexed, 1000)
        // If this is not 1, the ranking comparison below is not meaningful and the harness should
        // say so rather than report a mismatch (research D5).
        XCTAssertEqual(outcome.segmentCount, 1, "expected one segment; the golden is not comparable otherwise")

        // --- query -------------------------------------------------------------------------
        let (hits, queryM) = try Measure.measure("query") {
            try spikeQuery(indexDir: indexDir.path, query: corpus.query, k: 10)
        }
        measurements.append(queryM)

        // --- embed, once per load path (ADR-0002) ------------------------------------------
        var embeddings: [String: [Float]] = [:]
        if SpikeHarness.modelIsAvailable, let modelDir = SpikeHarness.modelDirectory {
            for (label, path) in [("buffered", LoadPath.buffered), ("mmapped", LoadPath.mmapped)] {
                let (vector, m) = try Measure.measure("embed", loadPath: label) {
                    try spikeEmbed(modelDir: modelDir.path, sentence: corpus.sentence, loadPath: path)
                }
                measurements.append(m)
                embeddings[label] = vector
            }
            // ADR-0002 condition 4. Bit-exact: identical bytes through identical arithmetic, so any
            // difference is a defect in the mmap path, not tolerable drift.
            if let b = embeddings["buffered"], let m = embeddings["mmapped"] {
                XCTAssertEqual(b.count, m.count)
                for (i, (x, y)) in zip(b, m).enumerated() {
                    XCTAssertEqual(x.bitPattern, y.bitPattern, "load paths differ at dimension \(i)")
                }
            }
        } else {
            unmet.append("model not bundled — the embedding was not measured at all; rebuild with "
                         + "scripts/build-ios-harness.sh --with-model")
        }

        // --- oracles, in the order that keeps causes distinguishable ------------------------
        try verifyRanking(hits)
        if let vector = embeddings["buffered"] { try verifyEmbedding(vector) }

        // --- verdict ------------------------------------------------------------------------
        // The run's peak is the largest lifetime high-water mark observed. Per-row values are
        // cumulative, so only their maximum is meaningful as a verdict input.
        let peak = measurements.map(\.cumulativePeakBytes).max() ?? 0

        // A failed task_info reports zero, and a maximum taken over zeros would PASS the ceiling
        // while measuring nothing. Refuse to produce a verdict at all in that case (FR-028).
        // The baseline feeds `baselineFootprintBytes` and the derived device limit, so a failure
        // there taints the record even if every measured operation succeeded.
        let memoryValid = baseline.isValid
            && !measurements.isEmpty
            && measurements.allSatisfy(\.memoryIsValid)
        if !memoryValid {
            unmet.append("task_info failed — footprint readings are not real values")
        }
        let record = DeviceRunRecord(
            runId: UUID().uuidString,
            deviceModel: Measure.deviceModel,
            systemVersion: ProcessInfo.processInfo.operatingSystemVersionString,
            isSimulator: isSimulator,
            buildConfiguration: Measure.isDebugBuild ? "Debug" : "Release",
            thermalState: Measure.thermalState,
            rayonNumThreads: rayonThreads,
            baselineFootprintBytes: baseline.footprintBytes,
            observedMemoryLimitBytes: baseline.observedLimitBytes,
            peakFootprintBytes: peak,
            ceilingBytes: Self.ceilingBytes,
            // PASS requires *every* prerequisite, not just a number under the ceiling. Without
            // this a Simulator run, a Debug run, or a run with the thread pool unpinned would all
            // serialize PASS while being explicitly non-results.
            verdict: unmet.isEmpty ? (peak <= Self.ceilingBytes ? "PASS" : "FAIL") : "UNTESTED",
            measurements: measurements,
            hits: hits.prefix(3).map { "\($0.externalId)@\($0.score)" },
            notes: notes + unmet.map { "UNMET PREREQUISITE: \($0)" }
        )
        try emit(record)

        // The verdict is recorded either way — a failure here is a *finding*, not a reason to
        // shrink the corpus and try again (FR-028).
        // On the Simulator this test is validating the harness, and UNTESTED is the correct and
        // expected outcome — so an unmet prerequisite is not a failure there. On a device the
        // operator asked for a measurement, so anything short of one is a loud failure.
        if isSimulator {
            XCTAssertFalse(unmet.isEmpty, "a Simulator run must never claim to be a device result")
        } else {
            XCTAssertTrue(
                unmet.isEmpty,
                "device run did not meet its prerequisites, so it produced no measurement:\n  "
                    + unmet.joined(separator: "\n  ")
            )
            XCTAssertLessThanOrEqual(
                peak, Self.ceilingBytes,
                "peak footprint \(record.peakMiB) MiB exceeds the \(Self.ceilingMiB) MiB ceiling"
            )
        }
    }

    // MARK: - Oracles

    /// FR-014 — bit-exact against the host-minted golden.
    private func verifyRanking(_ hits: [RankedHit]) throws {
        guard let dir = SpikeHarness.fixturesDirectory else { throw XCTSkip("no fixtures") }
        let data = try Data(contentsOf: dir.appendingPathComponent("ranking.json"))
        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              (json["status"] as? String) == "minted",
              let expected = json["hits"] as? [[String: Any]] else {
            throw XCTSkip("ranking.json is not minted")
        }
        XCTAssertEqual(hits.count, expected.count)
        for (got, want) in zip(hits, expected) {
            XCTAssertEqual(got.externalId, want["external_id"] as? String)
            // Compare bit patterns, not decimals: a ULP lost in JSON round-tripping would otherwise
            // be written up as an iOS determinism finding.
            XCTAssertEqual(got.score.bitPattern, UInt32(want["score_bits"] as? UInt ?? 0),
                           "score for \(got.externalId) is not bit-identical to the host")
        }
    }

    /// FR-015 — within the tolerance the spec states.
    private func verifyEmbedding(_ vector: [Float]) throws {
        guard let dir = SpikeHarness.fixturesDirectory else { throw XCTSkip("no fixtures") }
        let data = try Data(contentsOf: dir.appendingPathComponent("embedding.json"))
        guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let reference = (json["vector"] as? [Double])?.map(Float.init),
              let cosineMin = json["cosine_min"] as? Double,
              let maxAbsDiff = json["max_abs_diff"] as? Double else {
            return XCTFail("embedding.json is malformed")
        }
        XCTAssertEqual(vector.count, reference.count)
        var dot = 0.0, na = 0.0, nb = 0.0, worst = 0.0
        for (x, y) in zip(vector, reference) {
            dot += Double(x) * Double(y); na += Double(x) * Double(x); nb += Double(y) * Double(y)
            worst = max(worst, abs(Double(x) - Double(y)))
        }
        let cosine = dot / (na.squareRoot() * nb.squareRoot())
        XCTAssertGreaterThanOrEqual(cosine, cosineMin, "cosine \(cosine) below floor")
        XCTAssertLessThanOrEqual(worst, maxAbsDiff, "max abs diff \(worst) above tolerance")
    }

    // MARK: - Emitting the record

    private func emit(_ record: DeviceRunRecord) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let json = try encoder.encode(record)
        let text = String(decoding: json, as: UTF8.self)
        print("=== XTRIEVER DEVICE RUN BEGIN ===\n\(text)\n=== XTRIEVER DEVICE RUN END ===")
        let attachment = XCTAttachment(data: json, uniformTypeIdentifier: "public.json")
        attachment.name = "device-run.json"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    private static var ceilingMiB: Double { Double(ceilingBytes) / (1024 * 1024) }
}

/// One device run, serialized into the report (data-model.md `DeviceRun`).
struct DeviceRunRecord: Codable {
    let runId: String
    let deviceModel: String
    let systemVersion: String
    let isSimulator: Bool
    let buildConfiguration: String
    let thermalState: String
    let rayonNumThreads: String
    let baselineFootprintBytes: UInt64
    /// The device's own limit — **not** the 300 MB constitutional ceiling. Two different thresholds.
    let observedMemoryLimitBytes: UInt64
    let peakFootprintBytes: UInt64
    let ceilingBytes: UInt64
    let verdict: String
    let measurements: [Measure.Measurement]
    let hits: [String]
    let notes: [String]

    var peakMiB: Double { Double(peakFootprintBytes) / (1024 * 1024) }
}
