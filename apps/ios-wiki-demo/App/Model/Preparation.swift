import Foundation
import Xtriever

/// The app's start-up state machine (data-model "Preparation").
enum Preparation: Equatable {
    case idle
    /// `XtrieverIndex.open` in flight: both models and the index, one call.
    case loadingModels
    /// One throwaway search paging the vectors in (research D4).
    case warming
    case ready(ReadyInfo)
    case failed(PreparationFailure)

    var label: String {
        switch self {
        case .idle: return "starting"
        case .loadingModels: return "opening the index and both models"
        case .warming: return "warming the index — the first search pages the vectors in"
        case .ready: return "ready"
        case .failed(let f): return f.message
        }
    }
}

/// What the app learned while preparing.
struct ReadyInfo: Equatable {
    let info: IndexInfo
    let openMs: UInt64
    let warmMs: UInt64
    let corpus: CorpusSidecar?
    let attribution: String?
    /// "Simple English Wikipedia" or "007 fixture (40 documents)".
    let indexName: String
    let indexBytes: UInt64
}

enum PreparationFailure: Equatable {
    /// A resource the build did not stage, and the flag that stages it.
    case missingResource(name: String, stagingFlag: String)
    /// The engine refused to open (its own message).
    case engine(message: String)

    var message: String {
        switch self {
        case .missingResource(let name, _): return "\(name) is not in the app bundle"
        case .engine(let message): return message
        }
    }

    var remedy: String {
        switch self {
        case .missingResource(_, let flag): return "rebuild with scripts/build-ios-package.sh --with-models \(flag) --demo"
        case .engine: return "the index or the models refused to open; rebuild the resources and reinstall"
        }
    }
}
