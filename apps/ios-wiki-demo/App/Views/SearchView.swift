import SwiftUI
import Xtriever

/// The search screen (spec US1, US2, US4): submit-only search, the stage label, the hit list
/// that goes from fused to re-ranked with change marks, and the engine's stage report.
struct SearchView: View {
    @ObservedObject var model: DemoModel
    let info: ReadyInfo

    @State private var query = ""
    @State private var showSettings = false
    @State private var showAbout = false

    var body: some View {
        NavigationStack {
            List {
                if let search = model.search {
                    Section {
                        stageLabel(search)
                    }
                    if let response = search.current {
                        Section {
                            ForEach(DisplayedHit.list(from: response, marks: search.reranked == nil ? nil : search.marks)) { hit in
                                NavigationLink(value: hit) { HitRow(hit: hit) }
                            }
                            if !search.dropped.isEmpty {
                                Text("\(search.dropped.count) fused hit\(search.dropped.count == 1 ? "" : "s") fell out of the re-ranked top \(response.hits.count)")
                                    .font(.footnote).foregroundStyle(.secondary)
                            }
                        } header: {
                            Text(search.reranked == nil ? "Fused (lexical + dense)" : "Re-ranked")
                        }
                        Section("Stage report") {
                            StageReportView(response: response, search: search)
                        }
                    }
                } else {
                    Section {
                        Text("Ask a question and press return. The fused list appears first; the re-ranked order follows.")
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .animation(.default, value: model.search?.reranked != nil)
            .navigationTitle(info.indexName == "Simple English Wikipedia" ? "Wikipedia" : "Fixture")
            // The tapped hit travels with the navigation: if re-ranking finishes while the
            // detail is open and the hit drops out of the list, the detail stays as tapped.
            .navigationDestination(for: DisplayedHit.self) { hit in HitDetailView(hit: hit) }
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Ask Simple English Wikipedia")
            .onSubmit(of: .search) { model.submit(query) }
            .autocorrectionDisabled()
            .toolbar {
                // Bottom bar, not the navigation bar: while the search field is active — the
                // state a person is in after submitting — iOS hides navigation-bar items, and
                // Cancel would clear the results.
                ToolbarItemGroup(placement: .bottomBar) {
                    Button { showSettings = true } label: { Label("Settings", systemImage: "slider.horizontal.3") }
                    Spacer()
                    Button { showAbout = true } label: { Label("About", systemImage: "info.circle") }
                }
            }
            .sheet(isPresented: $showSettings) { SettingsView(settings: $model.settings) }
            .sheet(isPresented: $showAbout) { AboutView(info: info) }
        }
    }

    @ViewBuilder
    private func stageLabel(_ search: SearchState) -> some View {
        switch search.phase {
        case .fusing:
            Label("fusing lexical and dense…", systemImage: "arrow.triangle.merge")
        case .reranking:
            Label("re-ranking \(model.settings.rerankDepth) candidates…", systemImage: "arrow.up.arrow.down")
        case .done:
            Label(search.reranked == nil ? "fused" : "re-ranked", systemImage: "checkmark.circle")
        case .empty:
            Label("nothing to search — type a question", systemImage: "questionmark.circle")
        case .failed(let message):
            Label(message, systemImage: "exclamationmark.triangle").foregroundStyle(.red)
        }
    }
}

/// One hit in the list: rank, title, excerpt, and its change mark once re-ranked.
struct HitRow: View {
    let hit: DisplayedHit

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Text("\(hit.rank)").font(.title3.monospacedDigit()).foregroundStyle(.secondary).frame(minWidth: 28, alignment: .trailing)
            VStack(alignment: .leading, spacing: 4) {
                Text(hit.title).font(.headline)
                Text(hit.passage).font(.subheadline).lineLimit(3)
            }
            Spacer(minLength: 0)
            if let mark = hit.mark { MarkView(mark: mark) }
        }
        .padding(.vertical, 2)
    }
}

struct MarkView: View {
    let mark: ChangeMark

    var body: some View {
        switch mark {
        case .new: Label("new", systemImage: "sparkles").labelStyle(.iconOnly).foregroundStyle(.purple)
        case .same: Image(systemName: "minus").foregroundStyle(.secondary)
        case .up(let n): Label("\(n)", systemImage: "arrow.up").font(.footnote.monospacedDigit()).foregroundStyle(.green)
        case .down(let n): Label("\(n)", systemImage: "arrow.down").font(.footnote.monospacedDigit()).foregroundStyle(.orange)
        }
    }
}
