package dev.xtriever.demo

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Feature 025, User Story 2 (spec FR-006): what moved between the fused list and the re-ranked
 * one. A pure function of two ranked lists keyed by external identifier — the rule the iOS
 * demo's `ChangeMark` already states, ported unchanged so the two demonstrations mark the same
 * movement the same way.
 */
class ChangeMarkTests {

    private fun ids(vararg ids: String) = ids.toList()

    @Test
    fun identicalListsAreAllUnchangedAndNothingDropped() {
        val list = ids("a", "b", "c")
        val (marks, dropped) = ChangeMark.compute(fused = list, reranked = list)
        assertEquals(list.associateWith { ChangeMark.Unchanged }, marks)
        assertEquals(emptyList<String>(), dropped)
    }

    @Test
    fun aHitRisingFourPlacesDisplacesTheOthersByOne() {
        val fused = ids("a", "b", "c", "d", "e")
        val reranked = ids("e", "a", "b", "c", "d")
        val (marks, dropped) = ChangeMark.compute(fused, reranked)
        assertEquals(ChangeMark.Up(4), marks["e"])
        listOf("a", "b", "c", "d").forEach { assertEquals("$it fell one place", ChangeMark.Down(1), marks[it]) }
        assertEquals(emptyList<String>(), dropped)
    }

    @Test
    fun aHitOnlyInTheRerankedListIsNewAndOneOnlyInTheFusedListIsDropped() {
        val (marks, dropped) = ChangeMark.compute(fused = ids("a", "b"), reranked = ids("a", "z"))
        assertEquals(ChangeMark.Unchanged, marks["a"])
        assertEquals(ChangeMark.New, marks["z"])
        assertEquals(ids("b"), dropped)
    }

    @Test
    fun emptyListsProduceNothing() {
        val (marks, dropped) = ChangeMark.compute(fused = emptyList(), reranked = emptyList())
        assertEquals(emptyMap<String, ChangeMark>(), marks)
        assertEquals(emptyList<String>(), dropped)
    }
}
