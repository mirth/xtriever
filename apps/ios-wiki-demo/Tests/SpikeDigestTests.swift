import Foundation
import XCTest

/// Feature 028 (the accelerated inference spike): the hits digest is one definition in three
/// languages (data-model "Run-record additions") — the same test vector as the Python demo's
/// `test_measure_spike.py`, the same digest. A copy of the Swift package's test of the same name.
/// Spike code.
final class SpikeDigestTests: XCTestCase {
    static let vector: [SpikeDigest.Response] = [
        .init(queryId: "q01", depth: 0, hits: [.init(id: "a", score: 0.03, rerank: nil), .init(id: "b", score: 0.02, rerank: nil)]),
        .init(queryId: "q01", depth: 10, hits: [.init(id: "b", score: 0.02, rerank: 8.6), .init(id: "a", score: 0.03, rerank: 5.7)]),
        .init(queryId: "q02", depth: 0, hits: [.init(id: "c", score: 0.025, rerank: nil)]),
        .init(queryId: "q02", depth: 10, hits: [.init(id: "c", score: 0.025, rerank: -1.5)]),
    ]
    static let digest = "ef38d0b89356527c392fea5745fcc5e3f789feb76926d9c8c0122b257754d509"

    func testTheSharedVectorDigestsAsInPython() {
        XCTAssertEqual(SpikeDigest.digest(Self.vector), Self.digest)
    }

    func testAnyBitChangesTheDigest() {
        var changed = Self.vector
        changed[3] = .init(queryId: "q02", depth: 10, hits: [.init(id: "c", score: 0.025, rerank: -1.5000001)])
        XCTAssertNotEqual(SpikeDigest.digest(changed), Self.digest)
    }

    func testAMissingLabelFileIsUnknownNotAGuess() {
        let label = SpikeDigest.label(from: URL(fileURLWithPath: "/nonexistent/compute-path.json"))
        XCTAssertEqual(label.computePath, "unknown")
        XCTAssertFalse(label.rerankBatch)
    }

    func testTheLabelFileIsRead() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("compute-path-\(UUID()).json")
        try Data(#"{"computePath":"metal","rerankBatch":true}"#.utf8).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let label = SpikeDigest.label(from: url)
        XCTAssertEqual(label.computePath, "metal")
        XCTAssertTrue(label.rerankBatch)
    }
}
