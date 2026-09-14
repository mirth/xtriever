import Foundation
import SwiftUI
import Xtriever

/// What a search runs under (data-model "Settings"; research D5). Defaults from the 008 device
/// records: depth 20 and a 4,000 ms budget never cut a measured query at default threads.
struct Settings: Codable, Equatable {
    var rerankDepth: UInt32 = 20
    var budgetMs: UInt64? = 4_000
    var strict: Bool = false

    static let depths: [UInt32] = [0, 5, 20]
    static let budgets: [UInt64?] = [nil, 500, 1_000, 2_000, 4_000, 8_000]

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
