package dev.xtriever.demo

import android.os.Build
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.xtriever.android.XtrieverIndex
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.math.abs

/**
 * Feature 025, User Story 4 (spec FR-017, SC-003, SC-004): the twenty measurement queries on the
 * bundled corpus, compared with the host's answers for the same corpus and written as a record.
 *
 * The comparison is the FR-004 rule: identifiers, order, lexical bits and fused bits exact;
 * dense and re-rank scores within 1e-3. The goldens come from `xtriever wiki expected` on the
 * same index, so this covers all four re-rank depths — unlike the 40-document fixture, whose
 * goldens carry one depth.
 *
 * The record it writes is an **emulator** record and says so: its latency and memory figures
 * describe a virtual device on a host machine and must not be read as a phone's.
 */
@RunWith(AndroidJUnit4::class)
class MeasurementTest {

    private val instrumentation = InstrumentationRegistry.getInstrumentation()

    @Test
    fun theDeviceAgreesWithTheHostAndTheRunIsRecorded() {
        val prepared = Preparation(instrumentation.targetContext).prepareBlocking()
        val staged = File(instrumentation.targetContext.cacheDir, "measurement").also {
            Assets.copy(instrumentation.context.assets, "corpus", it)
        }
        val expected = JSONObject(File(staged, "expected.json").readText())
        val queries = expected.getJSONArray("queries")

        val perDepth = sortedMapOf<Int, MutableList<Long>>()
        var hitsCompared = 0
        var identical = 0
        var maxDense = 0.0f
        var maxRerank = 0.0f
        var orderMismatches = 0
        var lexicalMismatches = 0
        var fusedMismatches = 0

        XtrieverIndex.open(
            indexDir = File(prepared.corpusDir, "index"),
            embedderDir = File(prepared.modelsDir, "embedder"),
            rerankerDir = File(prepared.modelsDir, "reranker"),
        ).use { index ->
            // The first search of a process pages the vectors in; it is the warm-up everywhere
            // else this is measured, and it is excluded here too.
            index.search(queries.getJSONObject(0).getString("text"), k = 10, rerankDepth = 10)

            for (q in 0 until queries.length()) {
                val query = queries.getJSONObject(q)
                val depths = query.getJSONObject("depths")
                for (depth in DEPTHS) {
                    val golden = depths.optJSONObject(depth.toString()) ?: continue
                    val started = System.nanoTime()
                    val response = index.search(query.getString("text"), k = 10, rerankDepth = depth, explain = true)
                    perDepth.getOrPut(depth) { mutableListOf() }.add((System.nanoTime() - started) / 1_000_000)

                    val wanted = golden.getJSONArray("hits")
                    assertEquals("${query.getString("id")} depth $depth: hit count", wanted.length(), response.hits.size)
                    for (i in 0 until wanted.length()) {
                        val want = wanted.getJSONObject(i)
                        val got = response.hits[i]
                        hitsCompared++
                        if (want.getString("external_id") != got.externalId) orderMismatches++
                        if (bits(want, "score_bits") != got.score.toRawBits().toULong().toString(16).padStart(16, '0')) fusedMismatches++
                        val bm25 = floatOf(want, "bm25_score_bits")
                        if (bm25 != null && got.explain?.bm25Score?.toRawBits() != bm25.toRawBits()) lexicalMismatches++
                        val dense = floatOf(want, "dense_score_bits")
                        if (dense != null && got.explain?.denseScore != null) {
                            maxDense = maxOf(maxDense, abs(dense - got.explain!!.denseScore!!))
                        }
                        val rerank = floatOf(want, "rerank_score_bits")
                        if (rerank != null && got.rerankScore != null) {
                            maxRerank = maxOf(maxRerank, abs(rerank - got.rerankScore!!))
                        }
                        if (want.getString("external_id") == got.externalId) identical++
                    }
                }
            }
        }

        // The rule (spec FR-004): the ranking is exact, the model scores are within 1e-3.
        assertEquals("hit identifiers and their order", 0, orderMismatches)
        assertEquals("lexical score bits", 0, lexicalMismatches)
        assertEquals("fused score bits", 0, fusedMismatches)
        assertTrue("dense scores within 1e-3 (max $maxDense)", maxDense <= TOLERANCE)
        assertTrue("re-rank scores within 1e-3 (max $maxRerank)", maxRerank <= TOLERANCE)

        val record = JSONObject().apply {
            put("emulated", true)
            put(
                "claims",
                "Correctness is a real claim: identifiers, order, lexical bits and fused bits are " +
                    "identical to the host's and the model scores are within 1e-3. Latency and memory " +
                    "describe an emulated device on a host machine and are not a physical-device result; " +
                    "they must not be compared with the iPhone's record.",
            )
            put("device", JSONObject().apply {
                put("model", Build.MODEL)
                put("systemImage", "${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})")
                put("abi", Build.SUPPORTED_ABIS.firstOrNull())
            })
            put("threads", Runtime.getRuntime().availableProcessors())
            put("threadsSource", "Runtime.availableProcessors")
            put("loadPath", "mmap")
            put("index", JSONObject(expected.getJSONObject("info").toString()))
            put("queries", queries.length())
            put("perDepth", JSONObject().apply {
                perDepth.forEach { (depth, times) ->
                    put(depth.toString(), JSONObject().apply {
                        put("medianMs", times.sorted()[times.size / 2])
                        put("maxMs", times.max())
                    })
                }
            })
            put("peakResidentBytes", peakResidentBytes())
            put("ceilingBytes", 600_000_000)
            put("parity", JSONObject().apply {
                put("verdict", "PASS")
                put("hitsCompared", hitsCompared)
                put("identifiersIdentical", identical)
                put("orderMismatches", orderMismatches)
                put("lexicalBitMismatches", lexicalMismatches)
                put("fusedBitMismatches", fusedMismatches)
                put("denseMaxAbsoluteDifference", maxDense.toDouble())
                put("rerankMaxAbsoluteDifference", maxRerank.toDouble())
                put("tolerance", TOLERANCE.toDouble())
            })
        }

        // The application's own storage, not external storage: `adb exec-as` can read this one
        // on a debuggable build, and scoped storage hides the other from the shell user.
        val out = File(instrumentation.targetContext.filesDir, "measurement.json")
        out.writeText(record.toString(2))
        Log.i(TAG, "wrote ${out.absolutePath}")
        // logcat truncates long lines; the file is the record, the log only points at it.
    }

    /** Peak resident size as the kernel reports it, in bytes. */
    private fun peakResidentBytes(): Long =
        File("/proc/self/status").readLines()
            .firstOrNull { it.startsWith("VmHWM:") }
            ?.filter { it.isDigit() }?.toLongOrNull()?.times(1024) ?: -1

    private fun bits(golden: JSONObject, field: String): String? =
        if (golden.isNull(field)) null else golden.getString(field)

    private fun floatOf(golden: JSONObject, field: String): Float? =
        bits(golden, field)?.let { Float.fromBits(java.lang.Long.parseUnsignedLong(it, 16).toInt()) }

    private companion object {
        const val TAG = "XtrieverMeasure"
        const val TOLERANCE = 1e-3f
        val DEPTHS = listOf(0, 5, 10, 20)
    }
}
