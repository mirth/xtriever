import Foundation
import Xtriever

/// The app's one piece of logic (research D6; contracts/app.md "Model"): preparation with a
/// warm-up, the two-phase search — fused first, then re-ranked — with cooperative
/// cancellation, change marks, settings. Views render this; tests drive it. Nothing here
/// retrieves: every hit and every number is the engine's, or a wall clock around one call.
@MainActor
final class DemoModel: ObservableObject {
    @Published private(set) var preparation: Preparation = .idle
    @Published var settings: Settings { didSet { SettingsStore.save(settings) } }
    @Published private(set) var search: SearchState?

    /// The 20 measurement queries, when the Wikipedia resources are bundled.
    var measurementQueriesURL: URL? { HarnessResources.wikipediaQueries }

    /// Which bundled index to open: the app prefers Wikipedia and falls back to the fixture;
    /// the model tests pin the fixture so a build with both staged still tests the goldens.
    enum Corpus { case preferWikipedia, fixture }

    private let corpus: Corpus
    private var index: XtrieverIndex?
    private var task: Task<Void, Never>?
    private var starting = false

    init(settings: Settings = SettingsStore.load(), corpus: Corpus = .preferWikipedia) {
        self.settings = settings
        self.corpus = corpus
    }

    // MARK: preparation

    /// idle → loadingModels → warming → ready | failed. Idempotent while in flight; a retry
    /// from `.failed` starts over.
    func start() async {
        if starting { return }
        if case .ready = preparation { return }
        starting = true
        defer { starting = false }

        guard HarnessResources.modelsAreBundled,
              let embedderDir = HarnessResources.embedderDirectory,
              let rerankerDir = HarnessResources.rerankerDirectory
        else {
            preparation = .failed(.missingResource(name: "XtrieverData/models", stagingFlag: "--with-models"))
            return
        }
        let indexDir: URL
        let indexName: String
        let isWikipedia: Bool
        if corpus == .preferWikipedia, HarnessResources.wikipediaIsBundled, let dir = HarnessResources.wikipediaIndexDirectory {
            (indexDir, indexName, isWikipedia) = (dir, "Simple English Wikipedia", true)
        } else if HarnessResources.fixtureIsBundled, let dir = HarnessResources.fixtureIndexDirectory {
            (indexDir, indexName, isWikipedia) = (dir, "007 fixture (40 documents)", false)
        } else {
            preparation = .failed(.missingResource(name: "XtrieverData/wikipedia/index", stagingFlag: "--with-wiki"))
            return
        }

        preparation = .loadingModels
        let openStart = Measure.nowNanos()
        let opened: XtrieverIndex
        do {
            opened = try await XtrieverIndex.open(indexDir: indexDir, embedderDir: embedderDir,
                                                  rerankerDir: rerankerDir, loadPath: .mmap)
        } catch let error as XtrieverError {
            preparation = .failed(.engine(message: error.message))
            return
        } catch {
            preparation = .failed(.engine(message: String(describing: error)))
            return
        }
        let openMs = (Measure.nowNanos() &- openStart) / 1_000_000
        index = opened

        preparation = .warming
        let warmStart = Measure.nowNanos()
        do {
            // The warm-up is the first real engine call; if it fails, a search cannot succeed
            // and the field must not open (FR-002).
            _ = try await opened.search("warm up", options: settings.fusedOptions)
        } catch let error as XtrieverError {
            index = nil
            preparation = .failed(.engine(message: error.message))
            return
        } catch {
            index = nil
            preparation = .failed(.engine(message: String(describing: error)))
            return
        }
        let warmMs = (Measure.nowNanos() &- warmStart) / 1_000_000

        // The sidecar and the attribution are display metadata, not search preconditions: a
        // build that staged the index without them still searches, and About says so.
        let corpus = isWikipedia ? try? CorpusSidecar.load(from: indexDir) : nil
        let attribution = isWikipedia
            ? HarnessResources.wikipediaAttribution.flatMap { try? String(decoding: Data(contentsOf: $0), as: UTF8.self) }
            : nil
        preparation = .ready(ReadyInfo(info: opened.info, openMs: openMs, warmMs: warmMs, corpus: corpus,
                                       attribution: attribution, indexName: indexName,
                                       indexBytes: Self.directoryBytes(indexDir), isWikipedia: isWikipedia))
    }

    // MARK: search

    /// Submit a query: cancels the previous search's delivery, then fused → re-ranked.
    func submit(_ raw: String) {
        task?.cancel()
        task = nil
        let query = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else {
            search = SearchState.empty()
            return
        }
        guard let index else {
            search = SearchState(id: UUID(), query: query, phase: .failed("the index is not open"))
            return
        }
        let id = UUID()
        let settings = self.settings
        search = SearchState(id: id, query: query, phase: .fusing)
        task = Task { [weak self] in
            let fusedStart = Measure.nowNanos()
            do {
                let fused = try await index.search(query, options: settings.fusedOptions)
                try Task.checkCancellation()
                let fusedMs = (Measure.nowNanos() &- fusedStart) / 1_000_000
                guard let self, self.search?.id == id else { return }
                self.search?.fused = fused
                self.search?.fusedMs = fusedMs
                if settings.rerankDepth == 0 {
                    self.search?.footprintBytes = Measure.snapshot().footprintBytes
                    self.search?.phase = .done
                    return
                }
                self.search?.phase = .reranking
                let rerankedStart = Measure.nowNanos()
                let reranked = try await index.search(query, options: settings.rerankedOptions)
                try Task.checkCancellation()
                let rerankedMs = (Measure.nowNanos() &- rerankedStart) / 1_000_000
                guard self.search?.id == id else { return }
                let change = ChangeMark.compute(fused: fused.hits, reranked: reranked.hits)
                self.search?.reranked = reranked
                self.search?.rerankedMs = rerankedMs
                self.search?.marks = change.marks
                self.search?.dropped = change.dropped
                self.search?.footprintBytes = Measure.snapshot().footprintBytes
                self.search?.phase = .done
            } catch is CancellationError {
                // Stale: a newer submission owns `search`.
            } catch let error as XtrieverError {
                guard let self, self.search?.id == id else { return }
                self.search?.phase = .failed(error.message)
            } catch {
                guard let self, self.search?.id == id else { return }
                self.search?.phase = .failed(String(describing: error))
            }
        }
    }

    /// Cancel the current search's delivery (the engine finishes its call; the result is dropped).
    func cancel() {
        task?.cancel()
        task = nil
    }

    private static func directoryBytes(_ dir: URL) -> UInt64 {
        var total: UInt64 = 0
        if let e = FileManager.default.enumerator(at: dir, includingPropertiesForKeys: [.fileSizeKey]) {
            for case let url as URL in e {
                total += UInt64((try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
            }
        }
        return total
    }
}
