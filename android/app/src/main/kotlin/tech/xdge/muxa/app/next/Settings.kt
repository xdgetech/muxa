package tech.xdge.muxa.app.next

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.KindPalette
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.StateColors
import tech.xdge.muxa.v2.UsedBy
import tech.xdge.muxa.v2.Workspace

/** The phone's Settings tab: the host this window is connected to and its Workspaces. */
@Composable
fun SettingsScreen(model: NextModel, nav: Nav) {
    PhoneTabs(nav) {
        LargeTitle("Settings")
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) { SettingsContent(model) }
    }
}

/** A tablet's Settings, opened from the host in the sidebar's footer: the phone tab's content as a sheet. */
@Composable
fun SettingsDialog(model: NextModel, close: () -> Unit) {
    Dialog(close, DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(
            Modifier.widthIn(max = SHEET_WIDTH.dp).heightIn(max = SHEET_HEIGHT.dp).testTag("settings.sheet"),
            shape = MaterialTheme.shapes.large,
            color = Palette.surface,
            border = BorderStroke(1.dp, Palette.border),
        ) {
            Column {
                Row(Modifier.fillMaxWidth().padding(start = 18.dp, end = 8.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("Settings", Modifier.weight(1f), fontSize = 22.sp, fontWeight = FontWeight.Bold)
                    TextButton(close, Modifier.testTag("settings.close")) { Text("Done") }
                }
                Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState())) { SettingsContent(model) }
            }
        }
    }
}

@Composable
private fun SettingsContent(model: NextModel) {
    Column(Modifier.padding(bottom = 16.dp)) {
        HostCard(model)
        Note("Hosts are added and edited while disconnected.")
        WorkspacesSection(model)
    }
}

@Composable
private fun HostCard(model: NextModel) {
    Card {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.host, null, tint = Palette.textSecondary)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(hostName(model), fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                Text(hostLine(model).substringAfter(" · "), color = Palette.textTertiary, fontSize = 12.5.sp)
            }
        }
        HorizontalDivider(color = Palette.divider)
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .clickable(role = Role.Button) { model.disconnect() }
                .padding(horizontal = 14.dp)
                .testTag("settings.disconnect"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(if (model.demo) "Leave the Demo" else "Disconnect", color = Palette.accent)
        }
    }
}

/** Each Workspace as the host defines it, read only (ADR 0069 §7). */
@Composable
private fun WorkspacesSection(model: NextModel) {
    var workspaces by remember { mutableStateOf<List<Workspace>?>(null) }
    var failed by remember { mutableStateOf<String?>(null) }
    // Tasks that start or end change who uses what: ask again when the live terminals change.
    val live = model.terminals.filter { it.live }.map { it.id }
    LaunchedEffect(live) {
        val list = model.workspaces()
        if (list != null) {
            workspaces = list
            failed = null
        } else {
            failed = model.requestError ?: "The Workspaces could not be read."
        }
    }
    Column(Modifier.testTag("settings.workspaces")) {
        Header("Workspaces")
        InfoBanner(
            "Secrets stay in the host's Keychain. muxa never shows them. " +
                "A Default one goes to every Task and Routine of its Workspace. " +
                "This is read only: edit workspaces.toml on the host to change it.",
        )
        val list = workspaces
        when {
            list == null && failed != null -> Note(failed.orEmpty())
            list == null -> Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            list.isEmpty() -> Note("This host has no Workspaces. Add one in workspaces.toml on the host.")
            else -> for (ws in list) WorkspaceCard(ws, model)
        }
    }
}

