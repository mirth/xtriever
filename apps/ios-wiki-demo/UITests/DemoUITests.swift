import XCTest

/// The screens, driven as a person would (spec US1 scenarios 2, 5; US2 scenarios 1–4; US3; US4)
/// against whichever index is bundled — the fixture on the simulator. The model's fidelity is
/// proven by `DemoModelTests`; this proves the views show it.
final class DemoUITests: XCTestCase {
    override func setUpWithError() throws { continueAfterFailure = false }

    private func text(_ app: XCUIApplication, containing needle: String) -> XCUIElement {
        app.staticTexts.matching(NSPredicate(format: "label CONTAINS[c] %@", needle)).firstMatch
    }

    /// Wait for a text; if the list is longer than the screen, scroll to find it.
    private func expect(_ app: XCUIApplication, _ needle: String, timeout: TimeInterval = 60,
                        file: StaticString = #filePath, line: UInt = #line) {
        let element = text(app, containing: needle)
        var found = element.waitForExistence(timeout: timeout)
        var swipes = 0
        while !found && swipes < 6 {
            app.swipeUp()
            swipes += 1
            found = element.waitForExistence(timeout: 2)
        }
        if !found {
            let visible = app.staticTexts.allElementsBoundByIndex.prefix(40).map(\.label)
            XCTFail("no text containing \(needle.debugDescription); visible: \(visible)", file: file, line: line)
        }
    }

    func testSearchOpenAHitReadTheReportSettingsAndAbout() throws {
        let app = XCUIApplication()
        app.launch()

        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 90), "the search field appears once the index is ready")
        field.tap()
        field.typeText("zephyr obsidian marlin\n")

        // The re-ranked stage replaces the fused one; the stage report is on screen.
        expect(app, "re-ranked")
        expect(app, "lexical candidates")
        expect(app, "re-rank", timeout: 5)
        expect(app, "engine", timeout: 5)

        // Back to the top for the toolbar. (Tapping a row to push the detail screen is verified
        // by hand: XCUITest's synthesized tap on a row inside an active `.searchable` list does
        // not push in this configuration, while a finger does — 009 report F-001.)
        for _ in 0..<6 { app.swipeDown() }

        // Settings and About open and close.
        app.buttons["Settings"].firstMatch.tap()
        expect(app, "Re-rank depth", timeout: 10)
        app.buttons["Done"].tap()
        app.buttons["About"].firstMatch.tap()
        expect(app, "unmeasured", timeout: 10)   // Corpus section, top of the sheet
        expect(app, "Attribution", timeout: 5)   // further down
        app.buttons["Done"].tap()

        // An empty submission says so.
        field.tap()
        if field.buttons["Clear text"].exists { field.buttons["Clear text"].tap() }
        field.typeText("   \n")
        expect(app, "nothing to search", timeout: 10)
    }
}
