package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.StateColors
import tech.xdge.muxa.v2.Terminal

/** The app's places once connected. Settings is a phone tab; a tablet opens it from the host in the sidebar's footer. */
enum class Section { INBOX, TASKS, BOTS, SETTINGS }

/**
 * Where a screen can send the user: another section, a new Task, or a terminal: a Task's in Focus,
 * a Bot's talk, run or member on the Bot's page (ADR 0086). [bots] is where the Bots place stands.
 */
class Nav(
    val section: Section,
    val go: (Section) -> Unit,
    val newTask: (TaskSeed) -> Unit,
    val open: (Terminal) -> Unit,
    val bots: BotNav,
    /** Settings › Host resources: the phone's Settings tab on that page. */
    val hostResources: () -> Unit = { go(Section.SETTINGS) },
)

private data class Tab(val section: Section, val title: String, val icon: ImageVector, val tag: String)

private val tabs = listOf(
    Tab(Section.INBOX, "Inbox", Icons.inbox, "home.part.Inbox"),
    Tab(Section.TASKS, "Workspaces", Icons.workspaces, "mode.focus"),
    Tab(Section.BOTS, "Bots", Icons.bots, "home.part.Bots"),
    Tab(Section.SETTINGS, "Settings", Icons.settingsTab, "home.settings"),
)

/** A phone's top-level screen: its content above the navigation bar. */
@Composable
fun PhoneTabs(nav: Nav, content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize()) {
        Column(Modifier.weight(1f).fillMaxWidth(), content = content)
        Row(
            Modifier.fillMaxWidth().background(Palette.surface).topBorder().padding(horizontal = 4.dp, vertical = 4.dp),
        ) {
            for (tab in tabs) {
                val on = tab.section == nav.section
                Column(
                    Modifier
                        .weight(1f)
                        .heightIn(min = 56.dp)
                        .clip(RoundedCornerShape(12.dp))
                        .semantics { selected = on }
                        .clickable(role = Role.Tab) { nav.go(tab.section) }
                        .testTag(tab.tag),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    val color = if (on) Palette.text else Palette.textSecondary
                    Icon(tab.icon, null, Modifier.size(22.dp), tint = color)
                    Text(tab.title, color = color, fontSize = 11.sp, fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal)
                }
            }
        }
    }
}

/**
 * A tablet's sidebar: its title and New Task, Inbox | Tasks | Bots, the
 * list, and the host in the footer.
 */
@Composable
fun Sidebar(
    model: NextModel,
    nav: Nav,
    modifier: Modifier = Modifier,
    actions: @Composable RowScope.() -> Unit = {},
    content: @Composable ColumnScope.() -> Unit,
) {
    val parts = listOf(Section.INBOX, Section.TASKS, Section.BOTS)
    val open = model.inbox.items.size
    var settings by remember { mutableStateOf(false) }
    var hostPage by remember { mutableStateOf(false) }
    Column(modifier.width(SIDEBAR_WIDTH.dp).fillMaxHeight().background(Palette.surface).endBorder()) {
        Column(Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(sectionTitle(nav.section), Modifier.weight(1f), fontSize = 22.sp, fontWeight = FontWeight.Bold)
                actions()
                BoxedIconButton(Icons.add, "New Task", { nav.newTask(TaskSeed()) }, Modifier.testTag("home.newtask"))
            }
            Segmented(
                listOf(
                    Segment(if (open > 0) "Inbox $open" else "Inbox", "home.part.Inbox"),
                    Segment("Tasks", "mode.focus"),
                    Segment("Bots", "home.part.Bots"),
                ),
                parts.indexOf(nav.section).coerceAtLeast(0),
                { nav.go(parts[it]) },
                Modifier.fillMaxWidth(),
                track = Palette.ground,
            )
        }
        Column(Modifier.weight(1f).fillMaxWidth(), content = content)
        HorizontalDivider(color = Palette.border)
        HostSummary(model, {
            hostPage = true
            settings = true
        }, Modifier.padding(start = 12.dp, end = 12.dp, top = 10.dp), card = true)
        Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            HostMenu(model) { settings = true }
        }
    }
    if (settings) {
        SettingsDialog(model, nav, hostPage, { hostPage = it }) {
            settings = false
            hostPage = false
        }
    }
}

private fun sectionTitle(s: Section) = when (s) {
    Section.INBOX -> "Inbox"
    Section.TASKS -> "Tasks"
    Section.BOTS -> "Bots"
    Section.SETTINGS -> "Settings"
}

/** The host and its link, which opens Settings and Disconnect. */
@Composable
fun HostMenu(model: NextModel, settings: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier
                .heightIn(min = 48.dp)
                .clip(RoundedCornerShape(8.dp))
                .clickable(role = Role.Button) { menu = true }
                .padding(horizontal = 8.dp)
                .testTag("home.host"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Dot(if (model.isConnected) StateColors.working else StateColors.needsYou, 8.dp)
            Text(hostLine(model), color = Palette.textTertiary, fontSize = 12.5.sp, maxLines = 1)
        }
        DropdownMenu(menu, { menu = false }) {
            DropdownMenuItem({ Text("Settings") }, {
                menu = false
                settings()
            }, leadingIcon = { Icon(Icons.settings, null) }, modifier = Modifier.testTag("home.host.settings"))
            DropdownMenuItem({ Text("Disconnect") }, {
                menu = false
                model.disconnect()
            }, leadingIcon = { Icon(Icons.disconnect, null) }, modifier = Modifier.testTag("home.disconnect"))
        }
    }
}

fun hostName(model: NextModel): String = if (model.demo) "Demo" else model.currentHost?.title ?: "Host"

fun hostLine(model: NextModel): String = hostName(model) + " · " + when (model.link) {
    NextModel.Link.Connected -> "connected"
    is NextModel.Link.Reconnecting -> "reconnecting"
    else -> "offline"
}

/**
 * Agents waiting on the user (ADR 0066 decision 3), as the amber banner on top
 * of the Inbox. One waiting Task opens it; several open the Tasks.
 */
@Composable
fun NeedsYouBanner(model: NextModel, open: (Terminal) -> Unit, showTasks: () -> Unit) {
    val waiting = model.needsYou
    if (waiting.isEmpty()) return
    val one = waiting.singleOrNull()
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 48.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(StateColors.needsYouBackground)
            .border(1.dp, StateColors.needsYouBorder, RoundedCornerShape(12.dp))
            .clickable(role = Role.Button) { if (one != null) open(one) else showTasks() }
            .padding(horizontal = 12.dp)
            .testTag(if (one != null) "needsyou.${one.id}" else "needsyou"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Dot(StateColors.needsYou)
        Text(if (one != null) "${one.title} needs you" else "${waiting.size} agents need you", Modifier.weight(1f), fontSize = 14.sp)
        Icon(Icons.chevron, null, Modifier.size(18.dp), tint = Palette.textSecondary)
    }
}

/** A 1 dp line along the top edge. */
fun Modifier.topBorder(): Modifier = drawBehind {
    drawLine(Palette.border, Offset(0f, 0f), Offset(size.width, 0f), 1.dp.toPx())
}

/** A 1 dp line along the end edge, as the sidebar's. */
fun Modifier.endBorder(): Modifier = drawBehind {
    drawLine(Palette.border, Offset(size.width, 0f), Offset(size.width, size.height), 1.dp.toPx())
}

/** A 1 dp line along the bottom edge. */
fun Modifier.bottomBorder(): Modifier = drawBehind {
    drawLine(Palette.border, Offset(0f, size.height), Offset(size.width, size.height), 1.dp.toPx())
}

private const val SIDEBAR_WIDTH = 360
