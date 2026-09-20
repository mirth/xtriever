package dev.xtriever.demo

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import dev.xtriever.android.DeviceSupport
import dev.xtriever.android.Hit
import dev.xtriever.android.XtrieverIndex
import dev.xtriever.demo.ui.AboutScreen
import dev.xtriever.demo.ui.HitDetailScreen
import dev.xtriever.demo.ui.SearchScreen
import dev.xtriever.demo.ui.SettingsScreen
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File

/**
 * The demonstration (Feature 025, User Stories 2 and 3). It prepares itself once, opens the
 * bundled corpus and shows the pipeline working. It owns no retrieval logic.
 */
class MainActivity : ComponentActivity() {

    private sealed interface Stage {
        data object Starting : Stage
        data class Preparing(val done: Int, val total: Int) : Stage
        data class Failed(val reason: String) : Stage
        data class Ready(val index: XtrieverIndex, val model: DemoModel, val corpus: CorpusSidecar, val attribution: String?) : Stage
    }

    private var stage by mutableStateOf<Stage>(Stage.Starting)
    private lateinit var store: SettingsStore

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        store = SettingsStore(this)
        setContent { MaterialTheme { Surface(Modifier.fillMaxSize()) { Root() } } }
        lifecycleScope.launch { prepare() }
    }

    override fun onDestroy() {
        (stage as? Stage.Ready)?.index?.close()
        super.onDestroy()
    }

    private suspend fun prepare() {
        when (val support = DeviceSupport.check()) {
            is DeviceSupport.Unsupported -> { stage = Stage.Failed(support.reason); return }
            DeviceSupport.Supported -> Unit
        }
        val prepared = withContext(Dispatchers.IO) {
            Preparation(this@MainActivity).prepare { stage = Stage.Preparing(it.done, it.total) }
        }
        when (prepared) {
            is Preparation.State.Refused -> { stage = Stage.Failed(prepared.reason); return }
            is Preparation.State.Ready -> Unit
            else -> { stage = Stage.Failed("preparation did not finish"); return }
        }
        val dirs = (prepared as Preparation.State.Ready).prepared
        stage = withContext(Dispatchers.IO) {
            runCatching {
                val index = XtrieverIndex.open(
                    indexDir = File(dirs.corpusDir, "index"),
                    embedderDir = File(dirs.modelsDir, "embedder"),
                    rerankerDir = File(dirs.modelsDir, "reranker"),
                )
                Stage.Ready(
                    index = index,
                    model = DemoModel(index, store.load()),
                    corpus = CorpusSidecar.read(File(dirs.corpusDir, "corpus.json")),
                    attribution = File(dirs.corpusDir, "ATTRIBUTION.txt").takeIf { it.isFile }?.readText(),
                )
            }.getOrElse { Stage.Failed(it.message ?: it.toString()) }
        }
    }

    @Composable
    private fun Root() {
        when (val current = stage) {
            Stage.Starting -> Message("starting…")
            is Stage.Preparing -> Message("preparing ${current.done} of ${current.total} files…")
            is Stage.Failed -> Message(current.reason)
            is Stage.Ready -> Ready(current)
        }
    }

    @Composable
    private fun Ready(ready: Stage.Ready) {
        var tab by remember { mutableStateOf("search") }
        var query by remember { mutableStateOf("") }
        // The model publishes: the fused answer as soon as it arrives, then the complete one.
        val outcome by ready.model.state.collectAsState()
        var open by remember { mutableStateOf<Hit?>(null) }
        var settings by remember { mutableStateOf(ready.model.settings) }
        val scope = rememberCoroutineScope()

        LaunchedEffect(settings) {
            ready.model.settings = settings
            store.save(settings)
        }

        Column(Modifier.fillMaxSize()) {
            Row(Modifier.padding(8.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                listOf("search", "about", "settings").forEach { name ->
                    TextButton(onClick = { tab = name; open = null }) { Text(if (tab == name) "[$name]" else name) }
                }
            }
            val hit = open
            when {
                hit != null -> HitDetailScreen(hit)
                tab == "about" -> AboutScreen(ready.index.info, ready.corpus, ready.index.openMs, ready.attribution)
                tab == "settings" -> SettingsScreen(settings) { settings = it }
                else -> SearchScreen(
                    query = query,
                    onQueryChange = { query = it },
                    // Submitting again cancels the search in flight; the model drops the older
                    // answer even when its native call has already started.
                    onSubmit = { ready.model.submit(scope, query) },
                    running = outcome?.phase == DemoModel.Phase.FUSED,
                    outcome = outcome,
                    onOpen = { open = it },
                )
            }
        }
    }

    @Composable
    private fun Message(text: String) {
        Column(Modifier.fillMaxSize().padding(24.dp)) { Text(text) }
    }
}
