package tech.xdge.muxa.app.ui

import org.junit.Assert.assertEquals
import org.junit.Test

/** ADR 0013's table: the window and the hinge choose the layout, never the device. */
class LayoutTest {
    @Test
    fun sizeClassesPickPhoneOrTablet() {
        assertEquals(WindowLayout.Phone, windowLayout(411, 891, null))
        // A phone in landscape is compact tall.
        assertEquals(WindowLayout.Phone, windowLayout(891, 411, null))
        assertEquals(WindowLayout.Tablet, windowLayout(1280, 800, null))
        // A tablet in split screen, or a small desktop window, is compact wide.
        assertEquals(WindowLayout.Phone, windowLayout(500, 800, null))
        // Medium width is the tablet layout.
        assertEquals(WindowLayout.Tablet, windowLayout(700, 840, null))
    }

    @Test
    fun aFlatFoldUsesTheWholeWindow() {
        val flat = Hinge(vertical = true, halfOpened = false, separating = false, start = 420, end = 420)
        assertEquals(WindowLayout.Tablet, windowLayout(840, 900, flat))
    }

    @Test
    fun aHalfOpenBookPutsTheListAndThePaneOnEitherSide() {
        val book = Hinge(vertical = true, halfOpened = true, separating = true, start = 420, end = 420)
        assertEquals(WindowLayout.Book(420, 0), windowLayout(840, 900, book))
        // A hinge with a gap separates even when flat.
        val gap = Hinge(vertical = true, halfOpened = false, separating = true, start = 400, end = 430)
        assertEquals(WindowLayout.Book(400, 30), windowLayout(830, 900, gap))
    }

    @Test
    fun tabletopPutsTheAppAboveTheHinge() {
        val table = Hinge(vertical = false, halfOpened = true, separating = true, start = 450, end = 450)
        assertEquals(WindowLayout.Tabletop(450, 0), windowLayout(840, 900, table))
    }
}
