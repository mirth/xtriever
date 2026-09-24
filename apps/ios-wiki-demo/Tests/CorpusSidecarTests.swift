import XCTest
@testable import XtrieverWikiDemo

/// A slice of the edition (`xtriever wiki build --limit N`, staged with `--with-wiki-slice`)
/// must say so in About and in a measured run's record: the sidecar's `partial` is the
/// article limit, absent on the whole edition. Needs no staged index.
final class CorpusSidecarTests: XCTestCase {
    private static func sidecar(partial: String) -> String {
        """
        {"schema_version":1,"corpus_identity":"\(String(repeating: "a", count: 64))",
         "snapshot":{"edition":"simple","snapshot_date":"2026-09-01"},\(partial)
         "counts":{"articles":3922,"selected":3876,"passages":19998}}
        """
    }

    func testTheWholeEditionIsNamedPlainly() throws {
        let corpus = try JSONDecoder().decode(CorpusSidecar.self, from: Data(Self.sidecar(partial: "").utf8))
        XCTAssertNil(corpus.partial)
        XCTAssertNil(corpus.sliceNote)
        XCTAssertEqual(corpus.runLabel, "wikipedia")
    }

    func testASliceNamesItsArticleLimit() throws {
        let corpus = try JSONDecoder().decode(CorpusSidecar.self, from: Data(Self.sidecar(partial: #""partial":3922,"#).utf8))
        XCTAssertEqual(corpus.partial, 3922)
        XCTAssertEqual(corpus.sliceNote, "first 3922 articles, not the whole edition")
        XCTAssertEqual(corpus.runLabel, "wikipedia, first 3922 articles")
    }
}
