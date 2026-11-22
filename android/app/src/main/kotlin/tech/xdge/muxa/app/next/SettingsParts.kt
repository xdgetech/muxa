package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Corners
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.KindColors
import tech.xdge.muxa.app.ui.KindPalette
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.StateColors
import tech.xdge.muxa.v2.SecretState
import tech.xdge.muxa.v2.UsedBy

// The small pieces every part of Settings is drawn with, in the design's dark palette.

@Composable
fun SettingsCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .fillMaxWidth()
            .background(Palette.raised, RoundedCornerShape(14.dp))
            .border(1.dp, Palette.border, RoundedCornerShape(14.dp)),
        content = content,
    )
}

@Composable
fun SettingsHeader(title: String) {
    Text(
        title.uppercase(),
        Modifier.padding(start = 18.dp, end = 18.dp, top = 20.dp, bottom = 6.dp),
        color = Palette.textTertiary,
        fontSize = 11.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = 0.6.sp,
    )
}

/** A blue note that explains, such as where secrets stay. */
@Composable
fun SettingsInfo(text: String, tag: String? = null) {
    Text(
        text,
        Modifier
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .fillMaxWidth()
            .background(Palette.infoBackground, RoundedCornerShape(10.dp))
            .border(1.dp, Palette.infoBorder, RoundedCornerShape(10.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .then(if (tag != null) Modifier.testTag(tag) else Modifier),
        color = Palette.infoText,
        fontSize = 13.sp,
        lineHeight = 19.sp,
    )
}

/** An amber note about what the host did: it changed the settings, or locked them. */
@Composable
fun SettingsWarning(text: String, tag: String) {
    Text(
        text,
        Modifier
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .fillMaxWidth()
            .background(StateColors.needsYouBackground, RoundedCornerShape(10.dp))
            .border(1.dp, StateColors.needsYouBorder, RoundedCornerShape(10.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .testTag(tag),
        color = KindPalette.reply.content,
        fontSize = 13.sp,
        lineHeight = 19.sp,
    )
}

@Composable
fun SettingsNote(text: String, modifier: Modifier = Modifier) {
    Text(text, modifier.padding(horizontal = 18.dp, vertical = 4.dp), color = Palette.textTertiary, fontSize = 12.5.sp)
}

/** A titled part of a card. */
@Composable
fun CardTitle(title: String) {
    Text(
        title.uppercase(),
        Modifier.padding(start = 14.dp, end = 14.dp, top = 10.dp, bottom = 2.dp),
        color = Palette.textTertiary,
        fontSize = 11.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = 0.6.sp,
    )
}

@Composable
fun CardDivider() = HorizontalDivider(color = Palette.divider)

/** A card's row that opens or adds something: the design's "+ Add connection". */
@Composable
fun ActionRow(title: String, tag: String, enabled: Boolean, color: androidx.compose.ui.graphics.Color = Palette.accent, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .alpha(if (enabled) 1f else DISABLED)
            .padding(horizontal = 14.dp)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) { Text(title, color = color, fontSize = 14.sp) }
}

/** A row with a title, a detail line and a chevron, that opens an editor. */
@Composable
fun NavRow(title: String, detail: String?, tag: String, onClick: () -> Unit, trailing: @Composable () -> Unit = {}) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 52.dp).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 14.dp, vertical = 8.dp)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, fontSize = 15.sp, fontWeight = FontWeight.Medium)
            if (detail != null) Text(detail, color = Palette.textTertiary, fontSize = 12.5.sp)
        }
        trailing()
        Icon(Icons.chevron, null, tint = Palette.textTertiary)
    }
}

@Composable
fun Chip(text: String, colors: KindColors, modifier: Modifier = Modifier) {
    Text(
        text,
        modifier.background(colors.background, Corners.chip).padding(horizontal = 7.dp, vertical = 1.dp),
        color = colors.content,
        fontSize = 11.5.sp,
    )
}

/** Read in blue, write in amber, as the design. */
@Composable
fun AccessChip(access: String?) {
    when (access) {
        null -> {}
        "write" -> Chip("write", KindPalette.reply)
        else -> Chip(access, KindPalette.read)
    }
}

/** Whether a write-only value is there; never the value. */
@Composable
fun SecretChip(state: SecretState, modifier: Modifier = Modifier) {
    val colors = when (state) {
        SecretState.SET -> KindPalette.review
        SecretState.MISSING -> KindPalette.reply
        SecretState.LOCKED, SecretState.UNAVAILABLE -> KindPalette.failed
        SecretState.UNKNOWN -> KindPalette.todo
    }
    Chip(state.wire, colors, modifier)
}

/** Who uses a connection or MCP server now, and when it was last used. */
@Composable
fun UsageLines(usedBy: UsedBy?, lastUsed: Long?, model: NextModel) {
    val users = usedBy?.named(model.terminals)
    if (users == null || users.isEmpty) UsageLine("Not used now")
    if (users != null && users.routines.isNotEmpty()) UsageLine("Routines: " + users.routines.joinToString(", "))
    if (users != null && users.tasks.isNotEmpty()) UsageLine("Tasks: " + users.tasks.joinToString(", "))
    UsageLine("last used " + lastUsedText(lastUsed, System.currentTimeMillis() / 1000))
}

@Composable
private fun UsageLine(text: String) {
    Text(text, color = Palette.textSecondary, fontSize = 12.5.sp)
}

/** An item of a card: a badge or dot, a title line with chips, detail lines. */
@Composable
fun EntryRow(tag: String, enabled: Boolean, onClick: () -> Unit, badge: String?, lead: @Composable () -> Unit = {}, body: @Composable ColumnScope.() -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(enabled = enabled, role = Role.Button, onClick = onClick).padding(horizontal = 14.dp, vertical = 10.dp)
            .testTag(tag),
        verticalAlignment = Alignment.Top,
    ) {
        if (badge != null) {
            Box(Modifier.size(30.dp).background(Palette.control, RoundedCornerShape(8.dp)), contentAlignment = Alignment.Center) {
                Text(badge, color = KindPalette.todo.content, fontSize = 11.sp, fontWeight = FontWeight.SemiBold, fontFamily = FontFamily.Monospace)
            }
            Spacer(Modifier.width(10.dp))
        }
        lead()
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp), content = body)
    }
}

@Composable
fun EntryTitle(text: String, chips: @Composable () -> Unit = {}) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(text, fontSize = 14.sp, fontWeight = FontWeight.Medium)
        chips()
    }
}

@Composable
fun EntryDetail(text: String, mono: Boolean = false) {
    Text(text, color = Palette.textTertiary, fontSize = 12.5.sp, fontFamily = if (mono) FontFamily.Monospace else null)
}

private const val DISABLED = 0.4f