@Composable
private fun WorkspaceCard(ws: Workspace, model: NextModel) {
    val now = System.currentTimeMillis() / 1000
    Card(Modifier.testTag("settings.workspace.${ws.name}")) {
        Column(Modifier.padding(horizontal = 14.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Column {
                Text(ws.name, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                Text(ws.root, color = Palette.textTertiary, fontSize = 12.5.sp, fontFamily = FontFamily.Monospace)
            }
            Fact("Agent", ws.agent ?: "the host's default")
            Fact("Loops", loopText(ws.loop))
        }
        Group("Connections", "No connections.", ws.connections.isEmpty()) {
            for (c in ws.connections) {
                UsageRow(c.name, null, c.isDefault, c.usedBy, c.lastUsed, now, model, "settings.connection.${ws.name}.${c.name}")
            }
        }
        Group("MCP servers", "No MCP servers: agents get their own.", ws.mcp.isEmpty()) {
            for (m in ws.mcp) {
                UsageRow(m.name, m.name.take(2).uppercase(), m.isDefault, m.usedBy, m.lastUsed, now, model, "settings.mcp.${ws.name}.${m.name}")
            }
            Text(
                "Strict: agents in this Workspace get only these MCP servers.",
                Modifier.padding(horizontal = 14.dp, vertical = 10.dp).testTag("settings.strict.${ws.name}"),
                color = Palette.textTertiary,
                fontSize = 12.5.sp,
            )
        }
    }
}

/** A titled part of a Workspace's card, or [empty] when it has nothing. */
@Composable
private fun Group(title: String, empty: String, isEmpty: Boolean, rows: @Composable () -> Unit) {
    HorizontalDivider(color = Palette.divider)
    Text(
        title.uppercase(),
        Modifier.padding(start = 14.dp, end = 14.dp, top = 10.dp, bottom = 2.dp),
        color = Palette.textTertiary,
        fontSize = 11.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = 0.6.sp,
    )
    if (isEmpty) {
        Text(empty, Modifier.padding(horizontal = 14.dp, vertical = 8.dp), color = Palette.textSecondary, fontSize = 13.sp)
    } else {
        rows()
    }
}

/** A connection or MCP server: Default, who uses it and when it was last used. */
@Composable
private fun UsageRow(
    name: String,
    badge: String?,
    isDefault: Boolean,
    usedBy: UsedBy,
    lastUsed: Long?,
    now: Long,
    model: NextModel,
    tag: String,
) {
    val users = usedBy.named(model.terminals)
    Row(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 8.dp).testTag(tag), verticalAlignment = Alignment.Top) {
        if (badge != null) {
            Box(Modifier.size(30.dp).background(Palette.control, RoundedCornerShape(8.dp)), contentAlignment = Alignment.Center) {
                Text(badge, color = KindPalette.todo.content, fontSize = 11.sp, fontWeight = FontWeight.SemiBold, fontFamily = FontFamily.Monospace)
            }
        } else {
            Dot(if (users.isEmpty) StateColors.idle else StateColors.working, 8.dp, Modifier.padding(top = 6.dp))
        }
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(name, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                if (isDefault) DefaultChip()
            }
            if (users.isEmpty) UsageLine("Not used now")
            if (users.routines.isNotEmpty()) UsageLine("Routines: " + users.routines.joinToString(", "))
            if (users.tasks.isNotEmpty()) UsageLine("Tasks: " + users.tasks.joinToString(", "))
        }
        Text("last used " + lastUsedText(lastUsed, now), color = Palette.textTertiary, fontSize = 12.sp)
    }
}

@Composable
private fun UsageLine(text: String) {
    Text(text, color = Palette.textSecondary, fontSize = 12.5.sp)
}

/** Default: every Task and Routine of the Workspace gets it. */
@Composable
private fun DefaultChip() {
    Text(
        "Default",
        Modifier.background(KindPalette.read.background, RoundedCornerShape(5.dp)).padding(horizontal = 7.dp, vertical = 1.dp),
        color = KindPalette.read.content,
        fontSize = 11.5.sp,
    )
}

@Composable
private fun Fact(label: String, value: String) {
    Row {
        Text(label, Modifier.width(FACT_LABEL_WIDTH.dp), color = Palette.textTertiary, fontSize = 13.sp)
        Text(value, Modifier.weight(1f), fontSize = 13.sp)
    }
}

@Composable
private fun Card(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(
        modifier
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .fillMaxWidth()
            .background(Palette.raised, RoundedCornerShape(14.dp))
            .border(1.dp, Palette.border, RoundedCornerShape(14.dp)),
    ) { content() }
}

@Composable
private fun Header(title: String) {
    Text(
        title.uppercase(),
        Modifier.padding(start = 18.dp, end = 18.dp, top = 20.dp, bottom = 6.dp),
        color = Palette.textTertiary,
        fontSize = 11.sp,
        fontWeight = FontWeight.SemiBold,
        letterSpacing = 0.6.sp,
    )
}

@Composable
private fun InfoBanner(text: String) {
    Text(
        text,
        Modifier
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .fillMaxWidth()
            .background(Palette.infoBackground, RoundedCornerShape(10.dp))
            .border(1.dp, Palette.infoBorder, RoundedCornerShape(10.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .testTag("settings.secrets"),
        color = Palette.infoText,
        fontSize = 13.sp,
        lineHeight = 19.sp,
    )
}

@Composable
private fun Note(text: String) {
    Text(text, Modifier.padding(horizontal = 18.dp, vertical = 4.dp), color = Palette.textTertiary, fontSize = 12.5.sp)
}

private const val SHEET_WIDTH = 640
private const val SHEET_HEIGHT = 760
private const val FACT_LABEL_WIDTH = 64
