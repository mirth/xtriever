import Foundation
import SwiftUI
import Xtriever

/// What a search runs under (data-model "Settings"; research D5). The re-rank depth is the
/// app's choice, not the engine's: 10 since Feature 018 — Feature 014 measured depth 10 at
/// −0.3 mean nDCG@10 against the engine's default 20 for half the cross-encoder calls, and
/// Feature 017 measured the phone at 1.4 s instead of 2.3 s per re-ranked search. The 4,000 ms
/// budget is from the 008 device records: it never cut a measured query at default threads.
struct Settings: Codable, Equatable {
    var rerankDepth: UInt32 = 10
    var budgetMs: UInt64? = 4_000
    var strict: Bool = false

    static let depths: [UInt32] = [0, 5, 10, 20]
    static let budgets: [UInt64?] = [nil, 500, 1_000, 2_000, 4_000, 8_000]

    /// The Settings footer under the depth picker — the trade-off with its numbers (spec FR-002).
    static let depthExplanation =
        "Re-ranks the first 10 fused candidates by default — half the cross-encoder work of the engine's default 20, for −0.3 mean nDCG@10 on the BEIR benchmark sets (Feature 014). On the reference phone that is a re-ranked answer in 1.4 s instead of 2.3 s (Feature 017). Choose 20 for the engine's default."

    /// The first search: the fused stages only, unbudgeted (0.35 s median on the device).
    var fusedOptions: SearchOptions {
        SearchOptions(k: 10, rerankDepth: 0, explain: true)
    }

    /// The second search: the full pipeline under the budget.
    var rerankedOptions: SearchOptions {
        SearchOptions(k: 10, rerankDepth: rerankDepth, maxTimeMs: budgetMs, strict: strict, explain: true)
    }
}

/// `@AppStorage`-backed persistence for the three settings.
enum SettingsStore {
    private static let key = "xtriever.demo.settings"

    static func load() -> Settings {
        guard let data = UserDefaults.standard.data(forKey: key),
              let s = try? JSONDecoder().decode(Settings.self, from: data) else { return Settings() }
        return s
    }

    static func save(_ settings: Settings) {
        if let data = try? JSONEncoder().encode(settings) { UserDefaults.standard.set(data, forKey: key) }
    }
}
