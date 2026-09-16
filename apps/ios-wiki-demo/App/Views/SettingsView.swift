import SwiftUI

/// Re-rank depth, time budget, strict mode (spec US4; FR-011).
struct SettingsView: View {
    @Binding var settings: Settings
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Re-rank depth", selection: $settings.rerankDepth) {
                        ForEach(Settings.depths, id: \.self) { Text($0 == 0 ? "off" : "\($0)").tag($0) }
                    }
                } footer: {
                    Text(Settings.depthExplanation)
                }
                Section {
                    Picker("Time budget", selection: $settings.budgetMs) {
                        ForEach(Settings.budgets, id: \.self) { budget in
                            Text(budget.map { "\($0) ms" } ?? "none").tag(budget)
                        }
                    }
                    Toggle("Strict mode", isOn: $settings.strict)
                } footer: {
                    Text("The budget bounds the re-ranked search. When it cuts a stage short, the stage report says so and the previous stage's results stand; in strict mode a cut stage is an error instead.")
                }
            }
            .navigationTitle("Settings")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}
