package dev.xtriever.demo

/**
 * How a hit's position changed between the fused list and the re-ranked one (Feature 025,
 * spec FR-006). A pure function of two ranked lists keyed by external identifier — the rule
 * the iOS demo states in `ChangeMark.swift`, ported so both demonstrations mark the same
 * movement the same way.
 *
 * This is the only arithmetic the application performs. Everything else on screen is the
 * engine's own number.
 */
sealed interface ChangeMark {
    /** In the re-ranked list only. */
    data object New : ChangeMark

    /** Same rank in both lists. */
    data object Unchanged : ChangeMark

    /** Rose by [places] positions. */
    data class Up(val places: Int) : ChangeMark

    /** Fell by [places] positions. */
    data class Down(val places: Int) : ChangeMark

    companion object {
        /**
         * Marks for every re-ranked hit, and the identifiers of the fused hits that fell out of
         * the re-ranked head.
         */
        fun compute(fused: List<String>, reranked: List<String>): Pair<Map<String, ChangeMark>, List<String>> {
            val fusedRank = fused.withIndex().associate { (rank, id) -> id to rank }
            val marks = reranked.withIndex().associate { (rank, id) ->
                val before = fusedRank[id]
                id to when {
                    before == null -> New
                    before == rank -> Unchanged
                    before > rank -> Up(before - rank)
                    else -> Down(rank - before)
                }
            }
            val kept = reranked.toSet()
            return marks to fused.filterNot { it in kept }
        }
    }
}
