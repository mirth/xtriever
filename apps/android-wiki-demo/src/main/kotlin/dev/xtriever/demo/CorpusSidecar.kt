package dev.xtriever.demo

import org.json.JSONObject
import java.io.File

/**
 * The Feature 008 corpus sidecar, read as it stands (Feature 025, spec FR-009). The application
 * computes nothing from it: every line is what the build recorded.
 */
data class CorpusSidecar(val facts: Map<String, String>) {
    companion object {
        fun read(file: File): CorpusSidecar {
            if (!file.isFile) return CorpusSidecar(emptyMap())
            return parse(JSONObject(file.readText()))
        }

        fun parse(json: JSONObject): CorpusSidecar {
            val counts = json.optJSONObject("counts")
            val snapshot = json.optJSONObject("snapshot")
            return CorpusSidecar(
                buildMap {
                    snapshot?.let {
                        put("corpus", "Simple English Wikipedia (${it.optString("edition")})")
                        put("snapshot", it.optString("snapshot_date"))
                    }
                    counts?.let {
                        put("articles", "${it.optInt("articles")} read, ${it.optInt("selected")} selected")
                        put("passages", it.optInt("passages").toString())
                    }
                    if (json.has("partial") && !json.isNull("partial")) {
                        put("partial", "first ${json.opt("partial")} articles")
                    }
                    put("corpus identity", json.optString("corpus_identity"))
                },
            )
        }
    }
}
