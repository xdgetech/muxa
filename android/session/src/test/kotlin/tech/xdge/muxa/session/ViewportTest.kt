package tech.xdge.muxa.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ViewportTest {
    @Test
    fun aPhoneGetsASmallerDefaultFontThanATablet() {
        assertEquals(10f, PaneViewport.fontSize(compact = true))
        assertEquals(13f, PaneViewport.fontSize(compact = false))
        assertTrue(PaneViewport.fontSize(compact = true) >= PaneFit.MINIMUM_FONT_SIZE)
    }

    @Test
    fun theGridIsTheWholeCellsThatFitWithOnePixelOfSlack() {
        // A tablet in landscape at 13 sp, density 2: 15.6 x 31 px cells.
        assertEquals(PaneViewport(164, 51), PaneViewport.fitting(2560f, 1600f, 15.6f, 31f))
        // Exactly cols x cells + 1 fits; one pixel less does not.
        assertEquals(PaneViewport(100, 15), PaneViewport.fitting(801f, 241f, 8f, 16f))
        assertEquals(PaneViewport(99, 14), PaneViewport.fitting(800f, 240f, 8f, 16f))
    }

    @Test
    fun theGridStaysWithinTheProtocolLimits() {
        assertEquals(PaneViewport(10, 5), PaneViewport.fitting(40f, 30f, 6f, 12f))
        assertEquals(PaneViewport(1000, 1000), PaneViewport.fitting(20_000f, 20_000f, 6f, 12f))
        assertNull(PaneViewport.fitting(0f, 0f, 6f, 12f))
    }

    @Test
    fun aPaneWithinTheViewportShowsWhole() {
        val viewport = PaneViewport(120, 40)
        assertTrue(viewport.showsWhole(120, 40))
        assertTrue(viewport.showsWhole(75, 40))
        assertFalse(viewport.showsWhole(100, 60))
        assertFalse(viewport.showsWhole(160, 39))
    }

    @Test
    fun fitKeepsTheFontReadable() {
        assertEquals(PaneFit.WHOLE, PaneFit.automatic(wide = false, wholeFont = 12f, widthFont = 14f))
        assertEquals(PaneFit.WIDTH, PaneFit.automatic(wide = true, wholeFont = 12f, widthFont = 14f))
        assertEquals(PaneFit.WIDTH, PaneFit.automatic(wide = false, wholeFont = 6f, widthFont = 10f))
        assertEquals(PaneFit.READABLE, PaneFit.automatic(wide = false, wholeFont = 4f, widthFont = 6f))
        assertEquals(9f, PaneFit.READABLE.fontSize(4f, 6f))
        assertEquals(PaneFit.WHOLE, PaneFit.pinched(0.5f))
        assertEquals(PaneFit.READABLE, PaneFit.pinched(2f))
        assertNull(PaneFit.pinched(1f))
    }
}
