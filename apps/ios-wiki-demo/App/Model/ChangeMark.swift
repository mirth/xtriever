import Foundation
import Xtriever

/// How a hit's position changed between the fused list and the re-ranked list (spec US2;
/// data-model "ChangeMark"). A pure function of two ranked lists, keyed by external id.
enum ChangeMark: Equatable {
    /// In the re-ranked list only.
    case new
    /// Same rank in both.
    case same
    /// Rose by `n` positions.
    case up(Int)
    /// Fell by `n` positions.
    case down(Int)

    /// Marks for every re-ranked hit, and the fused hits that fell out of the re-ranked list.
    static func compute(fused: [Hit], reranked: [Hit]) -> (marks: [String: ChangeMark], dropped: [Hit]) {
        var fusedRank: [String: Int] = [:]
        for (rank, hit) in fused.enumerated() { fusedRank[hit.externalId] = rank }
        var marks: [String: ChangeMark] = [:]
        var seen: Set<String> = []
        for (rank, hit) in reranked.enumerated() {
            seen.insert(hit.externalId)
            guard let before = fusedRank[hit.externalId] else { marks[hit.externalId] = .new; continue }
            let delta = before - rank
            marks[hit.externalId] = delta == 0 ? .same : (delta > 0 ? .up(delta) : .down(-delta))
        }
        let dropped = fused.filter { !seen.contains($0.externalId) }
        return (marks, dropped)
    }
}
