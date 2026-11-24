package tech.xdge.muxa.app.next

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.StateColors
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.BotWork
import tech.xdge.muxa.v2.Routine

/**
 * Bots (ADR 0086): a main place in place of Routines. Bots by Workspace, then the Routines without a
 * Bot; a phone shows the list or one Bot's page, a tablet both. It reads the host's settings itself,
 * and again when the Inbox changes, since an applied proposal changes them.
 */
@Composable
fun BotsScreen(model: NextModel, nav: Nav) {
    val editor = remember(model, model.hostId) { SettingsEditor(model.settingsHost) }
    LaunchedEffect(editor, model.inbox.revision) { editor.load() }
    LaunchedEffect(Unit) { model.refreshRoutines() }
    var creating by remember { mutableStateOf(false) }
    val entries = botEntries(editor.settings, model.inbox, model.terminals, model.routines)
    val bots = nav.bots
    val entry = entries.firstOrNull { it.pick == bots.pick }
    val routine = model.routines.firstOrNull { it.id == bots.routine }
    // A talk that sets up a Bot not added yet (ADR 0083) has no page; it shows on its own.
    val loneTalk = model.terminals.firstOrNull { it.id == bots.terminal && it.live }?.takeIf { entry == null }
    val list: @Composable () -> Unit = { BotList(model, editor, entries, bots) { creating = true } }
    val detail: @Composable (Modifier) -> Unit = { m ->
        Box(m) {
            when {
                entry != null -> BotPage(model, nav, editor, entry)
                loneTalk != null && bots.pick != null -> BotConversation(model, bots.pick ?: return@Box, loneTalk) { bots.back() }
                routine != null -> RoutinePane(model, routine)
                // A Bot just added shows once the settings are read again.
                bots.pick != null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
                else -> Empty("Select a Bot")
            }
        }
    }
    if (LocalPhoneLayout.current) {
        if (bots.pick == null && routine == null) {
            Box(Modifier.fillMaxSize()) {
                PhoneTabs(nav) {
                    LargeTitle("Bots")
                    Box(Modifier.weight(1f)) { list() }
                }
                FloatingActionButton(
                    { creating = true },
                    Modifier.align(Alignment.BottomEnd).padding(end = 18.dp, bottom = 92.dp).testTag("bots.new"),
                    containerColor = Palette.picked,
                    contentColor = Palette.accent,
                ) { Icon(Icons.add, "New Bot") }
            }
        } else {
            BackHandler { bots.back() }
            Column(Modifier.fillMaxSize()) {
                if (routine != null || (entry == null && loneTalk == null)) {
                    IconButton({ bots.back() }, Modifier.padding(start = 4.dp).testTag("bots.back")) {
                        Icon(Icons.backChevron, "Bots", tint = Palette.accent)
                    }
                }
                detail(Modifier.weight(1f).fillMaxWidth())
            }
        }
    } else {
        Row(Modifier.fillMaxSize()) {
            Sidebar(model, nav, actions = {
                BoxedIconButton(Icons.bots, "New Bot", { creating = true }, Modifier.testTag("bots.new"))
            }) { Box(Modifier.weight(1f)) { list() } }
            detail(Modifier.weight(1f).fillMaxHeight())
        }
    }
    if (creating) NewBotSheet(model, nav, editor) { creating = false }
}

/** The Bots by Workspace, then the Routines without a Bot, failed or held ones first. */
@Composable
private fun BotList(model: NextModel, editor: SettingsEditor, entries: List<BotEntry>, bots: BotNav, create: () -> Unit) {
    val phone = LocalPhoneLayout.current
    val loose = routinesWithoutBot(model.routines)
    LazyColumn(
        Modifier.fillMaxSize().testTag("bots.list"),
        contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 4.dp, bottom = if (phone) 96.dp else 16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        when {
            editor.settings == null && editor.loadError != null -> item { SettingsNote(editor.loadError.orEmpty()) }
            editor.settings == null -> item {
                Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            }
            entries.isEmpty() -> item { NoBots(create) }
        }
        for ((workspace, group) in entries.groupBy { it.workspace.name }) {
            item(key = "ws-$workspace") { GroupTitle(workspace, "bots.workspace.$workspace") }
            items(group, key = { "bot-${it.workspace.name}-${it.name}" }) { e ->
                BotCard(e, e.pick == bots.pick) { bots.pickBot(e) }
            }
        }
        if (loose.isNotEmpty()) {
            item(key = "loose") { GroupTitle("Routines without a Bot", "bots.routines") }
            items(loose, key = { "routine-${it.id}" }) { r -> LooseRoutine(r, r.id == bots.routine) { bots.routine = r.id; bots.pick = null } }
        }
        if (model.routineProblems.isNotEmpty()) {
            item { GroupTitle("Cannot read", "bots.problems") }
            items(model.routineProblems, key = { "problem-" + it.workspace }) { p ->
                Column(Modifier.padding(horizontal = 6.dp, vertical = 4.dp)) {
                    Text(p.workspace)
                    Text(p.reason, color = Theme.failure, style = MaterialTheme.typography.labelMedium)
                }
            }
        }
    }
}

