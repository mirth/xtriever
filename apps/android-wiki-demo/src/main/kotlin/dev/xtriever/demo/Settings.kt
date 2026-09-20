package dev.xtriever.demo

import android.content.Context

/**
 * What the person chose (Feature 025, spec FR-010). The re-rank depth is the app's one real
 * choice: the engine's own default is 20, the demonstrations default to 10 (Feature 018), and
 * 0 shows the fused list alone.
 */
data class Settings(
    val rerankDepth: Int = DEFAULT_DEPTH,
    val maxTimeMs: Long? = null,
    val strict: Boolean = false,
) {
    companion object {
        const val DEFAULT_DEPTH = 10
        val DEPTHS = listOf(0, 5, 10, 20)
    }
}

/** Where the choice survives a restart. */
class SettingsStore(context: Context) {
    private val preferences = context.getSharedPreferences("xtriever-demo", Context.MODE_PRIVATE)

    fun load(): Settings {
        val depth = preferences.getInt(KEY_DEPTH, Settings.DEFAULT_DEPTH)
        return Settings(rerankDepth = if (depth in Settings.DEPTHS) depth else Settings.DEFAULT_DEPTH)
    }

    fun save(settings: Settings) {
        preferences.edit().putInt(KEY_DEPTH, settings.rerankDepth).apply()
    }

    private companion object {
        const val KEY_DEPTH = "rerank-depth"
    }
}
