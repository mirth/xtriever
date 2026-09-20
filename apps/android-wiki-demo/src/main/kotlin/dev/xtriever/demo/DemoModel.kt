package dev.xtriever.demo

import dev.xtriever.android.Hit
import dev.xtriever.android.StageReport
import dev.xtriever.android.XtrieverIndex
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * The demonstration's one piece of logic (Feature 025, spec FR-005, FR-006, FR-012).
 *
 * A question becomes two engine calls — the fused stage first, then the re-ranked stage — so a
 * person sees the pipeline working rather than only its conclusion, exactly as the iOS and
 * Python demonstrations do it. Nothing here retrieves, scores or re-orders: the only arithmetic
 * is [ChangeMark], which compares two lists the engine produced.
 */
class DemoModel(private val index: XtrieverIndex, var settings: Settings) {

    /** What one question produced. */
    data class Outcome(
        val query: String,
        val fused: List<Hit>,
        val reranked: List<Hit>,
        val marks: Map<String, ChangeMark>,
        val dropped: List<String>,
        val report: StageReport,
        val fusedMs: Long,
        val rerankedMs: Long,
    )

    private val _state = MutableStateFlow<Outcome?>(null)

    /** The latest finished search, or null before the first one. */
    val state: StateFlow<Outcome?> = _state.asStateFlow()

    private var inFlight: Job? = null

    /**
     * Ask the engine twice and return both answers with the marks between them.
     *
     * An empty or whitespace-only question is an empty result, not an error (spec US2
     * scenario 4). A spent time budget degrades inside the engine's report unless the settings
     * asked for strict mode, in which case the engine's error is raised.
     */
    suspend fun search(text: String): Outcome {
        val query = text.trim()
        if (query.isEmpty()) {
            return Outcome(query, emptyList(), emptyList(), emptyMap(), emptyList(), emptyReport(), 0, 0)
                .also { _state.value = it }
        }

        val fusedStarted = System.nanoTime()
        val fused = index.search(
            query,
            k = HITS,
            rerankDepth = 0,
            explain = true,
            maxTimeMs = settings.maxTimeMs,
            strict = settings.strict,
        )
        val fusedMs = millisSince(fusedStarted)

        if (settings.rerankDepth == 0) {
            val outcome = Outcome(query, fused.hits, fused.hits, emptyMap(), emptyList(), fused.stages, fusedMs, 0)
            _state.value = outcome
            return outcome
        }

        val rerankedStarted = System.nanoTime()
        val reranked = index.search(
            query,
            k = HITS,
            rerankDepth = settings.rerankDepth,
            explain = true,
            maxTimeMs = settings.maxTimeMs,
            strict = settings.strict,
        )
        val rerankedMs = millisSince(rerankedStarted)

        val (marks, dropped) = ChangeMark.compute(
            fused = fused.hits.map { it.externalId },
            reranked = reranked.hits.map { it.externalId },
        )
        val outcome = Outcome(query, fused.hits, reranked.hits, marks, dropped, reranked.stages, fusedMs, rerankedMs)
        _state.value = outcome
        return outcome
    }

    /**
     * Run a search in [scope], cancelling one already in flight: a person who types a second
     * question sees the second one's results, never the first's arriving late.
     */
    fun submit(scope: CoroutineScope, text: String) {
        inFlight?.cancel()
        inFlight = scope.launch {
            val outcome = runCatching { search(text) }
            if (isActive) outcome.getOrNull()
        }
    }

    /** Waits for the search in flight, if any — for tests and for the measurement runner. */
    suspend fun awaitIdle() {
        inFlight?.join()
    }

    private fun millisSince(startedNanos: Long): Long = (System.nanoTime() - startedNanos) / 1_000_000

    private fun emptyReport() = StageReport(
        lexicalCandidates = 0u,
        denseCandidates = null,
        degraded = null,
        rerank = null,
        timeLimitIgnored = false,
    )

    private companion object {
        /** Hits shown, as the other two demonstrations show. */
        const val HITS = 10
    }
}
