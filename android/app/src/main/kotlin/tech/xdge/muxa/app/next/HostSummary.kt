package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Corners
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.v2.HostReport
import kotlin.math.roundToInt
import kotlin.math.roundToLong

/** Asks the host for its resources while the calling composable is on screen (ADR 0103). */
@Composable
fun WatchHost(model: NextModel) {
    DisposableEffect(model) {
        model.watchHost()
        onDispose { model.unwatchHost() }
    }
}

/** One value of the summary; the host's `alerts` decide [alerted]. */
data class SummaryValue(val text: String, val spoken: String, val alerted: Boolean)

/** `CPU 12%`, `Mem 96%` and the system disk's free space. */
fun summaryValues(r: HostReport): List<SummaryValue> {
    val cpu = r.cpu.percent.roundToInt()
    val mem = memoryPercent(r)
    val disk = r.systemDisk
    return listOfNotNull(
        SummaryValue("CPU $cpu%", "CPU $cpu percent", "cpu" in r.alerts),
        SummaryValue("Mem $mem%", "memory $mem percent used", "memory" in r.alerts),
        disk?.let { SummaryValue("${shortSize(it.free)} free", "${size(it.free)} free", r.alerted(it)) },
    )
}

fun memoryPercent(r: HostReport): Int =
    if (r.memory.total > 0) (r.memory.used * 100.0 / r.memory.total).roundToInt() else 0

/**
 * The host's CPU, memory and free disk in one line; a tap opens Settings › Host resources.
 * A tablet shows it as a [card] in the sidebar's footer, a phone as a chip by the host menu.
 */
@Composable
fun HostSummary(model: NextModel, open: () -> Unit, modifier: Modifier = Modifier, card: Boolean = false) {
    WatchHost(model)
    val r = model.hostReport ?: return
    val values = summaryValues(r)
    val spoken = "Host resources: " + values.joinToString(", ") { if (it.alerted) "${it.spoken}, warning" else it.spoken }
    val shape = RoundedCornerShape(if (card) 10.dp else 8.dp)
    val size = if (card) 12.5.sp else 11.5.sp
    val frame = if (card) Modifier.fillMaxWidth().height(48.dp) else Modifier.heightIn(min = 32.dp)
    Row(
        modifier
            .testTag("host.summary")
            .then(frame)
            .clip(shape)
            .background(Palette.raised)
            .border(1.dp, Palette.border, shape)
            .clickable(role = Role.Button, onClick = open)
            .clearAndSetSemantics {
                contentDescription = spoken
                role = Role.Button
                onClick { open(); true }
            }
            .padding(horizontal = if (card) 12.dp else 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        values.forEachIndexed { i, v ->
            if (i > 0) Text("·", color = Palette.textTertiary, fontSize = size)
            SummaryText(v, size)
        }
    }
}

@Composable
private fun SummaryText(v: SummaryValue, size: TextUnit) {
    if (!v.alerted) return Text(v.text, color = Palette.textSecondary, fontSize = size, maxLines = 1)
    Text(
        v.text,
        Modifier
            .background(Palette.warnBack, Corners.chip)
            .border(1.dp, Palette.warnBorder, Corners.chip)
            .padding(horizontal = 5.dp, vertical = 1.dp),
        color = Palette.warnText,
        fontSize = size,
        fontWeight = FontWeight.Medium,
        maxLines = 1,
    )
}

private const val KIB = 1024L
private const val MIB = KIB * KIB
private const val GIB = MIB * KIB
private const val TIB = GIB * KIB

/** The summary's terse size: `512M`, `41G`, `1.2T`. */
fun shortSize(bytes: Long): String = when {
    bytes >= TIB -> "${tenths(bytes, TIB)}T"
    bytes >= GIB -> "${(bytes.toDouble() / GIB).roundToLong()}G"
    else -> "${(bytes.toDouble() / MIB).roundToLong()}M"
}

/** A size to read: `420 MB`, `2.4 GB`, `41 GB`, `1.2 TB`. */
fun size(bytes: Long): String = when {
    bytes >= TIB -> "${tenths(bytes, TIB)} TB"
    bytes >= 10 * GIB -> "${(bytes.toDouble() / GIB).roundToLong()} GB"
    bytes >= GIB -> "${tenths(bytes, GIB)} GB"
    else -> "${(bytes.toDouble() / MIB).roundToLong()} MB"
}

private fun tenths(bytes: Long, unit: Long): String = ((bytes.toDouble() / unit) * 10).roundToLong().let { "${it / 10}.${it % 10}" }
