package dev.xtriever.demo

/**
 * Title, passage and article link, derived from a hit's text by the Feature 008 convention
 * (spec FR-007) — the same rule the Swift package's `titleAndPassage` / `wikipediaURL` and the
 * Python demo's `hits.py` apply, so all three demonstrations link to the same article.
 */
object HitText {
    private const val BASE = "https://simple.wikipedia.org/wiki/"
    /** Everything outside this set is percent-encoded, as the Feature 008 URL cases require. */
    private const val UNRESERVED = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~/"

    /** The title is the first line before a blank line; without one there is no title. */
    fun split(text: String): Pair<String?, String> {
        val separator = text.indexOf("\n\n")
        if (separator < 0) return null to text
        return text.substring(0, separator).trim() to text.substring(separator + 2)
    }

    /** The article's link, or null for a hit with no title. */
    fun articleUrl(title: String?): String? {
        if (title == null) return null
        val encoded = StringBuilder()
        for (byte in title.toByteArray(Charsets.UTF_8)) {
            val ch = byte.toInt().toChar()
            if (ch in UNRESERVED) encoded.append(ch) else encoded.append("%%%02X".format(byte.toInt() and 0xFF))
        }
        return BASE + encoded
    }
}
