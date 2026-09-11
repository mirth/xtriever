import XCTest
// Plain `import`, not `@testable`: testability requires `-enable-testing`, which is a Debug-only
// default, and these measurements are meaningless unless built `-configuration Release`
// (research D11). Everything the harness exposes is `public`, so nothing is lost.
import XtrieverSpike

/// Simulator verification for User Story 2 (FR-006 – FR-008).
///
/// These answer a different question from the Rust tests in `crates/xtriever-ffi/tests/`. Those
/// prove the *logic* is right against the goldens; these prove the **language boundary** carries
/// it — that the XCFramework links, the generated Swift sees the three operations, the data
/// survives the crossing, and a Rust error arrives as a caught Swift error rather than killing the
/// process.
final class SpikeTests: XCTestCase {

    /// FR-006, FR-007 — index the corpus and get a ranked list back across the boundary.
    func testIndexAndQueryCrossTheBoundary() throws {
        let corpus = try SpikeHarness.loadCorpus()
        XCTAssertEqual(corpus.documents.count, 1000, "the committed corpus is 1,000 documents")

        let indexDir = try SpikeHarness.makeIndexDirectory()
        defer { try? FileManager.default.removeItem(at: indexDir) }

        let documents = corpus.documents.map {
            SpikeDocument(externalId: $0.externalId, text: $0.text)
        }
        let outcome = try spikeIndex(indexDir: indexDir.path, documents: documents)

        XCTAssertEqual(outcome.documentsIndexed, 1000)
        // One segment, or the ranking's tie-break order is not comparable to the host golden at
        // all (research D5). Worth asserting here too: if the iOS build somehow picked a different
        // thread count, this is where it shows up.
        XCTAssertEqual(outcome.segmentCount, 1, "the single-threaded writer must produce one segment")

        let hits = try spikeQuery(indexDir: indexDir.path, query: corpus.query, k: 10)
        XCTAssertEqual(hits.count, 10)
        XCTAssertFalse(hits[0].externalId.isEmpty)
        XCTAssertGreaterThan(hits[0].score, 0)
        // Strictly descending: proves the ordering survived the crossing, not just the values.
        for (a, b) in zip(hits, hits.dropFirst()) {
            XCTAssertGreaterThan(a.score, b.score, "hits must come back in descending score order")
        }
    }

    /// FR-007 — a `Vec<f32>` reaches Swift as `[Float]` with the right shape.
    func testEmbeddingCrossesAsFloatArray() throws {
        guard let modelDir = SpikeHarness.modelDirectory else {
            throw XCTSkip("XTRIEVER_MODEL_DIR not set — the 87.1 MiB weights are not committed")
        }
        let corpus = try SpikeHarness.loadCorpus()

        let vector = try spikeEmbed(
            modelDir: modelDir.path, sentence: corpus.sentence, loadPath: .buffered
        )

        XCTAssertEqual(vector.count, 384, "all-MiniLM-L6-v2 produces 384 dimensions")
        XCTAssertTrue(vector.allSatisfy { $0.isFinite }, "no NaN or infinity across the boundary")
        let norm = sqrt(vector.reduce(0) { $0 + Double($1) * Double($1) })
        XCTAssertEqual(norm, 1.0, accuracy: 1e-4, "the vector should arrive L2-normalized")
    }

    /// FR-008 — **the important one.** A Rust error must arrive as a caught Swift error.
    ///
    /// A panic crossing uniffi into a non-throwing Swift function is an *uncatchable* fatal error,
    /// so the failure mode this guards against is not a wrong value but a dead process that takes
    /// the measurement run with it. If this test crashes rather than fails, some path in the Rust
    /// is panicking where it should return `Result`.
    func testRustErrorsArriveAsCaughtSwiftErrors() throws {
        let missing = FileManager.default.temporaryDirectory
            .appendingPathComponent("xtriever-does-not-exist-\(UUID().uuidString)")

        XCTAssertThrowsError(
            try spikeEmbed(modelDir: missing.path, sentence: "anything", loadPath: .buffered),
            "a missing model directory must throw, not abort"
        ) { error in
            guard case SpikeError.Model = error else {
                return XCTFail("expected SpikeError.Model, got \(error)")
            }
        }
    }

    /// FR-008 — the same guarantee on the index/query path, including the deliberate
    /// empty-result-is-an-error rule.
    func testQueryErrorsAreTypedNotFatal() throws {
        let indexDir = try SpikeHarness.makeIndexDirectory()
        defer { try? FileManager.default.removeItem(at: indexDir) }

        // Opening an index that was never created.
        XCTAssertThrowsError(
            try spikeQuery(indexDir: indexDir.path, query: "anything", k: 10)
        ) { error in
            guard case SpikeError.IndexIo = error else {
                return XCTFail("expected SpikeError.IndexIo, got \(error)")
            }
        }

        // A query that matches nothing is an error, not a vacuous empty pass.
        let documents = [SpikeDocument(externalId: "doc-0", text: "harbour lantern compass")]
        _ = try spikeIndex(indexDir: indexDir.path, documents: documents)
        XCTAssertThrowsError(
            try spikeQuery(indexDir: indexDir.path, query: "zzzznotpresent", k: 10)
        ) { error in
            guard case SpikeError.QueryParse = error else {
                return XCTFail("expected SpikeError.QueryParse, got \(error)")
            }
        }
    }
}
