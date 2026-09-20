package dev.xtriever.demo

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Feature 025, User Story 2 (spec FR-007): the title, the passage and the article link, derived
 * from the hit text by the Feature 008 convention — the same rule the Swift package's
 * `titleAndPassage` / `wikipediaURL` and the Python demo's `hits.py` apply, so all three
 * demonstrations link to the same article.
 */
class HitTextTests {

    @Test
    fun theTitleIsTheFirstLineAndThePassageIsWhatFollowsTheBlankLine() {
        val (title, passage) = HitText.split("April\n\nApril is the fourth month of the year.")
        assertEquals("April", title)
        assertEquals("April is the fourth month of the year.", passage)
    }

    @Test
    fun textWithoutABlankLineIsAllPassageAndHasNoTitle() {
        val (title, passage) = HitText.split("a single line with no title")
        assertEquals(null, title)
        assertEquals("a single line with no title", passage)
    }

    @Test
    fun theArticleLinkPercentEncodesEverythingOutsideTheUnreservedSet() {
        assertEquals("https://simple.wikipedia.org/wiki/April", HitText.articleUrl("April"))
        assertEquals("https://simple.wikipedia.org/wiki/New%20York", HitText.articleUrl("New York"))
        assertEquals("https://simple.wikipedia.org/wiki/Saint-Ex", HitText.articleUrl("Saint-Ex"))
        // A slash is unreserved by the 008 convention: article paths keep it.
        assertEquals("https://simple.wikipedia.org/wiki/A/B", HitText.articleUrl("A/B"))
        assertEquals("https://simple.wikipedia.org/wiki/%C3%89mile", HitText.articleUrl("Émile"))
    }

    @Test
    fun aHitWithoutATitleHasNoLink() {
        assertEquals(null, HitText.articleUrl(null))
    }
}
