import CryptoKit
import Foundation

/// Feature 028 (the accelerated inference spike): the compute-path label the packager stages
/// and the hits digest of a measured run (data-model "Run-record additions"). A copy of the
/// Swift package test target's file of the same name, which this test target cannot import.
/// Spike code — removed or promoted by the follow-up the verdict names.
enum SpikeDigest {
    struct Hit { let id: String; let score: Double; let rerank: Float? }
    struct Response { let queryId: String; let depth: Int; let hits: [Hit] }
    struct Label: Codable { let computePath: String; let rerankBatch: Bool }

    /// Red-checkpoint stub (T007).
    static func digest(_ responses: [Response]) -> String { "" }

    /// Red-checkpoint stub (T007).
    static func label(from url: URL?) -> Label { Label(computePath: "", rerankBatch: true) }
}
