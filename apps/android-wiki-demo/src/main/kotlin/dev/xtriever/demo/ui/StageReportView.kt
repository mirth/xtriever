package dev.xtriever.demo.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.xtriever.demo.DemoModel

/**
 * What the stages did, as the engine reported it (spec FR-008): candidate counts, degradation
 * and its reason, the re-rank counts, the time-limit flag, and the wall clocks the app measured
 * around each call.
 */
@Composable
fun StageReportView(outcome: DemoModel.Outcome) {
    val report = outcome.report
    Column(Modifier.padding(12.dp)) {
        Text(
            "lexical ${report.lexicalCandidates} · dense ${report.denseCandidates ?: "—"} · " +
                "re-rank ${report.rerank?.let { "${it.candidates} candidates, ${it.scored} scored" } ?: "did not run"}",
            style = MaterialTheme.typography.labelMedium,
        )
        report.degraded?.let {
            Text("degraded: ${it.stage} — ${it.reason}", style = MaterialTheme.typography.labelMedium)
        }
        report.rerank?.skipped?.let {
            Text("re-rank skipped: $it", style = MaterialTheme.typography.labelMedium)
        }
        Text(
            "time limit ignored: ${report.timeLimitIgnored} · fused ${outcome.fusedMs} ms · " +
                "re-ranked ${outcome.rerankedMs} ms",
            style = MaterialTheme.typography.labelMedium,
        )
        if (outcome.dropped.isNotEmpty()) {
            Text("dropped from the head: ${outcome.dropped.joinToString()}", style = MaterialTheme.typography.labelMedium)
        }
    }
}
