import SwiftUI
import Xtriever

/// The engine's `StageReport`, rendered verbatim, plus the app's own wall times and footprint
/// (spec US2 scenario 4, US4 scenario 3).
struct StageReportView: View {
    let response: SearchResponse
    let search: SearchState

    var body: some View {
        row("lexical candidates", "\(response.stages.lexicalCandidates)")
        if let dense = response.stages.denseCandidates {
            row("dense candidates", "\(dense)")
        } else {
            row("dense", "skipped")
        }
        if let degraded = response.stages.degraded {
            row("degraded at \(degraded.stage)", describe(degraded.reason))
        }
        if let rerank = response.stages.rerank {
            if let skipped = rerank.skipped {
                row("re-rank", "skipped: \(describe(skipped))")
            } else {
                row("re-rank", "scored \(rerank.scored) of \(rerank.candidates)")
            }
        } else {
            row("re-rank", search.reranked == nil ? "not run (fused stage)" : "none")
        }
        if response.stages.timeLimitIgnored {
            row("time limit", "ignored (no clock)")
        }
        row("engine", "\(response.elapsedMs) ms")
        if let f = search.fusedMs { row("fused search (app)", "\(f) ms") }
        if let r = search.rerankedMs { row("re-ranked search (app)", "\(r) ms") }
        if let bytes = search.footprintBytes {
            row("footprint", String(format: "%.1f MB", Double(bytes) / 1e6))
        }
    }

    private func row(_ name: String, _ value: String) -> some View {
        HStack {
            Text(name)
            Spacer()
            Text(value).foregroundStyle(.secondary).multilineTextAlignment(.trailing)
        }
        .font(.footnote)
    }

    private func describe(_ reason: DegradeReason) -> String {
        switch reason {
        case .stageError(let message): return "stage error: \(message)"
        case .budgetExceeded(let elapsedMs, let limitMs): return "budget exceeded: \(elapsedMs) ms of \(limitMs) ms"
        }
    }
}
