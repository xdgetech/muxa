package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.KindPalette
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.app.ui.Theme
import tech.xdge.muxa.v2.FoundLogin
import tech.xdge.muxa.v2.Settings
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SubscriptionUsage
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.TokenUsage
import tech.xdge.muxa.v2.UsageWindow
import java.text.DateFormat
import java.util.Date

/**
 * The host's subscriptions (ADR 0103): first the logins it found that are no subscription yet, then the
 * subscriptions with their 5-hour line. The login itself stays with the agent. [open] shows a login terminal.
 */
@Composable
fun SubscriptionsSection(editor: SettingsEditor, model: NextModel, usage: List<SubscriptionUsage>, open: (Terminal) -> Unit) {
    val settings = editor.settings ?: return
    var editing by remember { mutableStateOf<SubscriptionDraft?>(null) }
    SettingsHeader("Subscriptions")
    if (settings.foundLogins.isNotEmpty()) FoundCard(editor, settings.foundLogins)
    SettingsCard(Modifier.testTag("settings.subscriptions")) {
        for (s in settings.subscriptions) {
            EntryRow("settings.sub.${s.name}", !editor.readOnly, { editing = SubscriptionDraft.of(s) }, badge = null) {
                EntryTitle(s.name) { Chip(agentTitle(s.agent), KindPalette.todo) }
                EntryDetail(listOfNotNull(s.plan, PlanWindow.line(usage.firstOrNull { it.name == s.name })).joinToString(" · "))
            }
            CardDivider()
        }
        ActionRow("+ Add subscription", "settings.sub.add", !editor.readOnly) { editing = SubscriptionDraft() }
    }
    SettingsNote("A subscription is an agent's own login, such as a Claude Max or a ChatGPT plan. muxa keeps its name and folder; the login stays with the agent, and muxa never reads it.")
    editing?.let { d ->
        SubscriptionDialog(editor, model, settings, usage.firstOrNull { it.name == d.existing }, d, { editing = null }) { t ->
            editing = null
            open(t)
        }
    }
}

/** Onboarding (ADR 0103 §7): what the host found, each with a suggested name. */
@Composable
private fun FoundCard(editor: SettingsEditor, found: List<FoundLogin>) {
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    val on = !editor.readOnly && !editor.saving
    fun add(list: List<FoundLogin>) = scope.launch { error = editor.save(list.map(Found::op)) }
    SettingsCard(Modifier.testTag("settings.found")) {
        CardTitle(if (found.size == 1) "muxa found 1 login" else "muxa found ${found.size} logins")
        for (f in found) {
            Row(Modifier.fillMaxWidth().padding(start = 14.dp, end = 6.dp, top = 6.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(Found.name(f), fontSize = 14.sp, fontWeight = FontWeight.Medium)
                    EntryDetail(Found.origin(f))
                }
                LinkButton("Add", { add(listOf(f)) }, Modifier.testTag("settings.found.add.${Found.name(f)}"), enabled = on)
            }
        }
        if (found.size > 1) {
            CardDivider()
            ActionRow("Add all ${found.size}", "settings.found.addall", on) { add(found) }
        }
    }
    error?.let { Text(it, Modifier.padding(horizontal = 18.dp).testTag("settings.found.error"), color = Theme.failure, fontSize = 13.sp) }
    SettingsNote("Found by the folder names and the Workspaces' env only. A Workspace keeps the same login: its env variable becomes the subscription it lists.")
}

