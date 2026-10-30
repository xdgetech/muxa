package tech.xdge.muxa.app.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.TaskDraft
import tech.xdge.muxa.session.defaultPane
import tech.xdge.muxa.session.groupedByRepo
import tech.xdge.muxa.session.knownRepos

/** Ctrl+Shift+K: every session with its agent's state. Enter opens the first match. */
@Composable
fun SessionSwitcher(model: AppModel, onDismiss: () -> Unit) {
    var query by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    val matches = model.visibleSessions.filter {
        query.isEmpty() || listOfNotNull(it.name, it.repo, it.branch).any { f -> f.contains(query, ignoreCase = true) }
    }.groupedByRepo()
    fun open(s: Session) {
        onDismiss()
        model.open(s)
    }
    // The list can be stale: the host reports changes only for the bridge's own session.
    LaunchedEffect(Unit) {
        runCatching { model.session.refresh() }
        runCatching { focus.requestFocus() }
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Sessions") },
        text = {
            Column {
                OutlinedTextField(
                    query, { query = it },
                    Modifier.fillMaxWidth().focusRequester(focus).testTag("switcher-search"),
                    placeholder = { Text("Search") }, singleLine = true,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Go, autoCorrectEnabled = false),
                    keyboardActions = KeyboardActions(onGo = { matches.firstOrNull()?.sessions?.firstOrNull()?.let(::open) }),
                )
                LazyColumn(Modifier.heightIn(max = 420.dp).padding(top = 8.dp)) {
                    for (group in matches) {
                        item(key = "h-${group.repo}") {
                            Text(group.repo ?: "Other Sessions", Modifier.padding(top = 12.dp, bottom = 4.dp), style = MaterialTheme.typography.labelLarge)
                        }
                        items(group.sessions, key = { it.id }) { s -> SwitcherRow(s, s.id == model.currentSession?.id) { open(s) } }
                    }
                }
            }
        },
        confirmButton = {
            if (model.canStartTasks) TextButton({ model.startNewTask() }) { Text("New Task") }
        },
        dismissButton = { TextButton(onDismiss) { Text("Close") } },
    )
}

@Composable
private fun SwitcherRow(s: Session, current: Boolean, open: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = open).padding(vertical = 8.dp).testTag("switch-${s.name}"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(s.name, fontWeight = FontWeight.SemiBold)
            // The repo is the section header; the branch tells sessions apart.
            listOfNotNull(s.branch, s.task).joinToString(" · ").takeIf { it.isNotEmpty() }?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        s.defaultPane?.let { AgentBadge(it) }
        if (current) Icon(Icons.done, "Current", tint = Theme.accent)
    }
}

/** Starts a task on the host: a worktree on a branch and a session with an agent in it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewTaskSheet(model: AppModel, initial: TaskDraft) {
    val repos = model.state.sessions.knownRepos
    var draft by remember { mutableStateOf(initial) }
    var otherRepo by remember { mutableStateOf(initial.repoDir.isNotEmpty() && repos.none { it.dir == initial.repoDir }) }
    var starting by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var picking by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val branchFocus = remember { FocusRequester() }
    val plain = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false)
    fun edit(next: TaskDraft) {
        draft = next
        error = null
    }
    fun start() {
        if (draft.problem != null || starting) return
        starting = true
        scope.launch {
            try {
                model.start(draft)
            } catch (e: PaneSession.Failure) {
                error = PaneSession.describe(e)
            } finally {
                starting = false
            }
        }
    }
    LaunchedEffect(Unit) { runCatching { branchFocus.requestFocus() } }
    AlertDialog(
        onDismissRequest = { if (!starting) model.newTask = null },
        title = { Text("New Task") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Repository", style = MaterialTheme.typography.titleSmall)
                if (repos.isEmpty() || otherRepo) {
                    OutlinedTextField(draft.repoDir, { edit(draft.copy(repoDir = it)) }, Modifier.fillMaxWidth().testTag("task-repo"),
                        placeholder = { Text("/path/to/repo") }, singleLine = true, keyboardOptions = plain)
                } else {
                    ExposedDropdownMenuBox(picking, { picking = it }) {
                        OutlinedTextField(
                            repos.firstOrNull { it.dir == draft.repoDir }?.name ?: draft.repoDir, {},
                            Modifier.fillMaxWidth().menuAnchor(MenuAnchorType.PrimaryNotEditable).testTag("task-repo"),
                            readOnly = true, singleLine = true,
                            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(picking) },
                        )
                        ExposedDropdownMenu(picking, { picking = false }) {
                            for (repo in repos) {
                                DropdownMenuItem({ Text(repo.name) }, {
                                    edit(draft.copy(repoDir = repo.dir))
                                    picking = false
                                })
                            }
                        }
                    }
                }
                if (repos.isNotEmpty()) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("Other path", Modifier.weight(1f))
                        Switch(otherRepo, { otherRepo = it })
                    }
                }
                Text("Branch", style = MaterialTheme.typography.titleSmall)
                OutlinedTextField(draft.branch, { edit(draft.copy(branch = it)) }, Modifier.fillMaxWidth().focusRequester(branchFocus).testTag("task-branch"),
                    placeholder = { Text("feature/name") }, singleLine = true,
                    keyboardOptions = plain.copy(imeAction = ImeAction.Go), keyboardActions = KeyboardActions(onGo = { start() }))
                OutlinedTextField(draft.base, { edit(draft.copy(base = it)) }, Modifier.fillMaxWidth(),
                    placeholder = { Text("Base (default branch)") }, singleLine = true, keyboardOptions = plain)
                Text(
                    "An existing branch is checked out; a new one starts from the base. The host creates a worktree and a session with an agent in it.",
                    style = MaterialTheme.typography.bodySmall,
                )
                // The draft's problem, once there is a branch to complain about.
                (error ?: draft.problem.takeIf { draft.branch.isNotEmpty() })?.let {
                    Text(it, color = Theme.failure, modifier = Modifier.testTag("task-error"))
                }
            }
        },
        confirmButton = {
            if (starting) {
                CircularProgressIndicator()
            } else {
                TextButton(::start, enabled = draft.problem == null, modifier = Modifier.testTag("task-start")) { Text("Start") }
            }
        },
        dismissButton = { TextButton({ model.newTask = null }, enabled = !starting) { Text("Cancel") } },
    )
}
