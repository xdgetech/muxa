package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.LoopSpec
import tech.xdge.muxa.protocol.WorkspaceInfo

/**
 * What the New Task sheet edits for a task record (`create-task`). Checked here for quick
 * feedback; the bridge checks again and has the final say.
 */
data class TaskRecordDraft(
    val workspace: String = "",
    val title: String = "",
    val goal: String = "",
    val project: String = "",
    /** Empty: the Task gets a directory in its workspace instead of a worktree. */
    val repoDir: String = "",
    val branch: String = "",
    /** Empty: the repo's default branch. */
    val base: String = "",
    /** Also start a Task Loop; null for none. */
    val loop: LoopDraft? = null,
    val worker: String? = null,
    /** The host names a repo Task's branch after its title when none is given (`work` level 2, ADR 0031). */
    val branchFromTitle: Boolean = false,
    /** The Work item the Task is for, and its `rev` the client saw (`work` level 8, ADR 0046). */
    val work: String? = null,
    val workRev: Long? = null,
) {
    data class Create(
        val workspace: String,
        val title: String,
        val goal: String,
        val project: String?,
        val repo: String?,
        val branch: String?,
        val base: String?,
        val loop: LoopSpec? = null,
        val worker: String? = null,
        val work: String? = null,
        val workRev: Long? = null,
    )

    /** What to fix before the Task can be created; null when it can. */
    val problem: String?
        get() {
            if (workspace.isBlank()) return "Pick a workspace."
            if (title.isBlank()) return "Give the task a title."
            if (goal.isBlank()) return "Describe the goal."
            loop?.problem?.let { return it }
            val repo = repoDir.trim()
            if (repo.isEmpty()) {
                return if (branch.isNotBlank() || base.isNotBlank()) "A branch needs a repository." else null
            }
            if (!repo.startsWith("/")) return "The repository path must start with /."
            val branch = branch.trim()
            if (branch.isEmpty()) {
                if (!branchFromTitle) return "Name the branch."
            } else {
                TaskDraft.refProblem(branch, "branch")?.let { return it }
            }
            val base = base.trim()
            if (base.isNotEmpty()) TaskDraft.refProblem(base, "base")?.let { return it }
            return null
        }

    /** The `create-task` fields, trimmed, with empty optionals left out; null while there is a problem. */
    val request: Create?
        get() {
            if (problem != null) return null
            val repo = repoDir.trim().ifEmpty { null }
            return Create(
                workspace = workspace,
                title = title.trim(),
                goal = goal.trim(),
                project = project.trim().ifEmpty { null },
                repo = repo,
                branch = repo?.let { branch.trim().ifEmpty { null } },
                base = repo?.let { base.trim().ifEmpty { null } },
                loop = loop?.spec,
                worker = worker,
                work = work,
                workRev = workRev,
            )
        }

    companion object {
        /**
         * A draft in [repoDir], in the workspace whose `repos` lists it, else in the first
         * workspace. An empty [repoDir] means a task directory.
         */
        fun inRepo(repoDir: String, workspaces: List<WorkspaceInfo>): TaskRecordDraft {
            val workspace = workspaces.firstOrNull { repoDir in it.repos } ?: workspaces.firstOrNull()
            return TaskRecordDraft(workspace = workspace?.name.orEmpty(), repoDir = repoDir)
        }
    }
}
