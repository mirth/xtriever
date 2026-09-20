package dev.xtriever.demo

import dev.xtriever.android.Hit
import dev.xtriever.android.StageReport
import dev.xtriever.android.XtrieverIndex
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The demonstration's one piece of logic (Feature 025, spec FR-005, FR-006, FR-012).
 *
 * A question becomes two engine calls — the fused stage first, then the re-ranked stage — and
 * the fused answer is published as soon as it arrives, so a person watches the pipeline work
 * rather than waiting for its conclusion (spec US2 scenario 1). Nothing here retrieves, scores
 * or re-orders: the only arithmetic is [ChangeMark], which compares two lists the engine
 * produced.
 *
 * Engine calls are synchronous and take seconds, so they run on [dispatcher], never on the
 * thread that draws.
 */
class DemoModel(
    private val index: XtrieverIndex,
    var settings: Settings,
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) {

    /** How far a search has got. */
    enum class Phase { FUSED, COMPLETE }

    /** What one question produced, at [phase]. */
    data class Outcome(
        val query: String,
        val phase: Phase,
        val fused: List<Hit>,
        val reranked: List<Hit>,
        val marks: Map<String, ChangeMark>,
        val dropped: List<String>,
        val report: StageReport,
        val fusedMs: Long,
        val rerankedMs: Long,
    )

    private val _state = MutableStateFlow<Outcome?>(null)

    /** The latest search, fused-only or complete, or null before the first one. */
    val state: StateFlow<Outcome?> = _state.asStateFlow()

    private var inFlight: Job? = null

    /**
     * Which search may publish. A cancelled coroutine can be inside a native call that cannot
     * be interrupted; when it returns, its generation is stale and its answer is dropped rather
     * than overwriting a newer one.
     */
    private var generation = 0L

    /**
     * Ask the engine twice and return both answers with the marks between them, publishing the
     * fused answer as soon as it arrives — to [state], and to [onFused] at the same moment.
     *
     * An empty or whitespace-only question is an empty result, not an error (spec US2
     * scenario 4). A spent time budget degrades inside the engine's report unless the settings
     * asked for strict mode, in which case the engine's error is raised.
     */
    suspend fun search(text: String, onFused: (Outcome) -> Unit = {}): Outcome {
        val mine = ++generation
        val query = text.trim()
        if (query.isEmpty()) {
            return Outcome(query, Phase.COMPLETE, emptyList(), emptyList(), emptyMap(), emptyList(), emptyReport(), 0, 0)
                .also { publish(it, mine) }
        }

        val depth = settings.rerankDepth
        val fusedStarted = System.nanoTime()
        val fused = withContext(dispatcher) {
            index.search(
                query,
                k = HITS,
                rerankDepth = 0,
                explain = true,
                maxTimeMs = settings.maxTimeMs,
                strict = settings.strict,
            )
        }
        val fusedMs = millisSince(fusedStarted)
        val fusedOnly = Outcome(query, Phase.FUSED, fused.hits, emptyList(), emptyMap(), emptyList(), fused.stages, fusedMs, 0)
        publish(fusedOnly, mine)
        // The callback is the same moment as the publication above, in a form a test can pin
        // without depending on how a conflated flow schedules its collectors.
        onFused(fusedOnly)

        if (depth == 0) {
            val complete = fusedOnly.copy(phase = Phase.COMPLETE, reranked = fused.hits)
            publish(complete, mine)
            return complete
        }

        val rerankedStarted = System.nanoTime()
        val reranked = withContext(dispatcher) {
            index.search(
                query,
                k = HITS,
                rerankDepth = depth,
                explain = true,
                maxTimeMs = settings.maxTimeMs,
                strict = settings.strict,
            )
        }
        val rerankedMs = millisSince(rerankedStarted)

        val (marks, dropped) = ChangeMark.compute(
            fused = fused.hits.map { it.externalId },
            reranked = reranked.hits.map { it.externalId },
        )
        val complete = Outcome(
            query = query,
            phase = Phase.COMPLETE,
            fused = fused.hits,
            reranked = reranked.hits,
            marks = marks,
            dropped = dropped,
            report = reranked.stages,
            fusedMs = fusedMs,
            rerankedMs = rerankedMs,
        )
        publish(complete, mine)
        return complete
    }

    /**
     * Run a search in [scope], cancelling one already in flight: a person who types a second
     * question sees the second one's results, and the first's cannot arrive late (the
     * generation check in [publish] is what guarantees that, not the cancellation).
     */
    fun submit(scope: CoroutineScope, text: String) {
        inFlight?.cancel()
        inFlight = scope.launch { runCatching { search(text) } }
    }

    /** Waits for the search in flight, if any — for tests and for the measurement runner. */
    suspend fun awaitIdle() {
        inFlight?.join()
    }

    /** Publishes only while this search is still the newest one. */
    private fun publish(outcome: Outcome, mine: Long) {
        if (mine == generation) _state.value = outcome
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
