package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.v2.HostReport
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.UsageGroup
import tech.xdge.muxa.v2.UsageKind
import java.util.Locale
import kotlin.math.roundToInt

/** Settings' row that opens Host resources; an amber dot while the host reports a value past its limits. */
@Composable
fun HostRow(model: NextModel, open: () -> Unit) {
    WatchHost(model)
    val r = model.hostReport
    SettingsHeader("Host")
    SettingsCard {
        NavRow("Host resources", r?.let { summaryValues(it).joinToString(" · ") { v -> v.text } }, "settings.host", open) {
            if (r != null && r.alerts.isNotEmpty()) {
                Dot(Palette.warnText, 8.dp, Modifier.padding(end = 8.dp).semantics { contentDescription = "warning" })
            }
        }
    }
}

/**
 * Settings › Host resources (ADR 0103): the figures, the Tasks and talks by memory with how
 * long each agent has waited, what else uses the host, and the disk use of Task places.
 */
@Composable
fun HostSettings(model: NextModel, open: (Terminal) -> Unit, back: () -> Unit) {
    WatchHost(model)
    Column(Modifier.testTag("settings.host.page")) {
        Row(Modifier.fillMaxWidth().padding(start = 4.dp, end = 14.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(back, Modifier.testTag("host.back")) { Icon(Icons.back, "Settings") }
            Text("Host resources", fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
        }
        val r = model.hostReport
        when {
            r != null -> Report(model, r, open)
            model.hostError != null -> SettingsWarning("This host cannot report its resources: ${model.hostError}", "host.error")
            else -> CircularProgressIndicator(Modifier.padding(18.dp))
        }
        HostDiskSection(model)
    }
}

@Composable
private fun Report(model: NextModel, r: HostReport, open: (Terminal) -> Unit) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(1_000)
            now = System.currentTimeMillis()
        }
    }
    SettingsNote("${hostName(model)} · sampled ${ago(now - r.sampledMs)}", Modifier.testTag("host.sampled"))
    Tiles(r)
    AgentWork(model, r, open)
    Elsewhere(r)
}

/** `4 s ago`, `3 min ago`; a clock ahead of the host's reads as just now. */
private fun ago(ms: Long): String {
    val s = maxOf(0L, ms / 1_000)
    return if (s < 120) "$s s ago" else "${s / 60} min ago"
}

@Composable
private fun Tiles(r: HostReport) {
    val build = r.disks.firstOrNull { it.name == "build" }
    val system = r.systemDisk
    val compressed = r.memory.compressed?.let { "${size(it)} compressed" }
    val pressure = r.memory.pressure.takeIf { it != "normal" }?.let { "pressure $it" }
    val tiles = listOf(
        Tile("CPU", "${r.cpu.percent.roundToInt()}%", listOfNotNull("${r.cpu.cores} cores", r.load.firstOrNull()?.let { "load ${oneDecimal(it)}" }), "cpu" in r.alerts, "cpu"),
        Tile("Memory", "${size(r.memory.used)} of ${size(r.memory.total)}", listOfNotNull(compressed, pressure), "memory" in r.alerts, "memory"),
        Tile("Swap", size(r.swap.used), listOf("of ${size(r.swap.total)}"), false, "swap"),
        Tile(
            "Disk free", system?.let { size(it.free) } ?: "–",
            listOfNotNull(build?.takeIf { it != system }?.let { "build ${size(it.free)}" }),
            r.disks.any(r::alerted), "disk",
        ),
    )
    BoxWithConstraints(Modifier.padding(horizontal = 12.dp, vertical = 6.dp)) {
        val perRow = if (maxWidth < WIDE_TILES.dp) 2 else 4
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (line in tiles.chunked(perRow)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (t in line) TileView(t, Modifier.weight(1f))
                }
            }
        }
    }
}

private data class Tile(val label: String, val value: String, val lines: List<String>, val alerted: Boolean, val tag: String)

