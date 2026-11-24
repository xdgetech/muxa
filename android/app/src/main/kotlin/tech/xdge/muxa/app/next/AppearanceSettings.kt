package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.AppTheme
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.ThemeStore
import tech.xdge.muxa.session.ThemeToken

/** Settings' row that opens Appearance, with the picked theme's name. */
@Composable
fun AppearanceRow(open: () -> Unit) {
    SettingsHeader("This device")
    SettingsCard {
        NavRow("Appearance", ThemeStore.current.resolved.name, "settings.terminal", open)
    }
}

/**
 * Settings › Appearance (ADR 0091): the app's one theme, kept on this device. Each
 * theme shows the colors it gives the app. Import comes later on Android (T4).
 */
@Composable
fun AppearanceSettings(back: () -> Unit) {
    val registry = ThemeStore.registry
    val themes = remember(registry) { registry.files.map { AppTheme(registry.resolve(it.id)) } }
    Column(Modifier.testTag("settings.appearance")) {
        Row(Modifier.fillMaxWidth().padding(start = 4.dp, end = 14.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(back, Modifier.testTag("appearance.back")) { Icon(Icons.back, "Settings") }
            Text("Appearance", fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
        }
        SettingsHeader("Theme")
        SettingsCard {
            themes.forEachIndexed { i, theme ->
                if (i > 0) CardDivider()
                val id = theme.resolved.id
                // A saved id that is gone resolves as the default: mark what the app shows.
                ThemeRow(theme, ThemeStore.current.resolved.id == id) { ThemeStore.choose(id) }
            }
        }
        SettingsNote("The theme colors the whole app and every terminal on this device.")
    }
}

@Composable
private fun ThemeRow(theme: AppTheme, selected: Boolean, pick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .selectable(selected, role = Role.RadioButton, onClick = pick)
            .padding(horizontal = 14.dp, vertical = 8.dp)
            .testTag("terminal.theme.${theme.resolved.id}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(theme.resolved.name, fontSize = 15.sp, fontWeight = FontWeight.Medium)
            Swatch(theme, Modifier.padding(top = 6.dp))
        }
        Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) {
            if (selected) Icon(Icons.check, "Picked", tint = Palette.accent)
        }
    }
}

/** The theme's ground with its text, accent and state colors on it. */
@Composable
private fun Swatch(theme: AppTheme, modifier: Modifier = Modifier) {
    Row(
        modifier
            .background(theme[ThemeToken.ground], SWATCH_SHAPE)
            .border(1.dp, theme[ThemeToken.border], SWATCH_SHAPE)
            .padding(horizontal = 8.dp, vertical = 5.dp),
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        for (token in SWATCH) Dot(theme[token])
    }
}

@Composable
private fun Dot(color: Color) = Box(Modifier.size(12.dp).background(color, CircleShape))

private val SWATCH_SHAPE = RoundedCornerShape(6.dp)
private val SWATCH = listOf(
    ThemeToken.text, ThemeToken.accent, ThemeToken.working, ThemeToken.needsYou, ThemeToken.failure, ThemeToken.decide,
)
