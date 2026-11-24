package tech.xdge.muxa.app.next

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.AppTheme
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.ThemeChoice
import tech.xdge.muxa.app.ui.ThemeStore
import tech.xdge.muxa.session.ThemePick
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
 * theme shows the colors it gives the app. Automatic, import and editing colors
 * follow Amendment 1.
 */
@Composable
fun AppearanceSettings(back: () -> Unit) {
    var editing by remember { mutableStateOf(false) }
    if (editing) {
        EditColors { editing = false }
        return
    }
    val registry = ThemeStore.registry
    val themes = remember(registry) { registry.files.map { AppTheme(registry.resolve(it.id)) } }
    val automatic = remember(registry) {
        listOf(ThemeChoice.AUTOMATIC.light, ThemeChoice.AUTOMATIC.dark).map { AppTheme(registry.resolve(it)) }
    }
    val pick = ThemeStore.pick
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var importError by remember { mutableStateOf<String?>(null) }
    val importer = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) scope.launch { importError = ThemeStore.import(context, uri) }
    }
    Column(Modifier.testTag("settings.appearance")) {
        Row(Modifier.fillMaxWidth().padding(start = 4.dp, end = 14.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(back, Modifier.testTag("appearance.back")) { Icon(Icons.back, "Settings") }
            Text("Appearance", fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
        }
        SettingsHeader("Theme")
        SettingsCard {
            ThemeRow("Automatic", automatic, "terminal.theme.automatic", pick is ThemePick.System) {
                ThemeStore.choose(ThemeChoice.AUTOMATIC)
            }
            themes.forEach { theme ->
                CardDivider()
                val id = theme.resolved.id
                // A saved id that is gone resolves as the default: mark what the app shows.
                val picked = pick is ThemePick.Fixed && ThemeStore.current.resolved.id == id
                val remove = if (id in registry.bundledIds) null else ({ ThemeStore.remove(id) })
                ThemeRow(theme.resolved.name, listOf(theme), "terminal.theme.$id", picked, remove) { ThemeStore.choose(id) }
            }
        }
        SettingsCard {
            ActionRow("Import Theme…", "appearance.import", enabled = true) {
                importError = null
                importer.launch(arrayOf("*/*"))
            }
            CardDivider()
            ActionRow("Edit Colors…", "appearance.edit", enabled = true) { editing = true }
        }
        importError?.let {
            Text(
                it,
                Modifier.padding(horizontal = 18.dp, vertical = 4.dp).testTag("appearance.import.error"),
                color = Palette.destructive,
                fontSize = 13.sp,
            )
        }
        SettingsNote(
            "The theme colors the whole app and every terminal on this device. Import a muxa theme file, " +
                "an iTerm2 .itermcolors preset or a Ghostty theme.",
        )
    }
}

@Composable
private fun ThemeRow(
    name: String,
    swatches: List<AppTheme>,
    tag: String,
    selected: Boolean,
    remove: (() -> Unit)? = null,
    pick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .selectable(selected, role = Role.RadioButton, onClick = pick)
            .padding(start = 14.dp, end = 14.dp, top = 8.dp, bottom = 8.dp)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(name, fontSize = 15.sp, fontWeight = FontWeight.Medium)
            Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                swatches.forEach { Swatch(it) }
            }
        }
        if (remove != null) {
            IconButton(remove, Modifier.testTag("$tag.remove")) {
                Icon(Icons.delete, "Remove $name", tint = Palette.textTertiary)
            }
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
