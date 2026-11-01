package tech.xdge.muxa.session

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
) {
    data class Create(
        val workspace: String,
        val title: String,
        val goal: String,
        val project: String?,
        val repo: String?,
        val branch: String?,
        val base: String?,
    )

    /** What to fix before the Task can be created; null when it can. */
    val problem: String?
        get() {
            if (workspace.isBlank()) return "Pick a workspace."
            if (title.isBlank()) return "Give the task a title."
            if (goal.isBlank()) return "Describe the goal."
            val repo = repoDir.trim()
            if (repo.isEmpty()) {
                return if (branch.isNotBlank() || base.isNotBlank()) "A branch needs a repository." else null
            }
            if (!repo.startsWith("/")) return "The repository path must start with /."
            val branch = branch.trim()
            if (branch.isEmpty()) return "Name the branch."
            TaskDraft.refProblem(branch, "branch")?.let { return it }
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
                branch = repo?.let { branch.trim() },
                base = repo?.let { base.trim().ifEmpty { null } },
            )
        }
}
