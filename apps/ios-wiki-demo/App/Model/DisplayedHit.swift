import Foundation
import Xtriever

/// A hit as the screens show it (data-model "DisplayedHit"): derived from the engine's `Hit`
/// and nothing else. A non-008 index (the fixture) has no title line, so the whole text is
/// the passage and the external id stands in for the title.
struct DisplayedHit: Identifiable, Hashable {
    // Navigation carries the hit itself (SwiftUI's path needs Hashable), so an opened detail
    // stays what was tapped even if the list changes underneath it.
    static func == (a: DisplayedHit, b: DisplayedHit) -> Bool {
        a.id == b.id && a.rank == b.rank && a.mark == b.mark && a.title == b.title && a.passage == b.passage
    }
    func hash(into hasher: inout Hasher) { hasher.combine(id); hasher.combine(rank) }

    let id: String
    let rank: Int
    let title: String
    let passage: String
    let url: URL?
    let ordinal: UInt32?
    let mark: ChangeMark?
    let features: [(name: String, value: Float)]
    let score: Double
    let rerankScore: Float?

    init(rank: Int, hit: Hit, mark: ChangeMark?) {
        id = hit.externalId
        self.rank = rank
        if let split = hit.titleAndPassage {
            title = split.title
            passage = split.passage
        } else {
            title = hit.externalId
            passage = hit.text
        }
        url = hit.wikipediaURL
        ordinal = hit.chunk?.ordinal
        self.mark = mark
        features = hit.explain?.features() ?? []
        score = hit.score
        rerankScore = hit.rerankScore
    }

    static func list(from response: SearchResponse, marks: [String: ChangeMark]?) -> [DisplayedHit] {
        response.hits.enumerated().map { DisplayedHit(rank: $0.offset + 1, hit: $0.element, mark: marks?[$0.element.externalId]) }
    }

    /// "not seen by this stage" where the engine reports `nan`.
    static func render(_ value: Float) -> String {
        value.isNaN ? "not seen by this stage" : String(format: "%.4f", value)
    }
}
