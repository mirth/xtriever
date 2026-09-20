package dev.xtriever.android

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.BeforeClass
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Feature 025, User Story 1 (spec FR-004, SC-001): the engine's answers on Android match the
 * host's by the project's cross-device rule.
 *
 * The oracle is not a new fixture. It is `swift/Xtriever/Tests/Fixtures/expected.json` — the
 * same file the Swift package is checked against, minted on the host by
 * `crates/xtriever-ffi/examples/fixture_index.rs` — staged into this module's test assets by
 * `scripts/build-android-package.sh --with-fixtures`.
 *
 * **What is exact and what is not.** Hit identifiers, their order, the lexical score bits and
 * the fused score bits are compared bit for bit: they come from pure Rust and from rank
 * arithmetic, and no compilation target may move them. The two model-computed scores — dense
 * and re-rank — are compared within [TOLERANCE], because the inference engine's matrix kernels
 * round their reductions differently when compiled for this target (spec FR-004, owner's
 * decision 2026-09-20; measured gaps 1.3e-7 and 3.3e-6). [ParityCensusTest] prints the census
 * on every run, so drift beyond that is visible rather than absorbed. Neither side of this
 * split may be loosened to make a run pass (Agent Operating Rule 6).
 *
 * **What the goldens cover.** Each of the eight queries carries one re-rank depth — 5 — so
 * that is the depth this file replays against them, exactly as the Swift package's own parity
 * tests do. Depth 0 is covered too, by a different route: with the re-ranker attached but
 * `rerankDepth = 0` the stage must not run, so the answer must equal the goldens' no-re-ranker
 * case. Depths 10 and 20 have no goldens on any platform; covering them needs per-depth
 * goldens minted on the host, which would change the fixture the Swift tests share.
 */
@RunWith(AndroidJUnit4::class)
class FixtureParityTest {

    companion object {
        /** The cross-device tolerance for model-computed scores (spec FR-004). */
        private const val TOLERANCE = 1e-3f

        private lateinit var staged: File
        private lateinit var indexDir: File
        private lateinit var expected: JSONObject

        /** Test assets live inside the package; the engine memory-maps real files (research D6). */
        @BeforeClass
        @JvmStatic
        fun stage() {
            if (::indexDir.isInitialized) return
            val context = InstrumentationRegistry.getInstrumentation().context
            val target = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "fixture")
            copyAsset(context, "fixture", target)
            staged = target
            indexDir = File(target, "index")
            expected = JSONObject(File(target, "expected.json").readText())
        }

        /** Shared with [ParityCensusTest]: the staged fixture, opened the same way. */
        fun openStaged(withReranker: Boolean): XtrieverIndex {
            stage()
            return openStagedInternal(withReranker)
        }

        private fun openStagedInternal(withReranker: Boolean): XtrieverIndex = XtrieverIndex.open(
            indexDir = indexDir,
            embedderDir = File(staged, "models/embedder"),
            rerankerDir = if (withReranker) File(staged, "models/reranker") else null,
        )

        /** Shared with [ParityCensusTest]: every golden query. */
        fun eachQuery(body: (JSONObject) -> Unit) {
            stage()
            val queries = expected.getJSONArray("queries")
            for (i in 0 until queries.length()) body(queries.getJSONObject(i))
        }

