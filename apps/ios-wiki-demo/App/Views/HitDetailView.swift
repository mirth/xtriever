import SwiftUI

/// The full passage, its provenance, the article link and the engine's explanation (spec US1
/// scenario 5, US2 scenario 3).
struct HitDetailView: View {
    let hit: DisplayedHit

    var body: some View {
        List {
            Section {
                Text(hit.title).font(.title2.bold())
                if let ordinal = hit.ordinal {
                    Text("passage \(ordinal + 1) of the article").font(.footnote).foregroundStyle(.secondary)
                }
                Text(hit.passage).textSelection(.enabled)
                if let url = hit.url {
                    Link("Open on Wikipedia", destination: url)
                }
            }
            Section("Why this hit") {
                ForEach(hit.features, id: \.name) { feature in
                    HStack {
                        Text(feature.name).font(.body.monospaced())
                        Spacer()
                        Text(DisplayedHit.render(feature.value)).foregroundStyle(feature.value.isNaN ? .secondary : .primary)
                    }
                }
                HStack { Text("fused score"); Spacer(); Text(String(format: "%.6f", hit.score)) }
                if let r = hit.rerankScore {
                    HStack { Text("re-rank score"); Spacer(); Text(String(format: "%.4f", r)) }
                }
            }
        }
        .navigationTitle("Rank \(hit.rank)")
        .navigationBarTitleDisplayMode(.inline)
    }
}
