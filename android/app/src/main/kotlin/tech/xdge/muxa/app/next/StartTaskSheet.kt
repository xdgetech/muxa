package tech.xdge.muxa.app.next

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ui.Corners
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.LocalPhoneLayout
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.v2.LoopChecks
import tech.xdge.muxa.v2.Settings
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.Workspace

/**
 * Start Task (ADR 0068, 0075): a Workspace, the title and goal, the places, plan
 * mode and an optional Task Loop. A line says which connections and MCP servers
 * the Workspace's agents get; they all get every one. Full screen
 * on a phone. `Ctrl+Enter` starts it; on success [started] gets the agent terminal.
 */
@Composable
fun StartTaskSheet(model: NextModel, seed: TaskSeed, close: () -> Unit, started: (Terminal) -> Unit) {
    var workspaces by remember { mutableStateOf<List<Workspace>?>(null) }
    var choices by remember { mutableStateOf(TaskChoices(title = seed.title, goal = seed.goal)) }
    var starting by remember { mutableStateOf(false) }
    var settings by remember { mutableStateOf<Settings?>(null) }
    val scope = rememberCoroutineScope()
    val phone = LocalPhoneLayout.current
    LaunchedEffect(Unit) {
        model.requestError = null
        val list = model.workspaces().orEmpty()
        workspaces = list
        (list.firstOrNull { it.name == seed.workspace } ?: list.firstOrNull())?.let { choices = choices.inWorkspace(it) }
        // Only for the "Runs on" line: without them the sheet still starts Tasks.
        settings = runCatching { model.settingsHost.settings() }.getOrNull()
        model.loadPlanUsage()
    }
    val ws = workspaces?.firstOrNull { it.name == choices.workspace }

    fun start() {
        if (!choices.canStart || starting) return
        scope.launch {
            starting = true
            val t = model.startTask(choices.draft())
            starting = false
            if (t != null) started(t)
        }
    }
    val startKey = Modifier.onPreviewKeyEvent { e ->
        val hit = e.type == KeyEventType.KeyDown && e.isCtrlPressed && e.key == Key.Enter
        hit.also { if (it) start() }
    }

    Dialog(close, DialogProperties(usePlatformDefaultWidth = false)) {
        val size = if (phone) Modifier.fillMaxSize() else Modifier.widthIn(max = SHEET_WIDTH.dp).heightIn(max = SHEET_HEIGHT.dp)
        // A card over the scrim on a tablet, as the mocks' sheets; the whole screen on a phone.
        Surface(
            size.then(startKey).testTag("task.sheet"),
            shape = MaterialTheme.shapes.large,
            color = if (phone) Palette.ground else Palette.surface,
            border = if (phone) null else BorderStroke(1.dp, Palette.border),
        ) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("New Task", Modifier.weight(1f), style = MaterialTheme.typography.titleLarge)
                    LinkButton("Cancel", close, Modifier.testTag("task.cancel"))
                    PrimaryButton("Start", ::start, Modifier.testTag("task.start"), enabled = choices.canStart && !starting)
                }
                model.requestError?.let { Text(it, Modifier.testTag("task.error"), color = MaterialTheme.colorScheme.error) }
                val list = workspaces
                when {
                    list == null -> Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
                    list.isEmpty() -> Text("This host has no Workspaces. Add one in workspaces.toml on the host.")
                    else -> Column(Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        WorkspacePicker(list, choices.workspace) { choices = choices.inWorkspace(it) }
                        OutlinedTextField(
                            choices.title, { choices = choices.copy(title = it) }, Modifier.fillMaxWidth().testTag("task.title"),
                            label = { Text("Title") }, singleLine = true,
                        )
                        OutlinedTextField(
                            choices.goal, { choices = choices.copy(goal = it) }, Modifier.fillMaxWidth().testTag("task.goal"),
                            label = { Text("Goal") }, minLines = 2,
                        )
                        if (ws != null) {
                            val logins = settings?.loginsFor(ws.name, ws.agent).orEmpty()
                            val runsOn = settings?.runsOn(ws.name, ws.agent, model.planUsage)
                            // Use for next Tasks (ADR 0103 §4): the Workspace's order changes on the host.
                            val use: (String) -> Unit = { name ->
                                settings?.let { s ->
                                    val all = s.workspaces.firstOrNull { it.name == ws.name }?.subscriptions.orEmpty()
                                    scope.launch {
                                        val op = SettingsOp.WorkspaceUpdate(ws.name, subscriptions = firstIn(all, name))
                                        runCatching { model.settingsHost.settingsUpdate(s.revision, listOf(op)) }
                                            .onSuccess { settings = it }
                                            .onFailure { model.requestError = "The subscription did not change: ${it.message}" }
                                    }
                                }
                            }
                            TaskOptions(ws, runsOn, logins.drop(1), use, choices) { choices = it }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun WorkspacePicker(list: List<Workspace>, picked: String, pick: (Workspace) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        SecondaryButton("Workspace: $picked", { open = true }, Modifier.fillMaxWidth().testTag("task.workspace"))
        DropdownMenu(open, { open = false }) {
            for (w in list) {
                DropdownMenuItem({ Text(w.name) }, {
                    open = false
                    pick(w)
                }, Modifier.testTag("task.workspace.${w.name}"))
            }
        }
    }
}

/** The parts that depend on the Workspace. */
@Composable
private fun TaskOptions(
    ws: Workspace,
    runsOn: String?,
    others: List<String>,
    use: (String) -> Unit,
    choices: TaskChoices,
    change: (TaskChoices) -> Unit,
) {
    Places(ws, choices, change)
    toolsLine(ws)?.let { Text(it, Modifier.testTag("task.tools"), color = Palette.textSecondary, style = MaterialTheme.typography.bodyMedium) }
    // The Workspace's order picks the login (ADR 0103 §4); Change moves another first.
    runsOn?.let {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(it, Modifier.weight(1f).testTag("task.subscription"), color = Palette.textSecondary, style = MaterialTheme.typography.bodyMedium)
            if (others.isNotEmpty()) ChangeLogin(others, use)
        }
    }
    SwitchRow("Plan mode", "task.plan", choices.plan) { change(choices.copy(plan = it)) }
    ws.loop?.let { LoopOptions(it.checks, choices, change) }
}

/** One repo, as a branch or read only, or no repo, where the agent reads every repo (ADR 0077). */
@Composable
private fun Places(ws: Workspace, choices: TaskChoices, change: (TaskChoices) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Text("Repo", style = MaterialTheme.typography.titleSmall)
    Box {
        SecondaryButton(choices.repo ?: "No repo", { open = true }, Modifier.testTag("task.repo"))
        DropdownMenu(open, { open = false }) {
            DropdownMenuItem({ Text("No repo") }, { open = false; change(choices.copy(repo = null)) }, Modifier.testTag("task.repo.none"))
            for (r in ws.repos) {
                DropdownMenuItem({ Text(r) }, { open = false; change(choices.copy(repo = r)) }, Modifier.testTag("task.repo.$r"))
            }
        }
    }
    val repo = choices.repo
    if (repo != null) {
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
            SegmentedButton(!choices.readOnly, { change(choices.copy(readOnly = false)) }, SegmentedButtonDefaults.itemShape(0, 2, Corners.button),
                Modifier.testTag("task.mode.branch")) { Text("Branch") }
            SegmentedButton(choices.readOnly, { change(choices.copy(readOnly = true)) }, SegmentedButtonDefaults.itemShape(1, 2, Corners.button),
                Modifier.testTag("task.mode.read")) { Text("Read only") }
        }
        if (!choices.readOnly) {
            OutlinedTextField(
                choices.branch, { change(choices.copy(branch = it.trim())) }, Modifier.fillMaxWidth().testTag("task.branch"),
                placeholder = { Text("Branch in $repo") }, singleLine = true,
            )
        }
    }
    val footer = when {
        repo == null -> "The agent works in a new folder and can read every repo of ${ws.name}."
        choices.readOnly -> "The agent works in a new folder and reads $repo. Read only is a rule for the agent."
        else -> "The agent works in a new worktree on this branch."
    }
    Text(footer, color = Palette.textSecondary, style = MaterialTheme.typography.bodySmall)
}

@Composable
private fun LoopOptions(checks: LoopChecks, choices: TaskChoices, change: (TaskChoices) -> Unit) {
    SwitchRow("Task Loop", "task.loop", choices.loop) { change(choices.copy(loop = it)) }
    if (!choices.loop) return
    when (checks) {
        is LoopChecks.List -> FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (c in checks.commands) {
                FilterChip(choices.check == c, { change(choices.copy(check = c)) }, { Text(c, fontFamily = FontFamily.Monospace) },
                    Modifier.testTag("task.check.$c"))
            }
        }
        LoopChecks.Any -> OutlinedTextField(
            choices.check, { change(choices.copy(check = it)) }, Modifier.fillMaxWidth().testTag("task.check"),
            label = { Text("Check") }, placeholder = { Text("make test") }, singleLine = true,
            textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
        )
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text("Stop after ${choices.maxAttempts} failed checks", Modifier.weight(1f))
        LinkButton("−", { change(choices.copy(maxAttempts = (choices.maxAttempts - 1).coerceAtLeast(1))) })
        LinkButton("+", { change(choices.copy(maxAttempts = (choices.maxAttempts + 1).coerceAtMost(TaskChoices.MAX_ATTEMPTS))) })
    }
}

@Composable
private fun SwitchRow(title: String, tag: String, on: Boolean, change: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(title, Modifier.weight(1f))
        Switch(on, change, Modifier.testTag(tag))
    }
}

private const val SHEET_WIDTH = 560
private const val SHEET_HEIGHT = 720

/** Another login of the Workspace for this Task and the next ones. */
@Composable
private fun ChangeLogin(others: List<String>, use: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        LinkButton("Change", { open = true }, Modifier.testTag("task.subscription.change"))
        DropdownMenu(open, { open = false }) {
            for (n in others) {
                DropdownMenuItem({ Text(n) }, {
                    open = false
                    use(n)
                }, Modifier.testTag("task.subscription.use.$n"))
            }
        }
    }
}
