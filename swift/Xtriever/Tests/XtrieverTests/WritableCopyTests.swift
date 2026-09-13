import XCTest
import Xtriever

/// `XtrieverIndex.writableCopy(of:named:)` — the copy an app makes of a bundled index (report
/// F-001). Needs no models: the "index" here is a directory with a descriptor and one file.
final class WritableCopyTests: XCTestCase {
    private let fm = FileManager.default
    private var source: URL!

    override func setUpWithError() throws {
        source = fm.temporaryDirectory.appendingPathComponent("xt-copy-source-\(UUID().uuidString)", isDirectory: true)
        try fm.createDirectory(at: source, withIntermediateDirectories: true)
        try write("descriptor v1", to: "xtriever-pipeline.json")
        try write("payload", to: "lexical/meta.json")
    }

    override func tearDownWithError() throws {
        try? fm.removeItem(at: source)
        try? fm.removeItem(at: appSupport.appendingPathComponent("Xtriever/copy-test", isDirectory: true))
    }

    func testANameThatIsNotOnePathComponentIsRefused() {
        for bad in ["", ".", "..", "../elsewhere", "a/b", "a\\b"] {
            XCTAssertThrowsError(try XtrieverIndex.writableCopy(of: source, named: bad), "\(bad) accepted") { error in
                XCTAssertEqual((error as? CocoaError)?.code, .fileWriteInvalidFileName, "\(bad): \(error)")
            }
        }
    }

    func testTheSameDescriptorIsNotCopiedTwiceAndANewOneReplacesTheTreeWhole() throws {
        let first = try XtrieverIndex.writableCopy(of: source, named: "copy-test")
        XCTAssertEqual(try read("lexical/meta.json", in: first), "payload")

        // A marker written into the copy survives a second call with the same descriptor …
        try Data("kept".utf8).write(to: first.appendingPathComponent("marker"))
        let second = try XtrieverIndex.writableCopy(of: source, named: "copy-test")
        XCTAssertEqual(second, first)
        XCTAssertEqual(try read("marker", in: second), "kept")

        // … and does not survive a new descriptor: the tree is replaced whole, nothing lingers.
        try write("descriptor v2", to: "xtriever-pipeline.json")
        try write("payload 2", to: "lexical/meta.json")
        let third = try XtrieverIndex.writableCopy(of: source, named: "copy-test")
        XCTAssertEqual(third, first)
        XCTAssertEqual(try read("xtriever-pipeline.json", in: third), "descriptor v2")
        XCTAssertEqual(try read("lexical/meta.json", in: third), "payload 2")
        XCTAssertFalse(fm.fileExists(atPath: third.appendingPathComponent("marker").path))

        let siblings = try fm.contentsOfDirectory(atPath: third.deletingLastPathComponent().path)
        XCTAssertTrue(siblings.filter { $0.hasPrefix("copy-test.staging") }.isEmpty, "staging left behind: \(siblings)")
    }

    private var appSupport: URL {
        fm.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    }

    private func write(_ text: String, to relative: String) throws {
        let url = source.appendingPathComponent(relative)
        try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(text.utf8).write(to: url)
    }

    private func read(_ relative: String, in dir: URL) throws -> String {
        String(decoding: try Data(contentsOf: dir.appendingPathComponent(relative)), as: UTF8.self)
    }
}