@Composable
private fun SubscriptionDialog(
    editor: SettingsEditor,
    model: NextModel,
    settings: Settings,
    usage: SubscriptionUsage?,
    initial: SubscriptionDraft,
    close: () -> Unit,
    open: (Terminal) -> Unit,
) {
    var d by remember { mutableStateOf(initial) }
    val name = d.existing
    FormDialog(
        name ?: "New Subscription", "settings.sub.form", d.isValid, { editor.save(listOf(d.op())) }, close,
        remove = name?.let { n -> { editor.save(listOf(SettingsOp.SubscriptionRemove(n))) } },
        removeTitle = "Remove Subscription",
    ) {
        if (name == null) Field(d.name, { d = d.copy(name = it) }, "Name", "sub.name", placeholder = "claude-work")
        Choice(listOf(WorkerDraft.CLAUDE_CODE, WorkerDraft.CODEX), listOf("Claude Code", "Codex"), d.agent, "sub.agent") { d = d.copy(agent = it) }
        Field(d.home, { d = d.copy(home = it) }, "Folder", "sub.home", mono = true, placeholder = d.folder,
            help = "One folder per account: its CLAUDE_CONFIG_DIR or CODEX_HOME. ~/.claude and ~/.codex are the default logins.")
        Field(d.plan, { d = d.copy(plan = it) }, "Plan", "sub.plan", placeholder = "Max 20x", help = "A label only.")
        if (name != null) {
            Text("Limits", style = MaterialTheme.typography.titleSmall)
            Limits(usage)
            LoginButton(model, name, d.command, open)
            Text("Used by", style = MaterialTheme.typography.titleSmall)
            Text(settings.usersOf(name).joinToString(", ").ifEmpty { "Nothing lists it yet." }, Modifier.testTag("sub.usedby"),
                color = Palette.textSecondary, fontSize = 13.sp)
        }
    }
}

/** Opens a terminal on the host that runs the agent with this folder; the user logs in there. */
@Composable
private fun LoginButton(model: NextModel, name: String, command: String, open: (Terminal) -> Unit) {
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    SecondaryButton("Log in on the host", {
        scope.launch {
            val t = model.startLogin(name)
            if (t != null) open(t) else error = model.requestError ?: "The login terminal did not start."
        }
    }, Modifier.fillMaxWidth().testTag("sub.login"))
    error?.let { Text(it, Modifier.testTag("sub.login.error"), color = Theme.failure, fontSize = 13.sp) }
    Text("Log in in that terminal; muxa does not see it. In a shell on the host: $command", style = MaterialTheme.typography.bodySmall)
}

/** The Usage part of Settings (ADR 0103 §8): each subscription's windows as its agents last reported them. */
@Composable
fun UsageSection(editor: SettingsEditor, usage: List<SubscriptionUsage>, tokens: TokenUsage?, terminals: List<Terminal>, open: (Terminal) -> Unit) {
    val subs = editor.settings?.subscriptions.orEmpty()
    if (subs.isEmpty()) return
    SettingsHeader("Usage")
    SettingsCard(Modifier.testTag("settings.usage")) {
        for (s in subs) {
            val u = usage.firstOrNull { it.name == s.name }
            Column(Modifier.padding(horizontal = 14.dp, vertical = 10.dp).testTag("usage.${s.name}").semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(s.name, Modifier.weight(1f), fontSize = 14.sp, fontWeight = FontWeight.Medium)
                    u?.asOf?.let { EntryDetail("as of ${time(it)}") }
                }
                Limits(u)
            }
            CardDivider()
        }
    }
    SettingsNote(
        "Limits come from the agents' own reports: Claude Code's status line while a session runs, and Codex's session logs. " +
            "At ${PlanWindow.WARN}% muxa warns; from ${PlanWindow.SWITCH}% new agents take the next subscription in their list. " +
            "Tokens come from the agents' transcripts on the host.",
    )
    tokens?.let { TokenCards(it, terminals, open) }
}

/** Today's tokens (ADR 0103 §6): each Workspace, then the ten Task folders that used most. A running Task's row opens it. */
@Composable
private fun TokenCards(tokens: TokenUsage, terminals: List<Terminal>, open: (Terminal) -> Unit) {
    SettingsHeader("Today by Workspace")
    SettingsCard(Modifier.testTag("usage.workspaces").semantics(mergeDescendants = true) {}) {
        if (tokens.workspaces.isEmpty()) TokenRow("No agent used tokens today.", null, "")
        for (w in tokens.workspaces) {
            TokenRow(w.workspace, null, TokenText.line(w.input, w.cached, w.output))
            CardDivider()
        }
    }
    if (tokens.tasks.isEmpty()) return
    SettingsHeader("Today by Task")
    SettingsCard(Modifier.testTag("usage.tasks")) {
        for (t in tokens.tasks.take(TokenText.TASKS)) {
            val agent = TokenText.agentOf(t, terminals)
            val row = Modifier.testTag("usage.task.${t.title}")
            Box(if (agent == null) row.semantics(mergeDescendants = true) {} else row.clickable(onClickLabel = "Open the Task") { open(agent) }) {
                TokenRow(TokenText.taskName(t, terminals), t.workspace, TokenText.line(t.input, t.cached, t.output))
            }
            CardDivider()
        }
    }
}

