import XCTest
import Xtriever

/// US1 scenario 1 — the handle reports the index's identity.
final class InfoTests: XCTestCase {
    func testInfoMatchesTheGoldens() async throws {
        let (index, expected) = try await Support.openFixture(withReranker: true)
        let i = index.info
        XCTAssertEqual(i.documents, expected.info.documents)
        XCTAssertEqual(i.formatVersion, expected.info.formatVersion)
        XCTAssertEqual(i.embedderFingerprint, expected.info.embedderFingerprint)
        XCTAssertEqual(i.rerankerModelId, expected.info.rerankerModelId)
        XCTAssertEqual(i.candidateDepth, expected.info.candidateDepth)
        XCTAssertEqual(i.rerankDepth, expected.info.rerankDepth)
        XCTAssertEqual(i.rerankMode, expected.info.rerankMode.asRerankMode)
        XCTAssertEqual(i.rerankMode, .interpolate(alpha: 0.5), "Feature 015: the default, recorded")
        XCTAssertEqual(i.rrfK, expected.info.rrfK)
        XCTAssertGreaterThan(i.embedderLoadMs, 0)
        XCTAssertNotNil(i.rerankerLoadMs)
    }

    func testWithoutARerankerTheIdentityIsAbsent() async throws {
        let (index, _) = try await Support.openFixture(withReranker: false)
        XCTAssertNil(index.info.rerankerModelId)
        XCTAssertNil(index.info.rerankerLoadMs)
    }
}
