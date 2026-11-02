package tech.xdge.muxa.session

import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.terminal.PaneScreen

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

    private suspend fun connected(capabilities: List<String>, viewport: PaneViewport?): Pair<PaneSession, FakeBridge> {
        val bridge = FakeBridge()
        bridge.capabilities = capabilities
        val session = PaneSession(oneThread())
        session.renderer = PaneScreen()
        viewport?.let(session::setViewport)
        session.connect {
            bridge.start()
            bridge
        }
        return session to bridge
    }

    private fun FakeBridge.viewports() = received.filterIsInstance<Request.Viewport>().map { PaneViewport(it.cols, it.rows) }

    @Test
    fun theViewportGoesBeforeTheWatchOnConnect() = runBlocking {
        val (session, bridge) = connected(listOf("viewport"), PaneViewport(120, 40))
        session.watch("%1")
        val kinds = bridge.received.mapNotNull {
            when (it) {
                is Request.Viewport -> "viewport"
                is Request.Watch -> "watch"
                else -> null
            }
        }
        assertEquals(listOf("viewport", "watch"), kinds)
        assertEquals(listOf(PaneViewport(120, 40)), bridge.viewports())
    }

    @Test
    fun aBridgeWithoutTheCapabilityGetsNoViewport() = runBlocking {
        val (session, bridge) = connected(emptyList(), PaneViewport(120, 40))
        session.setViewport(PaneViewport(90, 30))
        session.watch("%1")
        delay(400)
        assertEquals(emptyList<PaneViewport>(), bridge.viewports())
    }

    @Test
    fun changesAreSentOnceTheySettle() = runBlocking {
        val (session, bridge) = connected(listOf("viewport"), PaneViewport(120, 40))
        // A rotation animates through several sizes.
        session.setViewport(PaneViewport(100, 50))
        session.setViewport(PaneViewport(90, 60))
        session.setViewport(PaneViewport(90, 60))
        delay(100)
        assertEquals(listOf(PaneViewport(120, 40)), bridge.viewports())
        delay(500)
        assertEquals(listOf(PaneViewport(120, 40), PaneViewport(90, 60)), bridge.viewports())
    }

    @Test
    fun aReconnectSendsTheViewportAgain() = runBlocking {
        val (session, first) = connected(listOf("viewport"), PaneViewport(120, 40))
        assertEquals(1, first.viewports().size)
        val second = FakeBridge()
        second.capabilities = listOf("viewport")
        session.connect {
            second.start()
            second
        }
        assertEquals(listOf(PaneViewport(120, 40)), second.viewports())
    }
}
