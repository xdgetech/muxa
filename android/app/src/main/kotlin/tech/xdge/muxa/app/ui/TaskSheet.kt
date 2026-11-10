package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.app.pickableWorkspaces
import tech.xdge.muxa.protocol.DEFAULT_WORKER
import tech.xdge.muxa.session.LoopDraft
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.TaskRecordDraft
import tech.xdge.muxa.session.repoChoices
import tech.xdge.muxa.session.allows
import tech.xdge.muxa.session.workerProblem

/**
 * Records a Task on the host and starts an agent for it: in a directory of its workspace,
 * or with a repository, in a worktree on a branch.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewTaskRecordSheet(model: AppModel, initial: TaskRecordDraft) {
    val workspaces = model.state.tasks?.workspaces.orEmpty()
    fun choicesIn(workspace: String) =
        repoChoices(workspaces.firstOrNull { it.name == workspace }, workspaces, model.state.sessions)
    // A level 2 host names the branch after the title (ADR 0031).
    var draft by remember { mutableStateOf(initial.copy(branchFromTitle = model.state.canAsk)) }
    var withRepo by remember { mutableStateOf(initial.repoDir.isNotEmpty()) }
    // A repo the workspace does not list keeps the typed path.
    var otherPath by remember { mutableStateOf(initial.repoDir.isNotEmpty() && choicesIn(initial.workspace).none { it.dir == initial.repoDir }) }
    var creating by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var picking by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val workerIssue = workerProblem(draft.worker, draft.workspace, model.state.tasks?.workers.orEmpty())
    val plain = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false)
    fun edit(next: TaskRecordDraft) {
        draft = next
        error = null
    }
    fun create() {
        if (draft.problem != null || workerIssue != null || creating) return
        creating = true
        scope.launch {
            try {
                // A worker-aware request always names a worker; a legacy one omits it.
                model.create(draft.copy(worker = if (model.state.canUseWorkers) draft.worker ?: DEFAULT_WORKER else null))
            } catch (e: PaneSession.Failure) {
                error = PaneSession.describe(e)
            } finally {
                creating = false
            }
        }
    }
    fun ask() {
        if (creating) return
        creating = true
        scope.launch {
            try {
                model.ask(draft.workspace, draft.repoDir.trim())
            } catch (e: PaneSession.Failure) {
                error = PaneSession.describe(e)
            } finally {
                creating = false
            }
        }
    }
    val choices = choicesIn(draft.workspace)
    AlertDialog(
        onDismissRequest = { if (!creating) model.newTaskRecord = null },
        title = { Text("New Task") },
        text = {
            Column(Modifier.heightIn(max = 520.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ExposedDropdownMenuBox(picking, { picking = it }) {
                    OutlinedTextField(
                        draft.workspace, {},
                        Modifier.fillMaxWidth().menuAnchor(MenuAnchorType.PrimaryNotEditable).testTag("task-workspace"),
                        readOnly = true, singleLine = true, label = { Text("Workspace") },
                        trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(picking) },
                    )
                    ExposedDropdownMenu(picking, { picking = false }) {
                        // The overall assistant's workspace takes no Tasks from here (ADR 0049 §2).
                        for (w in model.pickableWorkspaces) {
                            DropdownMenuItem({ Text(w.name) }, {
                                // Another workspace allows other checks.
                                val loop = draft.loop?.let { w.checks?.let { checks -> LoopDraft(checks) } }
                                // Another workspace offers its own repos.
                                val inWorkspace = choicesIn(w.name)
                                val repoDir = if (!withRepo || otherPath || inWorkspace.any { it.dir == draft.repoDir }) draft.repoDir else inWorkspace.firstOrNull()?.dir.orEmpty()
                                edit(draft.copy(workspace = w.name, repoDir = repoDir, loop = loop, worker = draft.worker?.takeIf { name -> model.state.tasks?.workers.orEmpty().any { it.name == name && it.allows(w.name) } }))
                                picking = false
                            }, modifier = Modifier.testTag("workspace-${w.name}"))
                        }
                    }
                }
                OutlinedTextField(
                    draft.title, { edit(draft.copy(title = it)) }, Modifier.fillMaxWidth().testTag("task-title"),
                    label = { Text("Title") }, singleLine = true,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                )
                OutlinedTextField(
                    draft.goal, { edit(draft.copy(goal = it)) }, Modifier.fillMaxWidth().testTag("task-goal"),
                    label = { Text("Goal") }, minLines = 3,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                )
                OutlinedTextField(
                    draft.project, { edit(draft.copy(project = it)) },
                    Modifier.fillMaxWidth().testTag("task-project"), label = { Text("Project (optional)") }, singleLine = true,
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Repository", Modifier.weight(1f))
                    Switch(withRepo, {
                        withRepo = it
                        if (!it) edit(draft.copy(repoDir = "", branch = "", base = ""))
                        else if (draft.repoDir.isEmpty()) edit(draft.copy(repoDir = choices.firstOrNull()?.dir ?: ""))
                    }, Modifier.testTag("task-with-repo"))
                }
                if (withRepo) {
                    if (choices.isNotEmpty()) {
                        RepoChoiceField(choices, draft.repoDir, otherPath, pick = {
                            otherPath = false
                            edit(draft.copy(repoDir = it))
                        }, other = { otherPath = true })
                    }
                    if (choices.isEmpty() || otherPath || choices.none { it.dir == draft.repoDir }) {
                        OutlinedTextField(
                            draft.repoDir, { edit(draft.copy(repoDir = it)) }, Modifier.fillMaxWidth().testTag("task-repo"),
                            label = { Text("Repository path") }, placeholder = { Text("/path/to/repo") }, singleLine = true, keyboardOptions = plain,
                        )
                    }
                    OutlinedTextField(
                        draft.branch, { edit(draft.copy(branch = it)) }, Modifier.fillMaxWidth().testTag("task-branch"),
                        label = { Text(if (draft.branchFromTitle) "Branch (optional)" else "Branch") },
                        placeholder = { Text(if (draft.branchFromTitle) "Named after the title" else "feature/name") },
                        singleLine = true, keyboardOptions = plain,
                    )
                    OutlinedTextField(
                        draft.base, { edit(draft.copy(base = it)) }, Modifier.fillMaxWidth(),
                        label = { Text("Base (default branch)") }, singleLine = true, keyboardOptions = plain,
                    )
                }
                Text(
                    if (withRepo) {
                        "The host creates a worktree on the branch and starts an agent there with the goal."
                    } else {
                        "The host makes a directory for the task in its workspace and starts an agent there with the goal."
                    },
                    style = MaterialTheme.typography.bodySmall,
                )
                if (withRepo && model.canAsk) {
                    TextButton(
                        ::ask, Modifier.testTag("record-ask"),
                        enabled = !creating && draft.workspace.isNotBlank() && draft.repoDir.trim().startsWith("/"),
                    ) { Text("Ask about this repo") }
                    Text("Ask runs the workspace's agent in the main checkout. No task, no worktree, no record.", style = MaterialTheme.typography.bodySmall)
                }
                if (model.state.canUseWorkers) {
                    WorkerPicker(
                        model.state.tasks?.workers.orEmpty().filter { it.allows(draft.workspace) }.map { it.name },
                        draft.worker ?: DEFAULT_WORKER, "task-worker",
                    ) { edit(draft.copy(worker = it)) }
                }
                NewTaskLoop(model, draft.loop, model.checksOf(draft.workspace), defaults = if (model.state.canUseWorkers) model.state.tasks?.workers?.firstOrNull { it.name == draft.worker && it.allows(draft.workspace) }?.loop else null) { edit(draft.copy(loop = it)) }
                // The draft's problem, once there is a title to complain about.
                (error ?: workerIssue ?: draft.problem.takeIf { draft.title.isNotEmpty() })?.let {
                    Text(it, color = Theme.failure, modifier = Modifier.testTag("task-record-error"))
                }
            }
        },
        confirmButton = {
            if (creating) {
                CircularProgressIndicator()
            } else {
                TextButton(::create, enabled = draft.problem == null && workerIssue == null, modifier = Modifier.testTag("task-create")) { Text("Create") }
            }
        },
        dismissButton = { TextButton({ model.newTaskRecord = null }, enabled = !creating) { Text("Cancel") } },
    )
}
