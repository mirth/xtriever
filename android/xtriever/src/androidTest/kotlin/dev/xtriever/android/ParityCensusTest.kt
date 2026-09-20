package dev.xtriever.android

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Test
import org.junit.runner.RunWith
import kotlin.math.abs

/**
 * Diagnostic, not a gate: counts *how* Android's answers differ from the host goldens, so a
 * difference can be described rather than guessed at. The gate is [FixtureParityTest].
 */
@RunWith(AndroidJUnit4::class)
class ParityCensusTest {

    @Test
    fun census() {
        val report = StringBuilder()
        for (withReranker in listOf(true, false)) {
            var hits = 0
            var idMismatch = 0
            var bm25Diff = 0
            var denseDiff = 0
            var rerankDiff = 0
            var maxDense = 0.0f
            var maxRerank = 0.0f
            var maxFused = 0.0
            var fusedDiff = 0
            FixtureParityTest.openStaged(withReranker).use { index ->
                FixtureParityTest.eachQuery { query ->
                    val response = index.search(
                        query.getString("text"),
                        k = query.getInt("k"),
                        rerankDepth = query.getInt("rerank_depth"),
                        explain = true,
                    )
                    val golden = query.getJSONObject(if (withReranker) "with_reranker" else "without_reranker")
                        .getJSONArray("hits")
                    for (i in 0 until minOf(golden.length(), response.hits.size)) {
                        val want = golden.getJSONObject(i)
                        val got = response.hits[i]
                        hits++
                        if (want.getString("external_id") != got.externalId) idMismatch++
                        val wantFused = java.lang.Long.parseUnsignedLong(want.getString("score_bits"), 16)
                        if (wantFused != got.score.toRawBits()) {
                            fusedDiff++
                            maxFused = maxOf(maxFused, abs(Double.fromBits(wantFused) - got.score))
                        }
                        compare(want, "bm25_score_bits", got.explain?.bm25Score)?.let { bm25Diff++ }
                        compare(want, "dense_score_bits", got.explain?.denseScore)?.let {
                            denseDiff++; maxDense = maxOf(maxDense, it)
                        }
                        compare(want, "rerank_score_bits", got.rerankScore)?.let {
                            rerankDiff++; maxRerank = maxOf(maxRerank, it)
                        }
                    }
                }
            }
            report.append(
                "\n${if (withReranker) "with" else "without"} re-ranker: $hits hits · " +
                    "identifiers differing $idMismatch · order preserved ${idMismatch == 0} · " +
                    "bm25 bits differing $bm25Diff · dense bits differing $denseDiff (max |Δ| $maxDense) · " +
                    "re-rank bits differing $rerankDiff (max |Δ| $maxRerank) · " +
                    "fused bits differing $fusedDiff (max |Δ| $maxFused)",
            )
        }
        Log.i("XtrieverParity", report.toString())
        println("PARITY CENSUS$report")
    }

    /** Returns the absolute difference when the bits differ, else null. */
    private fun compare(golden: org.json.JSONObject, field: String, got: Float?): Float? {
        if (golden.isNull(field) || got == null) return null
        val want = Float.fromBits(java.lang.Long.parseUnsignedLong(golden.getString(field), 16).toInt())
        return if (want.toRawBits() == got.toRawBits()) null else abs(want - got)
    }
}
