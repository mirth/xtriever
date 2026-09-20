package dev.xtriever.demo.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.foundation.clickable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import dev.xtriever.android.Hit
import dev.xtriever.android.IndexInfo
import dev.xtriever.demo.ChangeMark
import dev.xtriever.demo.AboutFacts
import dev.xtriever.demo.CorpusSidecar
import dev.xtriever.demo.DemoModel
import dev.xtriever.demo.HitText
import dev.xtriever.demo.Settings

/** The search screen: the fused list first, then the re-ranked order with what moved. */
@Composable
fun SearchScreen(
    query: String,
    onQueryChange: (String) -> Unit,
    onSubmit: () -> Unit,
    running: Boolean,
    outcome: DemoModel.Outcome?,
    onOpen: (Hit) -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.padding(12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                value = query,
                onValueChange = onQueryChange,
                label = { Text("ask a question") },
                singleLine = true,
                modifier = Modifier.weight(1f),
            )
            // Never disabled: submitting again while a search runs is how a person cancels it
            // (spec US2 scenario 5), so the button must stay live.
            Button(onClick = onSubmit) { Text(if (running) "searching…" else "search") }
        }
        if (running) CircularProgressIndicator(Modifier.padding(horizontal = 12.dp))
        outcome ?: return@Column
        LazyColumn(Modifier.fillMaxSize()) {
            item { SectionTitle("fused (lexical + dense), ${outcome.fused.size} hits") }
            itemsIndexed(outcome.fused) { i, hit -> HitRow(i + 1, hit, null, onOpen) }
            if (outcome.phase == DemoModel.Phase.FUSED) {
                item { SectionTitle("re-ranking…") }
            } else if (outcome.rerankedMs > 0) {
                item { SectionTitle("re-ranked (depth from settings), ${outcome.reranked.size} hits") }
                itemsIndexed(outcome.reranked) { i, hit ->
                    HitRow(i + 1, hit, outcome.marks[hit.externalId], onOpen)
                }
            }
            item { StageReportView(outcome) }
        }
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(text, style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(12.dp))
}

/** One hit in full: the passage, the article link, and the engine's eight explained features. */
@Composable
fun HitDetailScreen(hit: Hit) {
    val (title, passage) = HitText.split(hit.text)
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        Text(title ?: hit.externalId, style = MaterialTheme.typography.titleLarge)
        HitText.articleUrl(title)?.let { url ->
            val opener = LocalUriHandler.current
            Text(
                url,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.primary,
                textDecoration = TextDecoration.Underline,
                modifier = Modifier
                    .clickable { opener.openUri(url) }
                    .semantics { role = Role.Button },
            )
        }
        Text(passage, Modifier.padding(top = 12.dp), style = MaterialTheme.typography.bodyMedium)
        Text("features", Modifier.padding(top = 16.dp), style = MaterialTheme.typography.titleSmall)
        features(hit).forEach { (name, value) ->
            Row(Modifier.fillMaxWidth().padding(vertical = 2.dp), Arrangement.SpaceBetween) {
                Text(name, style = MaterialTheme.typography.labelMedium)
                Text(value, style = MaterialTheme.typography.labelMedium)
            }
        }
    }
}

/** The engine's own names, and its own words where a stage did not see the hit. */
private fun features(hit: Hit): List<Pair<String, String>> {
    val explain = hit.explain
    fun f(value: Float?) = value?.let { "%.6f".format(it) } ?: "not seen by this stage"
    fun r(value: UInt?) = value?.toString() ?: "not seen by this stage"
    return listOf(
        "bm25.score" to f(explain?.bm25Score),
        "bm25.rank" to r(explain?.bm25Rank),
        "dense.score" to f(explain?.denseScore),
        "dense.rank" to r(explain?.denseRank),
        "fused.score" to (explain?.fused?.let { "%.6f".format(it) } ?: "%.6f".format(hit.score)),
        "rerank.score" to f(explain?.rerankScore),
        "rerank.rank" to r(explain?.rerankRank),
        "rerank.combined" to (explain?.rerankCombined?.let { "%.6f".format(it) } ?: "not seen by this stage"),
    )
}

/** The corpus, the engine's account of the index, this session's timings, the attribution. */
@Composable
fun AboutScreen(info: IndexInfo, corpus: CorpusSidecar, openMs: Long, attribution: String?) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        AboutFacts.of(info, corpus, openMs).forEach { (name, value) -> AboutRow(name, value) }
        Text("Attribution", Modifier.padding(top = 16.dp), style = MaterialTheme.typography.titleMedium)
        Text(
            attribution ?: "Text from Simple English Wikipedia, CC BY-SA 4.0.",
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

@Composable
private fun AboutRow(name: String, value: String) {
    Column(Modifier.padding(vertical = 3.dp)) {
        Text(name, style = MaterialTheme.typography.labelSmall)
        Text(value, style = MaterialTheme.typography.bodySmall)
    }
}

/** The one choice: how many fused candidates the cross-encoder re-ranks. */
@Composable
fun SettingsScreen(settings: Settings, onChange: (Settings) -> Unit) {
    Column(Modifier.padding(16.dp)) {
        Text("Re-rank depth", style = MaterialTheme.typography.titleMedium)
        Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Settings.DEPTHS.forEach { depth ->
                FilterChip(
                    selected = settings.rerankDepth == depth,
                    onClick = { onChange(settings.copy(rerankDepth = depth)) },
                    label = { Text(depth.toString()) },
                )
            }
        }
        Text(
            "The engine's own default is 20. Feature 014 measured depth 10 at −0.3 mean nDCG@10 " +
                "for half the cross-encoder calls, so the demonstrations default to 10. " +
                "0 shows the fused list alone.",
            Modifier.padding(top = 12.dp),
            style = MaterialTheme.typography.bodySmall,
        )
    }
}
