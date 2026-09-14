import Foundation
import Xtriever

/// One submitted query and everything the app learned about it (data-model "SearchState").
struct SearchState {
    enum Phase: Equatable {
        case fusing, reranking, done, empty
        case failed(String)
        var isFailed: Bool { if case .failed = self { return true }; return false }
    }

    let id: UUID
    let query: String
    var phase: Phase
    var fused: SearchResponse?
    var reranked: SearchResponse?
    var marks: [String: ChangeMark] = [:]
    var dropped: [Hit] = []
    var fusedMs: UInt64?
    var rerankedMs: UInt64?
    var footprintBytes: UInt64?

    /// The response on screen: the re-ranked one once it exists.
    var current: SearchResponse? { reranked ?? fused }

    static func empty() -> SearchState { SearchState(id: UUID(), query: "", phase: .empty) }
}
