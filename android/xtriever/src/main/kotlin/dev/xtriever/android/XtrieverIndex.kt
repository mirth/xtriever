package dev.xtriever.android

import java.io.Closeable
import java.io.File
import uniffi.xtriever_ffi.IndexHandle
import uniffi.xtriever_ffi.LoadPath

// The generated types are the surface; the module re-exports them under its own package so a
// consumer never imports `uniffi.*` directly, exactly as the Swift package re-exports them.
typealias SearchOptions = uniffi.xtriever_ffi.SearchOptions
typealias SearchResponse = uniffi.xtriever_ffi.SearchResponse
typealias Hit = uniffi.xtriever_ffi.Hit
typealias HitExplain = uniffi.xtriever_ffi.HitExplain
typealias StageReport = uniffi.xtriever_ffi.StageReport
typealias IndexInfo = uniffi.xtriever_ffi.IndexInfo
typealias RerankMode = uniffi.xtriever_ffi.RerankMode
// A type alias cannot reach a nested classifier, so the two modes are re-exported by name.
typealias RerankInterpolate = uniffi.xtriever_ffi.RerankMode.Interpolate
typealias RerankReplace = uniffi.xtriever_ffi.RerankMode.Replace
typealias XtrieverException = uniffi.xtriever_ffi.XtrieverException

/**
 * An open Xtriever index on Android (Feature 025).
 *
 * Mirrors `swift/Xtriever/Sources/Xtriever/XtrieverIndex.swift`: it opens the index, holds what
 * it opened and how long that took, and closes deterministically. It adds no capability the
 * Swift wrapper lacks (spec FR-001) and owns no retrieval logic — every number it returns is
 * the engine's.
 *
 * The directories must be real files the operating system can map. Assets inside the
 * application package are not: copy them into application storage first (research D6).
 */
class XtrieverIndex private constructor(
    private val handle: IndexHandle,
    /** Where this index and its models were opened from. */
    val paths: Paths,
    /** Wall time the open itself took, in milliseconds. */
    val openMs: Long,
    /**
     * The engine's own account of the index — documents, format version, fingerprints, depths,
     * the recorded re-rank mode and dense compaction share.
     *
     * Read once at open and held, as the Swift wrapper holds it: it describes what was opened,
     * so it stays readable after [close] instead of calling through a destroyed handle.
     */
    val info: IndexInfo,
) : Closeable {

    /** What was opened. */
    data class Paths(val indexDir: File, val embedderDir: File, val rerankerDir: File?)

    /**
     * One search, with the engine's own options record — the whole surface, nothing withheld.
     *
     * @throws XtrieverException as the engine raised it — nothing is translated or swallowed.
     */
    fun search(query: String, options: SearchOptions): SearchResponse = handle.search(query, options)

    /**
     * One search, for callers who would rather pass numbers than build a record. Every field of
     * [SearchOptions] is here: [k] hits, [depth] candidates per stage, [rerankDepth] fused
     * candidates re-scored (`0` for the fused list alone, `null` for the index's default),
     * [rerankMode] how the re-ranked head is ordered, [maxTimeMs] a time budget, [maxItems] an
     * item budget, [strict] to raise a stage's error instead of degrading, [explain] to attach
     * per-stage features.
     *
     * Negative values are refused rather than wrapped into enormous unsigned ones.
     *
     * @throws IllegalArgumentException if any count or budget is negative.
     * @throws XtrieverException as the engine raised it.
     */
    fun search(
        query: String,
        k: Int,
        depth: Int? = null,
        rerankDepth: Int? = null,
        rerankMode: RerankMode? = null,
        maxTimeMs: Long? = null,
        maxItems: Int? = null,
        strict: Boolean = false,
        explain: Boolean = false,
    ): SearchResponse = search(
        query,
        SearchOptions(
            k = k.count("k"),
            depth = depth?.count("depth"),
            rerankDepth = rerankDepth?.count("rerankDepth"),
            rerankMode = rerankMode,
            maxTimeMs = maxTimeMs?.also { require(it >= 0) { "maxTimeMs must not be negative: $it" } }?.toULong(),
            maxItems = maxItems?.count("maxItems"),
            strict = strict,
            explain = explain,
        ),
    )

    private fun Int.count(name: String): UInt {
        require(this >= 0) { "$name must not be negative: $this" }
        return toUInt()
    }

    override fun close() = handle.close()

    companion object {
        /**
         * Opens [indexDir] with the embedder at [embedderDir] and, when given, the re-ranker at
         * [rerankerDir].
         *
         * @throws IllegalStateException before anything native is touched when the processor
         *   cannot run the library (see [DeviceSupport]).
         * @throws XtrieverException when the engine refuses the directory — a missing file, a
         *   fingerprint mismatch, or a dense stage still on format version 1.
         */
        fun open(
            indexDir: File,
            embedderDir: File,
            rerankerDir: File? = null,
            mapped: Boolean = true,
        ): XtrieverIndex {
            when (val support = DeviceSupport.check()) {
                is DeviceSupport.Unsupported -> error(support.reason)
                DeviceSupport.Supported -> Unit
            }
            val started = System.nanoTime()
            val handle = IndexHandle.open(
                indexDir.absolutePath,
                embedderDir.absolutePath,
                rerankerDir?.absolutePath,
                if (mapped) LoadPath.MMAP else LoadPath.BUFFERED,
            )
            val openMs = (System.nanoTime() - started) / 1_000_000
            return XtrieverIndex(handle, Paths(indexDir, embedderDir, rerankerDir), openMs, handle.info())
        }
    }
}