@Composable
private fun TileView(t: Tile, modifier: Modifier) {
    val shape = RoundedCornerShape(12.dp)
    val spoken = listOf(t.label, t.value).plus(t.lines).joinToString(", ") + if (t.alerted) ", warning" else ""
    Column(
        modifier
            .heightIn(min = 84.dp)
            .background(if (t.alerted) Palette.warnBack else Palette.raised, shape)
            .border(1.dp, if (t.alerted) Palette.warnBorder else Palette.border, shape)
            .padding(12.dp)
            .semantics(mergeDescendants = true) { contentDescription = spoken }
            .testTag("host.tile.${t.tag}"),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        val sub = if (t.alerted) Palette.warnText else Palette.textTertiary
        Text(t.label, color = sub, fontSize = 12.sp)
        Text(t.value, color = if (t.alerted) Palette.warnText else Palette.text, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
        for (line in t.lines) Text(line, color = sub, fontSize = 12.sp, maxLines = 1)
    }
}

@Composable
private fun AgentWork(model: NextModel, r: HostReport, open: (Terminal) -> Unit) {
    val work = r.groups.filter { it.isAgentWork }
    var all by remember { mutableStateOf(false) }
    SettingsHeader("Tasks and talks · ${work.size} · ${size(work.sumOf { it.memory })}")
    SettingsCard(Modifier.testTag("host.tasks")) {
        if (work.isEmpty()) SettingsNote("No Task or talk runs now.", Modifier.padding(vertical = 10.dp))
        val shown = if (all) work else work.take(FIRST_TASKS)
        shown.forEachIndexed { i, g ->
            if (i > 0) CardDivider()
            val terminal = model.terminals.firstOrNull { it.id == g.terminal && it.live }
            UsageRow(g, listOfNotNull(g.workspace, size(g.memory), cpu(g), agentState(g, r.sampledMs))) {
                if (terminal != null) LinkButton("Open", { open(terminal) }, Modifier.testTag("host.task.${g.terminal}.open"))
            }
        }
        if (!all && work.size > FIRST_TASKS) {
            CardDivider()
            ActionRow("Show all ${work.size}", "host.showall", true) { all = true }
        }
    }
}

@Composable
private fun Elsewhere(r: HostReport) {
    val rest = r.groups.filterNot { it.isAgentWork }
    if (rest.isEmpty()) return
    SettingsHeader("Elsewhere on the host")
    SettingsCard {
        rest.forEachIndexed { i, g ->
            if (i > 0) CardDivider()
            UsageRow(g, listOfNotNull(processes(g.processes), size(g.memory), cpu(g), hint(g)))
        }
    }
}

@Composable
private fun UsageRow(g: UsageGroup, details: List<String>, trailing: @Composable () -> Unit = {}) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(start = 14.dp, end = 6.dp, top = 8.dp, bottom = 8.dp)
            .semantics(mergeDescendants = true) {}
            .testTag("host.group.${g.terminal ?: g.kind}"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(groupTitle(g), fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1)
            Text(details.joinToString(" · "), color = Palette.textTertiary, fontSize = 12.5.sp)
        }
        trailing()
    }
}

private fun processes(n: Int): String = if (n == 1) "1 process" else "$n processes"

private fun groupTitle(g: UsageGroup): String = when (g.usageKind) {
    UsageKind.CLAUDE_OUTSIDE -> "Claude Code outside muxa"
    else -> g.name
}

private fun hint(g: UsageGroup): String? = if (g.usageKind == UsageKind.SIMULATOR) "Booted by Xcode or a test" else null

private fun cpu(g: UsageGroup): String = "CPU ${oneDecimal(g.cpu)}%"

private fun oneDecimal(x: Double): String = "%.1f".format(Locale.ROOT, x)

/** `working`, or how long the agent has waited since it reported done or idle. */
fun agentState(g: UsageGroup, sampledMs: Long): String {
    val since = g.idleSinceMs ?: return "working"
    val minutes = maxOf(0L, sampledMs - since) / 60_000
    return when {
        minutes >= 24 * 60 -> "idle ${minutes / (24 * 60)} d"
        minutes >= 60 -> "idle ${minutes / 60} h"
        else -> "idle $minutes min"
    }
}

private const val FIRST_TASKS = 5

/** From this width the four tiles stand in one row. */
private const val WIDE_TILES = 560
