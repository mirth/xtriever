import SwiftUI

/// What is being searched, on what terms (spec US3; FR-013): the engine's identities, the
/// corpus sidecar, this session's timings, and the attribution the licence requires.
struct AboutView: View {
    let info: ReadyInfo
    @Environment(\.dismiss) private var dismiss

    private static let licenceURL = URL(string: "https://creativecommons.org/licenses/by-sa/4.0/")!

    var body: some View {
        NavigationStack {
            List {
                Section("Corpus") {
                    row("index", info.indexName)
                    if let corpus = info.corpus {
                        row("edition", corpus.snapshot.edition)
                        row("snapshot", corpus.snapshot.snapshotDate)
                        if let slice = corpus.sliceNote { row("slice", slice) }
                        row("articles", corpus.counts.articles.formatted())
                        row("selected", corpus.counts.selected.formatted())
                        row("passages", corpus.counts.passages.formatted())
                        VStack(alignment: .leading) {
                            Text("corpus identity").font(.footnote)
                            Text(corpus.corpusIdentity).font(.caption.monospaced()).textSelection(.enabled)
                        }
                    } else {
                        Text("no corpus sidecar — not the Wikipedia index").foregroundStyle(.secondary)
                    }
                    row("index size", String(format: "%.0f MB", Double(info.indexBytes) / 1e6))
                    Text("Retrieval quality of this corpus is unmeasured.").font(.footnote).foregroundStyle(.secondary)
                }
                Section("Engine") {
                    row("passages (documents)", info.info.documents.formatted())
                    row("format version", "\(info.info.formatVersion)")
                    row("candidate depth", "\(info.info.candidateDepth)")
                    row("re-rank depth (engine default)", "\(info.info.rerankDepth)")
                    row("re-rank depth (app default)", "\(Settings().rerankDepth)")
                    Text("The app re-ranks fewer candidates than the engine's default; see Settings.").font(.footnote).foregroundStyle(.secondary)
                    row("rrf k", "\(info.info.rrfK)")
                    row("dense compaction", info.info.denseCompactDeadShare.map { "over \(Int($0 * 100))% dead rows" } ?? "on merge only")
                    VStack(alignment: .leading) {
                        Text("embedder").font(.footnote)
                        Text(info.info.embedderFingerprint).font(.caption.monospaced()).textSelection(.enabled)
                    }
                    VStack(alignment: .leading) {
                        Text("re-ranker").font(.footnote)
                        Text(info.info.rerankerModelId ?? "none").font(.caption.monospaced()).textSelection(.enabled)
                    }
                }
                Section("This session") {
                    row("open", "\(info.openMs) ms")
                    row("embedder load", "\(info.info.embedderLoadMs) ms")
                    row("re-ranker load", info.info.rerankerLoadMs.map { "\($0) ms" } ?? "—")
                    row("warm-up search", "\(info.warmMs) ms")
                }
                Section("Attribution") {
                    if let attribution = info.attribution {
                        Text(attribution).font(.footnote).textSelection(.enabled)
                    } else {
                        Text("Text from Simple English Wikipedia, CC BY-SA 4.0 (the attribution file ships with the Wikipedia index).").font(.footnote)
                    }
                    Link("CC BY-SA 4.0 licence", destination: Self.licenceURL)
                }
            }
            .navigationTitle("About")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }

    private func row(_ name: String, _ value: String) -> some View {
        HStack { Text(name); Spacer(); Text(value).foregroundStyle(.secondary).multilineTextAlignment(.trailing) }
    }
}
