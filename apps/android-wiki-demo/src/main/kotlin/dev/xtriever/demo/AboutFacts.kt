package dev.xtriever.demo

import dev.xtriever.android.IndexInfo

/**
 * What the About screen states (Feature 025, spec FR-009), as label and value pairs so the
 * facts can be checked without a screen — the shape the Python demonstration's `about_lines`
 * has. Every value is the engine's or the corpus sidecar's.
 */
object AboutFacts {
    fun of(info: IndexInfo, corpus: CorpusSidecar, openMs: Long): List<Pair<String, String>> =
        corpus.facts.toList() + listOf(
            "passages (documents)" to info.documents.toString(),
            "format version" to info.formatVersion.toString(),
            "candidate depth" to info.candidateDepth.toString(),
            "rrf k" to info.rrfK.toString(),
            "dense compaction" to compaction(info),
            "re-rank depth (engine default)" to info.rerankDepth.toString(),
            "re-rank depth (app default)" to Settings.DEFAULT_DEPTH.toString(),
            "embedder" to info.embedderFingerprint,
            "re-ranker" to (info.rerankerModelId ?: "none"),
            "open" to "$openMs ms",
            "embedder load" to "${info.embedderLoadMs} ms",
            "re-ranker load" to (info.rerankerLoadMs?.let { "$it ms" } ?: "—"),
        )

    /**
     * The dense compaction share the index recorded (Feature 024). Unset — what every built
     * artefact says, since a build commits once and merges — means compaction happens only on
     * `merge`.
     */
    fun compaction(info: IndexInfo): String =
        info.denseCompactDeadShare?.let { "within a commit over ${(it * 100).toInt()}% dead rows" }
            ?: "on merge only (no share recorded)"
}
