package dev.xtriever.demo

import android.content.res.AssetManager
import java.io.File

/**
 * Copying a bundled directory out of the application package (Feature 025, research D6).
 *
 * The engine memory-maps the vectors and both model weight files, and an asset is not a file
 * the operating system can map — it lives inside the package archive. Each file is written to a
 * temporary name and renamed, so an interrupted copy leaves nothing that looks complete.
 */
object Assets {
    fun copy(assets: AssetManager, path: String, destination: File, onFile: (String) -> Unit = {}) {
        val children = assets.list(path).orEmpty()
        if (children.isEmpty()) {
            destination.parentFile?.mkdirs()
            val temporary = File(destination.parentFile, destination.name + ".part")
            assets.open(path).use { input -> temporary.outputStream().use { output -> input.copyTo(output) } }
            check(temporary.renameTo(destination)) { "cannot place ${destination.path}" }
            onFile(path)
            return
        }
        destination.mkdirs()
        children.forEach { copy(assets, "$path/$it", File(destination, it), onFile) }
    }

    /** How many files a bundled directory holds, for a progress fraction. */
    fun count(assets: AssetManager, path: String): Int {
        val children = assets.list(path).orEmpty()
        return if (children.isEmpty()) 1 else children.sumOf { count(assets, "$path/$it") }
    }

    /** Whether the package carries this directory at all. */
    fun exists(assets: AssetManager, path: String): Boolean =
        assets.list(path).orEmpty().isNotEmpty() || runCatching { assets.open(path).close() }.isSuccess
}
