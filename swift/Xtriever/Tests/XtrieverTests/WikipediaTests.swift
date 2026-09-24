import XCTest
import Xtriever

/// Feature 008 US3 — the staged Wikipedia index opens **in place** inside the (read-only)
/// bundle, searches, and every hit carries a title line, a derivable article URL and chunk
/// provenance (contracts/artefact.md). Skips unless `--with-wiki` (or `--with-wiki-slice`, `--with-wiki-dev`) staged it.
final class WikipediaTests: XCTestCase {
    private struct MeasurementQuery: Decodable { let id: String; let text: String }

    private func resources() throws -> (index: URL, queries: [MeasurementQuery], models: (embedder: URL, reranker: URL)) {
        guard HarnessResources.wikipediaIsBundled,
              let i = HarnessResources.wikipediaIndexDirectory,
              let q = HarnessResources.wikipediaQueries
        else { throw XCTSkip("Wikipedia index not bundled — scripts/build-ios-package.sh --with-models --with-wiki (or --with-wiki-slice, --with-wiki-dev)") }
        let queries = try JSONDecoder().decode([MeasurementQuery].self, from: Data(contentsOf: q))
        return (i, queries, try Support.models())
    }

    private func fileList(_ dir: URL) throws -> [String: UInt64] {
        var out: [String: UInt64] = [:]
        let e = FileManager.default.enumerator(at: dir, includingPropertiesForKeys: [.fileSizeKey, .isDirectoryKey])!
        for case let url as URL in e {
            let v = try url.resourceValues(forKeys: [.fileSizeKey, .isDirectoryKey])
            if v.isDirectory == true { continue }
            out[url.path.replacingOccurrences(of: dir.path, with: "")] = UInt64(v.fileSize ?? 0)
        }
        return out
    }

    func testOpensInPlaceSearchesAndHitsCarryTitleUrlAndProvenance() async throws {
        let r = try resources()
        let before = try fileList(r.index)
        // No copy-out: the directory inside the bundle is opened directly (008 D11).
        let index = try await XtrieverIndex.open(indexDir: r.index, embedderDir: r.models.embedder,
                                                 rerankerDir: r.models.reranker, loadPath: .mmap)
        XCTAssertGreaterThan(index.info.documents, 0)
        let response = try await index.search(r.queries[0].text, options: SearchOptions(k: 5, explain: true))
        XCTAssertFalse(response.hits.isEmpty, "\(r.queries[0].text) returned no hits")
        for hit in response.hits {
            guard let split = hit.titleAndPassage else { return XCTFail("no title line in \(hit.text.prefix(60))") }
            XCTAssertFalse(split.title.isEmpty)
            XCTAssertFalse(split.passage.isEmpty)
            XCTAssertTrue(hit.text.hasPrefix(split.title + "\n\n"))
            guard let url = hit.wikipediaURL else { return XCTFail("no URL for \(split.title)") }
            XCTAssertTrue(url.absoluteString.hasPrefix("https://simple.wikipedia.org/wiki/"), url.absoluteString)
            guard let chunk = hit.chunk else { return XCTFail("no chunk provenance") }
            XCTAssertTrue(chunk.parent.allSatisfy(\.isNumber), "parent is the page id: \(chunk.parent)")
            XCTAssertEqual(hit.externalId, "\(chunk.parent)#\(chunk.ordinal)")
            XCTAssertNotNil(chunk.byteStart); XCTAssertNotNil(chunk.byteEnd)
        }
        XCTAssertEqual(try fileList(r.index), before, "opening in place must leave the bundle untouched")
    }

    func testWikipediaUrlDerivationMatchesTheSnapshotRule() {
        let base = "https://simple.wikipedia.org/wiki/"
        let cases: [(String, String)] = [
            ("April", "April"), ("Alan Turing", "Alan%20Turing"), ("Church (building)", "Church%20%28building%29"),
            ("Dutton's Speedwords", "Dutton%27s%20Speedwords"), ("AC/DC", "AC/DC"), ("Biel/Bienne", "Biel/Bienne"),
            ("Café", "Caf%C3%A9"), ("a~b_c-d.e", "a~b_c-d.e"), ("東京", "%E6%9D%B1%E4%BA%AC"), ("100% sure?", "100%25%20sure%3F"),
        ]
        for (title, want) in cases {
            XCTAssertEqual(Hit.wikipediaURL(forTitle: title)?.absoluteString, base + want, title)
        }
        XCTAssertNil(Hit(externalId: "1#0", text: "no blank line here", score: 0, rerankScore: nil, chunk: nil, explain: nil).titleAndPassage)
    }
}
