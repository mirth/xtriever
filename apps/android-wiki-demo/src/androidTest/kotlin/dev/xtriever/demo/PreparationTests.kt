package dev.xtriever.demo

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Feature 025, User Story 3 (spec FR-011): the first launch turns bundled assets into real
 * files, and an interrupted one never leaves something that looks finished.
 */
@RunWith(AndroidJUnit4::class)
class PreparationTests {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val root = File(context.filesDir, "xtriever")

    @Test
    fun aFreshInstallReachesReadyWithProgress() {
        root.deleteRecursively()
        var lastSeen = 0
        val state = Preparation(context).prepare { lastSeen = it.done }
        assertTrue("$state", state is Preparation.State.Ready)
        val ready = state as Preparation.State.Ready
        assertTrue("the index is a real file the engine can map", File(ready.prepared.corpusDir, "index").isDirectory)
        assertTrue("both models are extracted", File(ready.prepared.modelsDir, "embedder").isDirectory)
        assertTrue("progress was reported", lastSeen > 0)
        assertTrue(Preparation(context).isPrepared())
    }

    @Test
    fun anInterruptedExtractionIsRedoneRatherThanOpened() {
        // What a kill mid-copy leaves: files present, marker absent.
        Preparation(context).prepare()
        File(root, ".prepared").delete()
        assertFalse("without the marker nothing counts as prepared", Preparation(context).isPrepared())

        val state = Preparation(context).prepare()
        assertTrue("$state", state is Preparation.State.Ready)
        assertTrue("and the marker is back", Preparation(context).isPrepared())
    }

    @Test
    fun tooLittleSpaceIsRefusedBeforeAnythingIsWritten() {
        root.deleteRecursively()
        val state = Preparation(context, freeBytes = { 1_000_000 }).prepare()
        assertTrue("$state", state is Preparation.State.Refused)
        val reason = (state as Preparation.State.Refused).reason
        assertTrue("the reason names the space needed: $reason", reason.contains("MB of free space"))
        assertFalse("nothing was written", root.exists() && root.listFiles()?.isNotEmpty() == true)
    }

    @Test
    fun aStaleMarkerFromAnotherVersionIsNotTrusted() {
        Preparation(context).prepare()
        File(root, ".prepared").writeText("024.9")
        assertFalse("a marker from another staging version does not count", Preparation(context).isPrepared())
    }
}
