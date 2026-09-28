import CryptoKit
import Foundation

/// Feature 028 (the accelerated inference spike): the compute-path label the packager stages
/// and the hits digest of a measured run (data-model "Run-record additions"). Spike code —
/// removed or promoted by the follow-up the verdict names.
enum SpikeDigest {
    struct Hit { let id: String; let score: Double; let rerank: Float? }
    struct Response { let queryId: String; let depth: Int; let hits: [Hit] }
    struct Label: Codable { let computePath: String; let rerankBatch: Bool }

    /// One line per hit — query id, depth, 1-based rank, external id, the fused score's `f64`
    /// bits, the re-rank score's `f32` bits or `-` — SHA-256 over the whole, lower-case hex.
    static func digest(_ responses: [Response]) -> String {
        var text = ""
        for response in responses {
            for (index, hit) in response.hits.enumerated() {
                let rerank = hit.rerank.map { String(format: "%08x", $0.bitPattern) } ?? "-"
                let score = String(format: "%016llx", hit.score.bitPattern)
                text += "\(response.queryId)\t\(response.depth)\t\(index + 1)\t\(hit.id)\t\(score)\t\(rerank)\n"
            }
        }
        return SHA256.hash(data: Data(text.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    /// The staged label, or `unknown` when it is missing or unreadable — never a guess.
    static func label(from url: URL?) -> Label {
        guard let url, let data = try? Data(contentsOf: url),
              let label = try? JSONDecoder().decode(Label.self, from: data)
        else { return Label(computePath: "unknown", rerankBatch: false) }
        return label
    }
}
