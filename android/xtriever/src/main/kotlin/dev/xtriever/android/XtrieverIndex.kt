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
) : Closeable {

    /** What was opened. */
    data class Paths(val indexDir: File, val embedderDir: File, val rerankerDir: File?)

    /** The engine's own account of the index — documents, format version, fingerprints, depths. */
    val info: IndexInfo by lazy { handle.info() }

    /**
     * One search. [k] hits, [rerankDepth] fused candidates re-scored (`0` for the fused list
     * alone, `null` for the index's default), [explain] to attach per-stage features.
     *
     * @throws XtrieverException as the engine raised it — nothing is translated or swallowed.
     */
    fun search(
        query: String,
        k: Int,
        rerankDepth: Int? = null,
        explain: Boolean = false,
        maxTimeMs: Long? = null,
        strict: Boolean = false,
        rerankMode: RerankMode? = null,
    ): SearchResponse = handle.search(
        query,
        SearchOptions(
            k = k.toUInt(),
            rerankDepth = rerankDepth?.toUInt(),
            rerankMode = rerankMode,
            maxTimeMs = maxTimeMs?.toULong(),
            strict = strict,
            explain = explain,
        ),
    )

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
            return XtrieverIndex(handle, Paths(indexDir, embedderDir, rerankerDir), openMs)
        }
    }
}
