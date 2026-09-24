import XCTest
import Xtriever
@testable import XtrieverWikiDemo

/// The About screen's facts come from the engine and the shipped files (spec US3, SC-004).
/// Needs the Wikipedia index staged — skips on a fixture-only build.
@MainActor
final class AboutTests: XCTestCase {
    func testSidecarAndAttributionMatchTheShippedFiles() async throws {
        try Support.skipUnlessWikipedia()
        let model = DemoModel(settings: Settings())
        await model.start()
        guard case .ready(let info) = model.preparation else { return XCTFail("\(model.preparation)") }
        guard let corpus = info.corpus else { return XCTFail("no corpus sidecar decoded") }
        XCTAssertEqual(UInt64(corpus.counts.passages), info.info.documents)
        XCTAssertEqual(corpus.corpusIdentity.count, 64)
        XCTAssertTrue(corpus.corpusIdentity.allSatisfy(\.isHexDigit))
        XCTAssertEqual(corpus.snapshot.edition, "simple")
        let shipped = try String(decoding: Data(contentsOf: XCTUnwrap(HarnessResources.wikipediaAttribution)), as: UTF8.self)
        XCTAssertEqual(info.attribution, shipped)
        XCTAssertTrue(shipped.contains("creativecommons.org/licenses/by-sa/4.0"))
        XCTAssertEqual(info.indexName, "Simple English Wikipedia")
        // A slice (--with-wiki-slice) is Wikipedia too: the title must not fall back to the
        // fixture's, whatever the corpus is called.
        XCTAssertTrue(info.isWikipedia)
        XCTAssertEqual(info.title, "Wikipedia")
    }
}