        private fun copyAsset(context: Context, path: String, destination: File) {
            val children = context.assets.list(path) ?: emptyArray()
            if (children.isEmpty()) {
                destination.parentFile?.mkdirs()
                context.assets.open(path).use { input ->
                    destination.outputStream().use { output -> input.copyTo(output) }
                }
                return
            }
            destination.mkdirs()
            children.forEach { copyAsset(context, "$path/$it", File(destination, it)) }
        }
    }

    @Test
    fun theDeviceMeetsTheProcessorRequirement() {
        // Without half precision every call below dies on an illegal instruction rather than
        // failing an assertion, so the requirement is checked first and named (research D5).
        val support = DeviceSupport.check()
        assertTrue("unsupported processor: $support", support is DeviceSupport.Supported)
    }

    @Test
    fun hitsEqualTheGoldensWithTheReranker() {
        // Each golden query carries its own re-rank depth; the fixture mints them all at 5.
        openFixture(withReranker = true).use { index ->
            forEachQuery { query ->
                val response = index.search(
                    query.getString("text"),
                    k = query.getInt("k"),
                    rerankDepth = query.getInt("rerank_depth"),
                    explain = true,
                )
                assertParity(response, query.getJSONObject("with_reranker"), "${query.getString("id")} with re-ranker")
                assertNotNull(query.getString("id"), response.stages.rerank)
            }
        }
    }

    @Test
    fun hitsEqualTheGoldensWithoutTheReranker() {
        openFixture(withReranker = false).use { index ->
            forEachQuery { query ->
                val response = index.search(
                    query.getString("text"),
                    k = query.getInt("k"),
                    rerankDepth = query.getInt("rerank_depth"),
                    explain = true,
                )
                assertParity(response, query.getJSONObject("without_reranker"), "${query.getString("id")} without re-ranker")
                assertNull(query.getString("id"), response.stages.rerank)
                assertTrue(response.hits.all { it.rerankScore == null })
            }
        }
    }

    @Test
    fun depthZeroEqualsTheNoRerankerGoldens() {
        // The re-ranker is attached and must still not run: the engine's answer at depth 0 is
        // the fused list, which is what the no-re-ranker goldens hold.
        openFixture(withReranker = true).use { index ->
            forEachQuery { query ->
                val response = index.search(
                    query.getString("text"),
                    k = query.getInt("k"),
                    rerankDepth = 0,
                    explain = true,
                )
                assertParity(response, query.getJSONObject("without_reranker"), "${query.getString("id")} at depth 0")
                assertTrue("${query.getString("id")}: no hit may carry a re-rank score", response.hits.all { it.rerankScore == null })
            }
        }
    }

    @Test
    fun theIndexInformationEqualsTheGoldens() {
        openFixture(withReranker = true).use { index ->
            val info = expected.getJSONObject("info")
            assertEquals(info.getLong("documents").toULong(), index.info.documents)
            assertEquals(info.getInt("format_version").toUInt(), index.info.formatVersion)
            assertEquals(info.getString("embedder_fingerprint"), index.info.embedderFingerprint)
            assertEquals(info.getInt("candidate_depth").toUInt(), index.info.candidateDepth)
        }
    }

    // ── the comparison rule (spec FR-004, owner's decision 2026-09-20) ───────────────────────
    //
    // Identifiers, their order, the lexical bits and the fused bits are exact: those come from
    // pure Rust and from rank arithmetic, and nothing about a compilation target may move them.
    // The two model-computed scores are compared within 1e-3, the tolerance this project
    // already applies across devices (Features 009 and 019), because the inference engine's
    // matrix kernels round their reductions differently when compiled for this target. The
    // measured gaps are 1.3e-7 and 3.3e-6; ParityCensusTest prints the census each run, so
    // drift beyond what was established is visible rather than absorbed.

    private fun assertParity(response: SearchResponse, golden: JSONObject, label: String) {
        val hits = golden.getJSONArray("hits")
        assertEquals("$label: hit count", hits.length(), response.hits.size)
        for (i in 0 until hits.length()) {
            val want = hits.getJSONObject(i)
            val got = response.hits[i]
            assertEquals("$label #$i: identifier", want.getString("external_id"), got.externalId)
            assertEquals("$label #$i: fused score bits", want.getString("score_bits"), got.score.toRawBits().toHexString())
            assertEquals("$label #$i: bm25 score bits", want.bits("bm25_score_bits"), got.explain?.bm25Score?.toRawBits()?.toHexString())
            assertWithin("$label #$i: dense score", want.bits("dense_score_bits"), got.explain?.denseScore)
            assertWithin("$label #$i: re-rank score", want.bits("rerank_score_bits"), got.rerankScore)
        }
    }

    /** A model-computed score: present or absent exactly as the goldens have it, and within [TOLERANCE]. */
    private fun assertWithin(label: String, goldenBits: String?, got: Float?) {
        if (goldenBits == null) {
            assertNull("$label: expected absent", got)
            return
        }
        assertNotNull("$label: expected present", got)
        val want = Float.fromBits(java.lang.Long.parseUnsignedLong(goldenBits, 16).toInt())
        val difference = kotlin.math.abs(want - got!!)
        assertTrue(
            "$label: |$want - $got| = $difference exceeds $TOLERANCE",
            difference <= TOLERANCE,
        )
    }

    /** A golden field that may be absent, as `null` rather than the string "null". */
    private fun JSONObject.bits(name: String): String? = if (isNull(name)) null else getString(name)

    private fun forEachQuery(body: (JSONObject) -> Unit) {
        val queries = expected.getJSONArray("queries")
        for (i in 0 until queries.length()) body(queries.getJSONObject(i))
    }

    private fun openFixture(withReranker: Boolean): XtrieverIndex =
        XtrieverIndex.open(
            indexDir = indexDir,
            embedderDir = File(staged, "models/embedder"),
            rerankerDir = if (withReranker) File(staged, "models/reranker") else null,
        )

    private fun Int.toHexString(): String = java.lang.String.format("%08x", this)

    private fun Long.toHexString(): String = java.lang.String.format("%016x", this)
}
