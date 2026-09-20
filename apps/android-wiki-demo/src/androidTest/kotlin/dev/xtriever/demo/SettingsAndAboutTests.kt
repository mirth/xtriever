package dev.xtriever.demo

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.xtriever.android.XtrieverIndex
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Feature 025, User Story 3 (spec FR-009, FR-010): the choice survives a restart, and About
 * states what the engine and the corpus sidecar say — nothing the application made up.
 */
@RunWith(AndroidJUnit4::class)
class SettingsAndAboutTests {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun theDepthChoicesAreTheDemonstrationsAndTenIsTheDefault() {
        assertEquals(listOf(0, 5, 10, 20), Settings.DEPTHS)
        assertEquals(10, Settings.DEFAULT_DEPTH)
        assertEquals(10, Settings().rerankDepth)
    }

    @Test
    fun aChosenDepthSurvivesARestart() {
        SettingsStore(context).save(Settings(rerankDepth = 20))
        // A new store is what a fresh process sees.
        assertEquals(20, SettingsStore(context).load().rerankDepth)
        SettingsStore(context).save(Settings(rerankDepth = Settings.DEFAULT_DEPTH))
    }

    @Test
    fun anUnknownStoredDepthFallsBackToTheDefault() {
        context.getSharedPreferences("xtriever-demo", 0).edit().putInt("rerank-depth", 7).apply()
        assertEquals(Settings.DEFAULT_DEPTH, SettingsStore(context).load().rerankDepth)
    }

    @Test
    fun aboutStatesTheEnginesOwnAccountOfTheIndex() {
        val prepared = Preparation(context).prepareBlocking()
        XtrieverIndex.open(
            indexDir = File(prepared.corpusDir, "index"),
            embedderDir = File(prepared.modelsDir, "embedder"),
            rerankerDir = File(prepared.modelsDir, "reranker"),
        ).use { index ->
            val sidecar = CorpusSidecar.read(File(prepared.corpusDir, "corpus.json"))
            val facts = AboutFacts.of(index.info, sidecar, index.openMs).toMap()
            assertEquals(index.info.documents.toString(), facts["passages (documents)"])
            assertEquals(index.info.formatVersion.toString(), facts["format version"])
            assertEquals(index.info.embedderFingerprint, facts["embedder"])
            assertEquals(index.info.rrfK.toString(), facts["rrf k"])
            assertEquals("${index.openMs} ms", facts["open"])
            // Feature 024: every built artefact leaves the share unset, so the line says so.
            assertEquals(null, index.info.denseCompactDeadShare)
            assertEquals("on merge only (no share recorded)", facts["dense compaction"])
            assertTrue("the corpus identity comes from the sidecar", facts["corpus identity"]!!.length == 64)
        }
    }

    @Test
    fun theSidecarIsReadAsItStands() {
        val json = JSONObject(
            """{"corpus_identity":"abc","partial":2000,
                "snapshot":{"edition":"simple","snapshot_date":"2023-11-01"},
                "counts":{"articles":2000,"selected":1970,"passages":8529}}""",
        )
        val facts = CorpusSidecar.parse(json).facts
        assertEquals("Simple English Wikipedia (simple)", facts["corpus"])
        assertEquals("2023-11-01", facts["snapshot"])
        assertEquals("2000 read, 1970 selected", facts["articles"])
        assertEquals("8529", facts["passages"])
        assertEquals("first 2000 articles", facts["partial"])
        assertEquals("abc", facts["corpus identity"])
    }
}
