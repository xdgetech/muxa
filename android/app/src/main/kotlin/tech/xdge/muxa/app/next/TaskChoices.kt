package tech.xdge.muxa.app.next

import tech.xdge.muxa.v2.LoopChecks
import tech.xdge.muxa.v2.LoopSpec
import tech.xdge.muxa.v2.PlaceSpec
import tech.xdge.muxa.v2.Workspace

/** What a new Task starts from: an Inbox item's title and Workspace, or nothing. */
data class TaskSeed(val title: String = "", val workspace: String? = null)

/** A `start-task` request, as the sheet builds it. */
data class TaskDraft(
    val workspace: String,
    val title: String,
    val goal: String?,
    val places: List<PlaceSpec>,
    val connections: List<String>,
    val mcp: List<String>,
    val plan: Boolean,
    val loop: LoopSpec?,
)

data class BranchChoice(val repo: String, val branch: String = "")

/**
 * What the user picked in the Start Task sheet. Connections and MCP servers on
 * by default are always on, so [connections] and [mcp] hold only the others.
 */
data class TaskChoices(
    val workspace: String = "",
    val title: String = "",
    val goal: String = "",
    val worktrees: Boolean = false,
    val branches: List<BranchChoice> = emptyList(),
    val readOnly: Set<String> = emptySet(),
    val connections: Set<String> = emptySet(),
    val mcp: Set<String> = emptySet(),
    val plan: Boolean = false,
    val loop: Boolean = false,
    val check: String = "",
    val maxAttempts: Int = DEFAULT_ATTEMPTS,
) {
    val canStart: Boolean get() = workspace.isNotEmpty() && title.isNotBlank()

    /** A new Workspace keeps the title and the goal; its places, tools and loop start over. */
    fun inWorkspace(ws: Workspace): TaskChoices = copy(
        workspace = ws.name, worktrees = false, branches = emptyList(), readOnly = emptySet(),
        connections = emptySet(), mcp = emptySet(), loop = false,
        check = (ws.loop?.checks as? LoopChecks.List)?.commands?.firstOrNull().orEmpty(),
    )

    /** Branch rows without a name are left out; a repo with a worktree is not also read-only. */
    fun draft(): TaskDraft {
        val branchPlaces = if (worktrees) {
            branches.filter { it.branch.isNotBlank() }.map { PlaceSpec.Branch(it.repo, it.branch.trim()) }
        } else {
            emptyList()
        }
        val working = branchPlaces.ifEmpty { listOf(PlaceSpec.Folder) }
        val reads = readOnly.filter { r -> branchPlaces.none { it.repo == r } }.sorted().map { PlaceSpec.Read(it) }
        return TaskDraft(
            workspace = workspace,
            title = title.trim(),
            goal = goal.trim().ifEmpty { null },
            places = working + reads,
            connections = connections.sorted(),
            mcp = mcp.sorted(),
            plan = plan,
            loop = if (loop) LoopSpec(check = check.trim().ifEmpty { null }, maxAttempts = maxAttempts) else null,
        )
    }

    companion object {
        const val DEFAULT_ATTEMPTS = 3
        const val MAX_ATTEMPTS = 10
    }
}