@Composable
private fun GroupTitle(title: String, tag: String) {
    Text(
        title, Modifier.padding(start = 6.dp, top = 10.dp, bottom = 2.dp).testTag(tag),
        style = MaterialTheme.typography.titleSmall, color = Palette.textSecondary,
    )
}

@Composable
private fun NoBots(create: () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text("No Bots yet", fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
        Text(
            "A Bot watches something for you and adds what needs you to the Inbox. It starts with a chat, " +
                "and proposes the Routines you apply.",
            color = Palette.textSecondary,
        )
        PrimaryButton("New Bot", create, Modifier.testTag("bots.empty.new"))
    }
}

/** A Bot as a card: its glyph, name and state, and how much needs you. */
@Composable
private fun BotCard(e: BotEntry, picked: Boolean, onClick: () -> Unit) {
    val count = e.work.needsYou.size
    Row(
        Modifier
            .fillMaxWidth()
            .clip(CARD)
            .background(if (picked) Palette.picked else Palette.raised)
            .semantics { selected = picked }
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 12.dp)
            .testTag("bot.${e.workspace.name}.${e.name}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        BotGlyph(e.name, e.bot.enabled, 40)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(e.name, fontSize = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                statusLine(e), fontSize = 13.sp, color = Palette.textTertiary, maxLines = 1, overflow = TextOverflow.Ellipsis,
            )
        }
        if (count > 0) {
            Text(
                "$count", Modifier.background(StateColors.needsYouBackground, RoundedCornerShape(50)).padding(horizontal = 9.dp, vertical = 2.dp)
                    .testTag("bot.${e.workspace.name}.${e.name}.count"),
                color = StateColors.needsYou, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
            )
        }
    }
}

/** "Needs you", "Running", "Paused", or its description. */
fun statusLine(e: BotEntry): String = when (e.work.status) {
    BotWork.Status.NEEDS_YOU -> "Needs you"
    BotWork.Status.RUNNING -> "Running"
    BotWork.Status.PAUSED -> "Paused"
    BotWork.Status.IDLE -> e.bot.description ?: "Idle"
}

/** The Bot's state as a color: amber when it needs you, green when it runs. */
fun statusColor(s: BotWork.Status): Color = when (s) {
    BotWork.Status.NEEDS_YOU -> StateColors.needsYou
    BotWork.Status.RUNNING -> StateColors.working
    BotWork.Status.PAUSED -> Palette.textTertiary
    BotWork.Status.IDLE -> StateColors.idle
}

/** A Bot's first letter in a rounded square; dimmed while it is turned off. */
@Composable
fun BotGlyph(name: String, enabled: Boolean, size: Int) {
    Box(
        Modifier.size(size.dp).clip(RoundedCornerShape((size * 0.3f).dp)).background(if (enabled) Palette.picked else Palette.control),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            name.take(1).uppercase(), fontSize = (size * 0.42f).sp, fontWeight = FontWeight.SemiBold,
            color = if (enabled) Palette.accent else Palette.textTertiary,
        )
    }
}

@Composable
private fun LooseRoutine(r: Routine, picked: Boolean, onClick: () -> Unit) {
    RoutineRow(r, Modifier.clip(CARD).background(if (picked) Palette.picked else Palette.raised).semantics { selected = picked }.clickable(onClick = onClick))
}

private val CARD = RoundedCornerShape(16.dp)
