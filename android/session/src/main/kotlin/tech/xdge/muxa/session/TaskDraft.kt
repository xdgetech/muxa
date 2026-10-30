package tech.xdge.muxa.session

/**
 * What the New Task sheet edits. Checked here for quick feedback; the bridge checks
 * again and has the final say.
 */
data class TaskDraft(
    val repoDir: String = "",
    val branch: String = "",
    /** Empty: the repo's default branch. */
    val base: String = "",
) {
    data class NewTask(val repo: String, val branch: String, val base: String?)

    /** What to fix before the task can start; null when it can. */
    val problem: String?
        get() {
            val repo = repoDir.trim()
            if (repo.isEmpty()) return "Pick a repository."
            if (!repo.startsWith("/")) return "The repository path must start with /."
            val branch = branch.trim()
            if (branch.isEmpty()) return "Name the branch."
            refProblem(branch, "branch")?.let { return it }
            val base = base.trim()
            if (base.isNotEmpty()) refProblem(base, "base")?.let { return it }
            return null
        }

    /** The `new-task` fields, trimmed; null while there is a problem. */
    val request: NewTask?
        get() {
            if (problem != null) return null
            val base = base.trim()
            return NewTask(repoDir.trim(), branch.trim(), base.ifEmpty { null })
        }

    private companion object {
        /** The common mistakes git would refuse; `git check-ref-format` on the host decides the rest. */
        fun refProblem(name: String, what: String): String? {
            if (name.startsWith("-")) return "A $what name cannot start with -."
            if (name.any { it == ' ' || it == '~' || it == '^' || it == ':' || it == '\\' } || name.contains("..")) {
                return "A $what name cannot contain spaces, .., ~, ^, : or \\."
            }
            return null
        }
    }
}