@Composable
private fun TokenRow(name: String, detail: String?, line: String) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(name, fontSize = 14.sp)
            detail?.let { Text(it, color = Palette.textSecondary, fontSize = 12.sp) }
        }
        Text(line, color = Palette.textSecondary, fontSize = 13.sp)
    }
}

@Composable
private fun Limits(usage: SubscriptionUsage?) {
    val windows = usage?.windows.orEmpty()
    if (windows.isEmpty()) {
        Text("No data yet. It shows after the first agent on this login answers.", color = Palette.textSecondary, fontSize = 13.sp)
    }
    for (w in windows) PlanMeter(w)
}

/** One window as a bar, its percent and when it resets. */
@Composable
fun PlanMeter(window: UsageWindow) {
    val label = PlanWindow.label(window.minutes)
    Row(
        Modifier.fillMaxWidth().semantics(mergeDescendants = true) {
            contentDescription = if (window.hasReset()) "$label reset" else "${window.usedPercent}% of $label"
        },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(label, Modifier.width(40.dp), color = Palette.textSecondary, fontSize = 12.sp)
        if (window.hasReset()) {
            Text("reset, no new data", Modifier.weight(1f), color = Palette.textTertiary, fontSize = 12.sp)
            return@Row
        }
        val color = meterColor(window.usedPercent)
        Box(Modifier.weight(1f).height(6.dp).background(Palette.control, RoundedCornerShape(3.dp))) {
            Box(Modifier.fillMaxWidth(window.usedPercent.coerceIn(0, 100) / 100f).height(6.dp).background(color, RoundedCornerShape(3.dp)))
        }
        Text("${window.usedPercent}%", color = color, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
        Text("resets ${time(window.resetsAt)}", color = Palette.textTertiary, fontSize = 12.sp)
    }
}

/** Green below 80%, amber to 95%, red from 95%: the thresholds of ADR 0103. */
fun meterColor(percent: Int): Color = when {
    percent >= PlanWindow.SWITCH -> Theme.failure
    percent >= PlanWindow.WARN -> Theme.attention
    else -> Theme.connected
}

private fun time(seconds: Long): String = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(seconds * 1000))

/**
 * An ordered list of subscription names, as a Workspace, a Bot or the Capture assistant keeps it:
 * the first of the agent's kind runs it. [available] is what may be added, in the host's order.
 */
@Composable
fun SubscriptionOrder(
    names: List<String>,
    available: List<String>,
    usage: List<SubscriptionUsage>,
    tag: String,
    enabled: Boolean,
    /** The first of each kind; shown as Current when [use] is set. */
    current: Set<String> = emptySet(),
    /** Puts a login first of its kind at once (ADR 0103 §4); null: no Current and no Use. */
    use: ((String) -> Unit)? = null,
    change: (List<String>) -> Unit,
) {
    names.forEachIndexed { i, n ->
        Row(Modifier.fillMaxWidth().testTag("$tag.$n"), verticalAlignment = Alignment.CenterVertically) {
            Text("${i + 1}", Modifier.width(20.dp), color = Palette.textTertiary, fontSize = 12.sp)
            Column(Modifier.weight(1f)) {
                Text(n, fontSize = 14.sp)
                EntryDetail(PlanWindow.line(usage.firstOrNull { it.name == n }))
            }
            if (use != null) {
                if (n in current) {
                    Text("Current", Modifier.testTag("$tag.current.$n"), color = Palette.textSecondary, fontSize = 12.sp)
                } else {
                    LinkButton("Use", { use(n) }, Modifier.testTag("$tag.use.$n"), enabled = enabled)
                }
            }
            LinkButton("Up", { change(names.toMutableList().apply { add(i - 1, removeAt(i)) }) }, Modifier.testTag("$tag.up.$n"), enabled = enabled && i > 0)
            LinkButton("Remove", { change(names - n) }, Modifier.testTag("$tag.remove.$n"), enabled = enabled)
        }
    }
    val rest = available.filter { it !in names }
    if (rest.isNotEmpty()) {
        NamePicker("Add", "$tag.add", rest, null) { picked -> picked?.let { change(names + it) } }
    } else if (names.isEmpty()) {
        Text("The host has no subscriptions yet. Add them under Settings → Subscriptions.", style = MaterialTheme.typography.bodySmall)
    }
}
