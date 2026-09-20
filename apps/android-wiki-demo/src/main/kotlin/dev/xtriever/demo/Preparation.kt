package dev.xtriever.demo

import android.content.Context
import android.os.StatFs
import java.io.File

/**
 * First launch: the bundled models and corpus become real files in application storage
 * (Feature 025, spec FR-011; data-model "Preparation state").
 *
 * The marker is written last and read first. A directory without it is treated as absent and
 * extracted again, whatever it already contains, so an extraction interrupted by a kill or a
 * reboot cannot leave a half-written index that opens.
 */
class Preparation(
    private val context: Context,
    /** Free bytes in application storage; injectable so the refusal path can be tested. */
    private val freeBytes: () -> Long = { StatFs(context.filesDir.path).availableBytes },
) {

    /** Where everything ends up, once [State.Ready]. */
    data class Prepared(val corpusDir: File, val modelsDir: File)

    sealed interface State {
        data object Absent : State
        data class Extracting(val done: Int, val total: Int) : State
        data class Ready(val prepared: Prepared) : State
        data class Refused(val reason: String) : State
    }

    private val root = File(context.filesDir, "xtriever")
    private val marker = File(root, ".prepared")
    private val corpusDir = File(root, "corpus")
    private val modelsDir = File(root, "models")

    /** True when a previous run wrote the marker, so nothing needs copying. */
    fun isPrepared(): Boolean = marker.isFile && marker.readText().trim() == VERSION

    /**
     * Copies whatever the package carries, reporting progress. Returns [State.Ready] with the
     * directories, or [State.Refused] naming what is wrong — never a partial success.
     */
    fun prepare(onProgress: (State.Extracting) -> Unit = {}): State {
        if (isPrepared()) return State.Ready(Prepared(corpusDir, modelsDir))

        val assets = context.assets
        val bundled = listOf(MODELS, CORPUS).filter { Assets.exists(assets, it) }
        if (MODELS !in bundled) {
            return State.Refused(
                "this build carries no models — rebuild it with scripts/build-android-package.sh --with-models",
            )
        }
        if (CORPUS !in bundled) {
            return State.Refused(
                "this build carries no corpus — rebuild it with scripts/build-android-package.sh --with-corpus",
            )
        }

        val needed = bundled.sumOf { sizeOf(it) }
        val free = freeBytes()
        if (free < needed + HEADROOM) {
            return State.Refused(
                "needs ${needed / 1_000_000} MB of free space and there is ${free / 1_000_000} MB",
            )
        }

        // A stale directory is never opened: the marker is the only thing that makes one usable.
        marker.delete()
        root.deleteRecursively()
        root.mkdirs()

        val total = bundled.sumOf { Assets.count(assets, it) }
        var done = 0
        Assets.copy(assets, MODELS, modelsDir) { onProgress(State.Extracting(++done, total)) }
        Assets.copy(assets, CORPUS, corpusDir) { onProgress(State.Extracting(++done, total)) }
        marker.writeText(VERSION)
        return State.Ready(Prepared(corpusDir, modelsDir))
    }

    /** For tests and for the measurement runner: prepare, or fail loudly. */
    fun prepareBlocking(): Prepared = when (val state = prepare()) {
        is State.Ready -> state.prepared
        is State.Refused -> error(state.reason)
        else -> error("preparation did not finish: $state")
    }

    private fun sizeOf(path: String): Long {
        val children = context.assets.list(path).orEmpty()
        if (children.isEmpty()) {
            return runCatching { context.assets.openFd(path).length }
                .getOrElse { context.assets.open(path).use { it.available().toLong() } }
        }
        return children.sumOf { sizeOf("$path/$it") }
    }

    companion object {
        /** Bump when the staged resources change shape, so an old extraction is redone. */
        const val VERSION = "025.1"
        const val MODELS = "models"
        const val CORPUS = "corpus"
        private const val HEADROOM = 50L * 1000 * 1000
    }
}
