package tech.xdge.muxa.app.pane

import org.junit.Assert.assertEquals
import org.junit.Test
import tech.xdge.muxa.session.Rgb
import tech.xdge.muxa.session.TerminalPalette
import tech.xdge.muxa.terminal.TermColor

class TerminalColorsTest {
    private val ground = Rgb(1, 2, 3)
    private val text = Rgb(200, 201, 202)

    @Test
    fun theThemeSetsTheGroundTextAndFirstSixteen() {
        val ansi = List(16) { Rgb(it, it, it) }
        val colors = TerminalColors(TerminalPalette(ground, text, ansi = ansi))
        assertEquals(TerminalColors.rgb(1, 2, 3), colors.background(TermColor.Default))
        assertEquals(TerminalColors.rgb(200, 201, 202), colors.foreground(TermColor.Default, bold = false))
        assertEquals(TerminalColors.rgb(4, 4, 4), colors.foreground(TermColor.Indexed(4), bold = false))
        // Bold brightens 0–7 into the theme's 8–15.
        assertEquals(TerminalColors.rgb(12, 12, 12), colors.foreground(TermColor.Indexed(4), bold = true))
    }

    @Test
    fun withoutAnsiThePaneKeepsItsOwnSixteenAndTheCubeStays() {
        val colors = TerminalColors(TerminalPalette(ground, text))
        assertEquals(TerminalColors.rgb(205, 49, 49), colors.background(TermColor.Indexed(1)))
        assertEquals(TerminalColors.rgb(95, 135, 175), colors.background(TermColor.Indexed(67)))
        assertEquals(TerminalColors.rgb(8, 8, 8), colors.background(TermColor.Indexed(232)))
    }
}
