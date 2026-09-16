import XCTest
import Xtriever
@testable import XtrieverWikiDemo

/// Feature 018: the app's re-rank default is 10 (014 F-003, 017 device record), the picker
/// offers 0 / 5 / 10 / 20, a persisted choice wins over the default, and the Settings copy
/// quotes the numbers the trade-off rests on. Offline.
final class SettingsTests: XCTestCase {
    func testDefaultsAreTheAppsChoice() {
        let s = Settings()
        XCTAssertEqual(s.rerankDepth, 10, "the app re-ranks 10 by default (018); the engine's default stays 20")
        XCTAssertEqual(s.budgetMs, 4_000)
        XCTAssertFalse(s.strict)
        XCTAssertEqual(s.rerankedOptions.rerankDepth, 10)
        XCTAssertEqual(s.fusedOptions.rerankDepth, 0)
    }

    func testDepthChoices() {
        XCTAssertEqual(Settings.depths, [0, 5, 10, 20])
        XCTAssertTrue(Settings.depths.contains(Settings().rerankDepth))
    }

    func testPersistedChoiceWins() {
        // Through the store itself, not a bare Codable round trip: a regression that made
        // `load()` ignore the stored value must fail here. The store keeps one key in the
        // standard defaults; whatever was there is put back afterwards.
        let key = "xtriever.demo.settings"
        let previous = UserDefaults.standard.data(forKey: key)
        defer {
            if let previous { UserDefaults.standard.set(previous, forKey: key) } else { UserDefaults.standard.removeObject(forKey: key) }
        }
        let chosen = Settings(rerankDepth: 20, budgetMs: nil, strict: false)
        SettingsStore.save(chosen)
        let back = SettingsStore.load()
        XCTAssertEqual(back, chosen, "a stored 20 stays 20; only the fresh default changed")
        XCTAssertEqual(back.rerankedOptions.rerankDepth, 20)
        UserDefaults.standard.removeObject(forKey: key)
        XCTAssertEqual(SettingsStore.load(), Settings(), "nothing stored → the fresh default (10)")
    }

    func testTheExplanationQuotesTheNumbers() {
        let text = Settings.depthExplanation
        for needle in ["10", "20", "0.3", "1.4", "2.3"] {
            XCTAssertTrue(text.contains(needle), "the Settings copy must quote \(needle): \(text)")
        }
    }
}
