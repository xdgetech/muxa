package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.StateColors
import tech.xdge.muxa.v2.BotWork
import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.Routine
import tech.xdge.muxa.v2.Terminal

/**
 * A Bot's page (ADR 0086): its header, Home / Chats / Routines / Files / Settings, and the conversation
 * or item open on it, which takes the page's place until Back.
 */
@Composable
fun BotPage(model: NextModel, nav: Nav, editor: SettingsEditor, e: BotEntry) {
    val bots = nav.bots
    val phone = LocalPhoneLayout.current
    val scope = rememberCoroutineScope()
    val actions = remember(model) { ItemActionState(model, scope) }
    val terminal = model.terminals.firstOrNull { it.id == bots.terminal && it.live }
    val item = bots.item?.let { id -> (model.inbox.items + model.inbox.done).firstOrNull { it.id == id } }
    when {
        terminal != null -> BotConversation(model, e.pick, terminal) { bots.terminal = null }
        item != null -> Column(Modifier.fillMaxSize()) {
            BackRow(e.name, "bot.item.back") { bots.item = null }
            val position = model.inbox.items.indexOfFirst { it.id == item.id }.takeIf { it >= 0 }?.plus(1)
            InboxItemPane(model, item, position, actions, nav.open, nav.newTask) { bots.item = null }
        }
        else -> Column(Modifier.fillMaxSize().testTag("bot.page")) {
            if (phone) BackRow("Bots", "bots.back") { bots.back() }
            Column(
                Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(top = if (phone) 0.dp else 18.dp, bottom = 8.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                BotHeader(e)
                TabChips(bots.tab) { bots.selectTab(it) }
            }
            when (bots.tab) {
                BotTab.HOME -> PageColumn {
                    TalkField(model, e, bots)
                    BotHome(e, bots)
                }
                BotTab.CHATS -> BotChats(model, e, bots)
                BotTab.ROUTINES -> PageColumn { BotRoutines(model, editor, e) }
                BotTab.FILES -> PageColumn { BotFiles(editor, e) }
                BotTab.SETTINGS -> PageColumn { BotSettingsTab(model, editor, e, bots) }
            }
        }
    }
}

/** A scrolling column for a tab's content. */
@Composable
fun PageColumn(content: @Composable ColumnScope.() -> Unit) {
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp).widthIn(max = 760.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        content = content,
    )
}

/** Back, with where it leads. */
@Composable
fun BackRow(title: String, tag: String, back: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 4.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        IconButton(back, Modifier.testTag(tag)) { Icon(Icons.backChevron, "Back", tint = Palette.accent) }
        Text(title, color = Palette.accent, fontSize = 15.sp)
    }
}

@Composable
private fun BotHeader(e: BotEntry) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        BotGlyph(e.name, e.bot.enabled, 48)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(e.name, fontSize = 22.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.testTag("bot.title"))
                Text(
                    e.work.status.wire.replace('-', ' '), Modifier.testTag("bot.status"),
                    color = statusColor(e.work.status), fontSize = 12.5.sp,
                )
            }
            Text(e.workspace.name, color = Palette.textTertiary, fontSize = 13.sp)
            e.bot.description?.let { Text(it, fontSize = 14.sp, maxLines = 2, overflow = TextOverflow.Ellipsis) }
        }
    }
}

@Composable
private fun TabChips(picked: BotTab, pick: (BotTab) -> Unit) {
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (t in BotTab.entries) {
            FilterChip(
                t == picked, { pick(t) }, { Text(t.title) }, Modifier.testTag("bot.tab.${t.name.lowercase()}"),
                colors = FilterChipDefaults.filterChipColors(selectedContainerColor = Palette.picked, selectedLabelColor = Palette.text),
            )
        }
    }
}

/** Home: Needs you, In progress, Planned and Recent, by the rules of "A Bot's work". */
@Composable
private fun ColumnScope.BotHome(e: BotEntry, bots: BotNav) {
    val w = e.work
    fun openTerminal(t: Terminal) {
        bots.terminal = t.id
        bots.selectTab(BotTab.CHATS)
    }
    HomeSection("Needs you", w.needsYou.size, "bot.home.needsyou", amber = w.needsYou.isNotEmpty())
    if (w.needsYou.isEmpty()) Quiet("Nothing waits for you.")
    for (entry in w.needsYou) {
        when (entry) {
            is BotWork.Entry.Item -> ItemCard(entry.item) { bots.item = entry.item.id }
            is BotWork.Entry.Waiting -> TerminalCard(entry.terminal, e.name, waiting = true) { openTerminal(entry.terminal) }
        }
    }
    HomeSection("In progress", w.inProgress.size, "bot.home.inprogress")
    if (w.inProgress.isEmpty()) Quiet("Nothing runs now.")
    for (t in w.inProgress) TerminalCard(t, e.name, waiting = false) { openTerminal(t) }
    HomeSection("Planned", w.planned.size, "bot.home.planned")
    if (w.planned.isEmpty()) Quiet("No Routine is planned. Ask for one in a chat.")
    for (r in w.planned) PlannedCard(r)
    HomeSection("Recent", w.recent.size, "bot.home.recent")
    if (w.recent.isEmpty()) Quiet("No done items yet.")
    for (i in w.recent.take(RECENT)) ItemCard(i) { bots.item = i.id }
}

@Composable
private fun HomeSection(title: String, count: Int, tag: String, amber: Boolean = false) {
    Text(
        if (count > 0) "$title · $count" else title,
        Modifier.padding(start = 6.dp, top = 10.dp).testTag(tag),
        color = if (amber) StateColors.needsYou else Palette.textSecondary, fontSize = 14.sp,
    )
}

@Composable
fun Quiet(text: String) {
    Text(text, Modifier.padding(horizontal = 6.dp), color = Palette.textTertiary, fontSize = 13.sp)
}

@Composable
fun PageCard(tag: String, onClick: (() -> Unit)?, content: @Composable RowScope.() -> Unit) {
    val click = if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier
    Row(
        Modifier.fillMaxWidth().clip(CARD).background(Palette.raised).then(click).padding(horizontal = 14.dp, vertical = 12.dp).testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) { content() }
}

@Composable
private fun ItemCard(i: InboxItem, open: () -> Unit) {
    PageCard("bot.item.${i.id}", open) {
        KindChip(i.kind, Modifier.width(56.dp))
        Text(i.title, Modifier.weight(1f), fontSize = 15.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
        i.due?.let { DueLabel(it) }
    }
}

@Composable
private fun TerminalCard(t: Terminal, bot: String, waiting: Boolean, open: () -> Unit) {
    PageCard("bot.terminal.${t.id}", open) {
        Dot(if (waiting) StateColors.needsYou else t.state.color)
        Column(Modifier.weight(1f)) {
            Text(t.title, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(if (waiting) "Its menu waits for you" else t.botRole(bot), fontSize = 13.sp, color = Palette.textTertiary)
        }
        if (!waiting && t.state != TaskState.UNKNOWN) Text(t.state.title.lowercase(), fontSize = 12.5.sp, color = t.state.color)
    }
}

@Composable
private fun PlannedCard(r: Routine) {
    PageCard("bot.planned.${r.name}", null) {
        Column(Modifier.weight(1f)) {
            Text(r.name, fontSize = 15.sp)
            Text(r.whenText, fontSize = 13.sp, color = Palette.textTertiary)
        }
    }
}

private val CARD = RoundedCornerShape(16.dp)
private const val RECENT = 10
