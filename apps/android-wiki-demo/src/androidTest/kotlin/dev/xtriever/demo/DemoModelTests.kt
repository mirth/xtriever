package dev.xtriever.demo

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.xtriever.android.XtrieverIndex
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runTest
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Feature 025, User Story 2 (spec FR-005, FR-006, FR-008, FR-012): the demonstration's own
 * logic, driven against the 40-document fixture index and its committed goldens — no 24 MB
 * corpus needed, as the iOS demo's model tests do it.
 *
 * The model owns no retrieval logic. These tests check that it asks the engine twice, keeps the
 * engine's answers unchanged, computes the marks between them, and stays usable when a search
 * is empty, cancelled or cut short.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(AndroidJUnit4::class)
class DemoModelTests {

    private lateinit var index: XtrieverIndex
    private lateinit var expected: JSONObject
    private lateinit var model: DemoModel

    @Before
    fun open() {
        val staged = TestFixture.stage()
        expected = JSONObject(File(staged, "expected.json").readText())
        index = XtrieverIndex.open(
            indexDir = File(staged, "index"),
            embedderDir = TestFixture.embedderDir(),
            rerankerDir = TestFixture.rerankerDir(),
        )
        model = DemoModel(index, Settings(rerankDepth = 10))
    }

    @After
    fun close() {
        index.close()
    }

    @Test
    fun theFusedAndRerankedListsAreTheEnginesAnswers() = runTest {
        val query = expected.getJSONArray("queries").getJSONObject(0)
        val outcome = model.search(query.getString("text"))

        val fusedGolden = query.getJSONObject("without_reranker").getJSONArray("hits")
        assertEquals(
            "the fused list is the engine's depth-0 answer",
            (0 until fusedGolden.length()).map { fusedGolden.getJSONObject(it).getString("external_id") },
            outcome.fused.map { it.externalId },
        )
        assertTrue("every re-ranked hit carries the engine's re-rank score", outcome.reranked.all { it.rerankScore != null })
        assertNotNull("the stage report is the engine's", outcome.report.rerank)
        assertEquals("the marks cover every re-ranked hit", outcome.reranked.size, outcome.marks.size)
    }

    @Test
    fun depthZeroRunsOnlyTheFusedSearch() = runTest {
        model.settings = Settings(rerankDepth = 0)
        val outcome = model.search(expected.getJSONArray("queries").getJSONObject(0).getString("text"))
        assertEquals("nothing is re-ranked", outcome.fused.map { it.externalId }, outcome.reranked.map { it.externalId })
        assertTrue(outcome.reranked.all { it.rerankScore == null })
        assertNull("the re-rank stage did not run", outcome.report.rerank)
        assertEquals("no movement to mark", 0L, outcome.rerankedMs)
    }

    @Test
    fun anEmptyQueryIsAnEmptyResultNotAnError() = runTest {
        val outcome = model.search("   ")
        assertTrue(outcome.fused.isEmpty() && outcome.reranked.isEmpty())
        assertTrue("the model stays usable", model.search("zephyr").fused.isNotEmpty())
    }

    @Test
    fun aSpentBudgetDegradesInTheReportRatherThanFailing() = runTest {
        model.settings = Settings(rerankDepth = 10, maxTimeMs = 0)
        val outcome = model.search("zephyr")
        assertTrue("hits still come back", outcome.fused.isNotEmpty())
        val degraded = outcome.report.degraded != null || outcome.report.rerank?.skipped != null
        assertTrue("the report says a stage was cut short", degraded)
    }

    @Test
    fun aStrictSpentBudgetIsTheEnginesErrorAndTheModelStaysUsable() = runTest {
        model.settings = Settings(rerankDepth = 10, maxTimeMs = 0, strict = true)
        val failure = runCatching { model.search("zephyr") }.exceptionOrNull()
        assertNotNull("strict mode raises the engine's error", failure)
        model.settings = Settings(rerankDepth = 10)
        assertTrue("the model is still usable afterwards", model.search("zephyr").fused.isNotEmpty())
    }

    @Test
    fun theFusedAnswerIsPublishedBeforeTheRerankedOne() = runTest {
        // Spec US2 scenario 1: the fused list appears first. The model must publish it while
        // the cross-encoder is still working, not only when both calls have finished.
        var fusedSeen: DemoModel.Outcome? = null
        val complete = model.search("zephyr") { fusedSeen = it }

        val fused = requireNotNull(fusedSeen) { "the fused answer was never published" }
        assertEquals("published while the cross-encoder was still working", DemoModel.Phase.FUSED, fused.phase)
        assertTrue("with hits to show", fused.fused.isNotEmpty())
        assertTrue("and nothing re-ranked yet", fused.reranked.isEmpty())
        assertEquals("the complete answer follows", DemoModel.Phase.COMPLETE, complete.phase)
        assertEquals("and is what the screen ends on", DemoModel.Phase.COMPLETE, model.state.value?.phase)
    }

    @Test
    fun aStaleSearchNeverOverwritesANewerOne() = runTest {
        // Cancellation cannot interrupt a native call that has already started, so the model
        // drops the older answer by generation rather than trusting the coroutine to stop.
        model.submit(this, "zephyr")
        model.submit(this, "obsidian")
        model.awaitIdle()
        assertEquals("obsidian", model.state.value?.query)
        assertEquals(DemoModel.Phase.COMPLETE, model.state.value?.phase)
    }

    @Test
    fun aSecondSubmissionCancelsTheFirst() = runTest {
        val scope = this
        model.submit(scope, "zephyr")
        model.submit(scope, "obsidian")
        model.awaitIdle()
        assertEquals("only the second query's results are shown", "obsidian", model.state.value?.query)
    }

    companion object {
        /** Test assets are staged by `scripts/build-android-package.sh --with-fixtures`. */
        object TestFixture {
            fun stage(): File {
                val instrumentation = InstrumentationRegistry.getInstrumentation()
                val target = File(instrumentation.targetContext.cacheDir, "fixture")
                Assets.copy(instrumentation.context.assets, "fixture", target)
                return target
            }

            fun embedderDir(): File = File(preparedModels(), "embedder")

            fun rerankerDir(): File = File(preparedModels(), "reranker")

            /** The application's own bundled models, extracted once, as the application does. */
            private fun preparedModels(): File {
                val context = InstrumentationRegistry.getInstrumentation().targetContext
                return Preparation(context).prepareBlocking().modelsDir
            }
        }
    }
}
