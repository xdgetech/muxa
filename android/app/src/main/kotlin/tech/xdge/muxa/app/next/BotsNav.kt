package tech.xdge.muxa.app.next

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import tech.xdge.muxa.v2.BotWork
import tech.xdge.muxa.v2.Inbox
import tech.xdge.muxa.v2.Routine
import tech.xdge.muxa.v2.Settings
import tech.xdge.muxa.v2.SettingsBot
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsWorkspace
import tech.xdge.muxa.v2.Terminal

/** A Bot by its Workspace and name: a Bot's name is unique only in its Workspace. */
data class BotPick(val workspace: String, val name: String)

/** The parts of a Bot's page (ADR 0086). */
enum class BotTab(val title: String) { HOME("Home"), CHATS("Chats"), ROUTINES("Routines"), FILES("Files"), SETTINGS("Settings") }

/**
 * Where the Bots place stands: the picked Bot or Routine without a Bot, the tab of the Bot's page, and
 * the conversation or item open on it. Kept above the screens, so a terminal opened from elsewhere
 * (Needs you, Ask a Bot, New Bot) lands on its Bot's page.
 */
@Stable
class BotNav {
    var pick: BotPick? by mutableStateOf(null)
    var tab by mutableStateOf(BotTab.HOME)

    /** The talk, run or member open on the Bot's page. */
    var terminal: String? by mutableStateOf(null)

    /** The item open on the Bot's page. */
    var item: String? by mutableStateOf(null)

    /** A Routine without a Bot, by its id. */
    var routine: String? by mutableStateOf(null)

    /** The last tab of each Bot the user picked; a Bot not in it lands by [pickBot]'s rule. */
    private val lastTab = mutableMapOf<BotPick, BotTab>()

    /**
     * The user picks [e] in the list. It lands on its live talk's conversation in Chats; a Bot with no
     * live talk lands on the tab it was left on, else on Home (ADR 0086).
     */
    fun pickBot(e: BotEntry) {
        val p = e.pick
        pick = p
        routine = null
        item = null
        val talk = e.liveTalk
        terminal = talk?.id
        tab = if (talk != null) BotTab.CHATS else lastTab[p] ?: BotTab.HOME
        lastTab[p] = tab
    }

    /** Shows [t] and remembers it as the picked Bot's last tab. */
    fun selectTab(t: BotTab) {
        tab = t
        pick?.let { lastTab[it] = t }
    }

    /**
     * [t]'s Bot's page, on the conversation: a member opens on its own Bot, not its lead. It is not the
     * user picking the Bot, so a Bot never picked still lands by [pickBot]'s rule next time.
     */
    fun open(t: Terminal) {
        val name = t.bot ?: t.lead ?: return
        val workspace = t.labels["muxa.workspace"] ?: return
        val p = BotPick(workspace, name)
        pick = p
        routine = null
        item = null
        tab = BotTab.CHATS
        if (p in lastTab) lastTab[p] = BotTab.CHATS
        terminal = t.id
    }

    /** One step back on a phone: the conversation or item, else the page, else nothing. */
    fun back(): Boolean = when {
        terminal != null -> true.also { terminal = null }
        item != null -> true.also { item = null }
        pick != null || routine != null -> true.also {
            pick = null
            routine = null
        }
        else -> false
    }
}

/** A Bot as the Bots place shows it: its settings and its work. */
data class BotEntry(val workspace: SettingsWorkspace, val bot: SettingsBot, val work: BotWork) {
    val name: String get() = bot.name
    val pick: BotPick get() = BotPick(workspace.name, bot.name)

    /** Its live talks: Task agents with its `muxa.bot`. */
    val talks: List<Terminal> get() = work.terminals.filter { it.isTaskAgent && it.bot == name }

    /** Its other terminals: Routine runs, its runs as a member, and its members' runs. */
    val runs: List<Terminal> get() = work.terminals - talks.toSet()

    /** The talk a picked Bot opens on: its most recent live one. */
    val liveTalk: Terminal? get() = liveTalk(talks)
}

/**
 * The most recent of [talks] that is live. The server lists terminals in the order they started, so
 * the last one is the newest. Its state does not count: Claude Code reports `done` after every turn,
 * and the talk then waits for the user's next message.
 */
fun liveTalk(talks: List<Terminal>): Terminal? = talks.lastOrNull { it.live }

/** Every Bot of [settings], Workspace by Workspace, with its work grouped as the protocol says. */
fun botEntries(settings: Settings?, inbox: Inbox, terminals: List<Terminal>, routines: List<Routine>): List<BotEntry> =
    settings?.workspaces.orEmpty().flatMap { w ->
        w.bots.map { b -> BotEntry(w, b, BotWork.of(b.name, w.name, b.enabled, inbox, terminals, routines)) }
    }

/** Routines no Bot owns: failed or held ones first (ADR 0086). */
fun routinesWithoutBot(routines: List<Routine>): List<Routine> {
    val own = routines.filter { it.bot == null }
    val first = own.filter { it.last?.ok == false || !it.enabled }
    return first + (own - first.toSet())
}

/** What [t] is to Bot [bot]: a chat, a run, or a member's run. */
fun Terminal.botRole(bot: String): String = when {
    lead == bot -> "${this.bot ?: "a member"} works for it"
    lead != null -> "a member of $lead"
    labels["muxa.kind"] == "routine" -> labels["muxa.routine"]?.let { "run of $it" } ?: "Routine run"
    isTaskAgent -> "chat"
    else -> "terminal"
}

/**
 * Removing Bot [name]: each of its Routines moves to [target] (null: no Bot) by a full `routine-update`,
 * then `bot-remove`, as one `settings-update`, so all of it applies or none.
 */
fun SettingsWorkspace.botRemoval(name: String, target: String?): List<SettingsOp> =
    routines.filter { it.bot == name }.map { SettingsOp.RoutineUpdate(this.name, it.spec.copy(bot = target)) } +
        SettingsOp.BotRemove(this.name, name)
