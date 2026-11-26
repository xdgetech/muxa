package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.AppTheme
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.ThemeStore
import tech.xdge.muxa.session.ResolvedTheme
import tech.xdge.muxa.session.Rgb
import tech.xdge.muxa.session.ThemeFile
import tech.xdge.muxa.session.ThemeRegistry
import tech.xdge.muxa.session.ThemeToken

/**
 * Settings › Appearance › Edit Colors (ADR 0091 Amendment 1): the picked theme's base
 * tokens as `#rrggbb` fields. The edit goes into [ThemeRegistry.editable]'s file, which
 * becomes the pick; Reset removes a token's override so it follows the parent again.
 */
@Composable
fun EditColors(back: () -> Unit) {
    val registry = ThemeStore.registry
    val pickedId = ThemeStore.current.resolved.id
    val original = remember(registry, pickedId) { registry.editable(pickedId) }
    var draft by remember(original) { mutableStateOf(original) }
    // What the user typed, kept while it is not a color yet.
    val typed = remember(original) { mutableStateMapOf<ThemeToken, String>() }
    var error by remember { mutableStateOf<String?>(null) }
    val preview = remember(registry, draft) { AppTheme(preview(registry, draft)) }
    val invalid = typed.any { (_, text) -> Rgb.parse(text.trim()) == null }

    Column(Modifier.testTag("settings.edit")) {
        Row(Modifier.fillMaxWidth().padding(start = 4.dp, end = 8.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(back, Modifier.testTag("edit.back")) { Icon(Icons.back, "Appearance") }
            Column(Modifier.weight(1f)) {
                Text("Edit Colors", fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
                Text(draft.name, color = Palette.textTertiary, fontSize = 12.5.sp)
            }
            TextButton(
                onClick = {
                    error = ThemeStore.save(draft)
                    if (error == null) back()
                },
                modifier = Modifier.testTag("edit.save"),
                enabled = !invalid,
            ) { Text("Save") }
        }
        SettingsCard {
            EDITABLE.forEachIndexed { i, token ->
                if (i > 0) CardDivider()
                TokenRow(
                    token = token,
                    color = preview.resolved[token],
                    text = typed[token],
                    overridden = draft.tokens?.containsKey(token.key) == true,
                    change = { text ->
                        typed[token] = text
                        Rgb.parse(text.trim())?.let { draft = draft.setting(token, it) }
                    },
                    reset = {
                        typed.remove(token)
                        draft = draft.setting(token, null)
                    },
                )
            }
        }
        error?.let { Text(it, Modifier.padding(horizontal = 18.dp, vertical = 4.dp).testTag("edit.error"), color = Palette.destructive, fontSize = 13.sp) }
        SettingsNote("These are the base colors. The rest of the app's colors follow them.")
    }
}

@Composable
private fun TokenRow(token: ThemeToken, color: Rgb, text: String?, overridden: Boolean, change: (String) -> Unit, reset: () -> Unit) {
    val bad = text != null && Rgb.parse(text.trim()) == null
    Row(
        Modifier.fillMaxWidth().padding(start = 14.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            Modifier
                .size(28.dp)
                .background(Color(color.r, color.g, color.b), SWATCH_SHAPE)
                .border(1.dp, Palette.border, SWATCH_SHAPE),
        )
        OutlinedTextField(
            value = text ?: color.hex,
            onValueChange = change,
            modifier = Modifier.weight(1f).testTag("edit.token.${token.key}"),
            label = { Text(token.key) },
            singleLine = true,
            isError = bad,
            supportingText = if (bad) ({ Text("Use #rrggbb") }) else null,
            textStyle = TextStyle(fontFamily = FontFamily.Monospace, fontSize = 14.sp),
        )
        TextButton(onClick = reset, modifier = Modifier.testTag("edit.reset.${token.key}"), enabled = overridden || text != null) {
            Text("Reset")
        }
    }
}

/** The draft resolved as if saved, so a reset shows the parent's color and derived colors follow. */
private fun preview(registry: ThemeRegistry, draft: ThemeFile): ResolvedTheme =
    ThemeRegistry(registry.files.filter { it.id != draft.id } + draft).resolve(draft.id)

/** The base tokens, without the terminal's own two colors, which come from its palette. */
private val EDITABLE = ThemeToken.entries.filter {
    it.isBase && it != ThemeToken.terminalBackground && it != ThemeToken.terminalForeground
}

private val SWATCH_SHAPE = RoundedCornerShape(6.dp)
