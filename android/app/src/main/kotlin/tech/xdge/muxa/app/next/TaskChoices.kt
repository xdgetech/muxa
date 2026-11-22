package tech.xdge.muxa.app.next

import tech.xdge.muxa.v2.InboxItem
import tech.xdge.muxa.v2.LoopChecks
import tech.xdge.muxa.v2.LoopSpec
import tech.xdge.muxa.v2.PlaceSpec
import tech.xdge.muxa.v2.Workspace

/** What a new Task starts from: an Inbox item's title, Workspace and note, or nothing. */
data class TaskSeed(val title: String = "", val workspace: String? = null, val goal: String = "")

/** A `start-task` request, as the sheet builds it. */
data class TaskDraft(
    val workspace: String,
    val title: String,
    val goal: String?,
    val places: List<PlaceSpec>,
    val plan: Boolean,
    val loop: LoopSpec?,
) {
    companion object {
        /**
         * Resume Task of an item migrated from v1: the agent starts in the old
         * Task's place as it is. Null when the item has no place.
         */
        fun resume(item: InboxItem): TaskDraft? = item.place?.let {
            TaskDraft(it.workspace, item.title.removePrefix(RESUME_PREFIX), null, listOf(PlaceSpec.Existing(it.path)), plan = false, loop = null)
        }

        private const val RESUME_PREFIX = "Resume: "
    }
}

/**
 * What every agent of [ws] gets (ADR 0075), as "This Workspace's agents get: github, linear · MCP: docs";
 * null when it has neither.
 */
fun toolsLine(ws: Workspace): String? {
    val parts = listOfNotNull(
        ws.connections.takeIf { it.isNotEmpty() }?.joinToString(", ") { it.name },
        ws.mcp.takeIf { it.isNotEmpty() }?.joinToString(", ", prefix = "MCP: ") { it.name },
    )
    return if (parts.isEmpty()) null else "This Workspace's agents get: " + parts.joinToString(" · ")
}

data class BranchChoice(val repo: String, val branch: String = "")

/** What the user picked in the Start Task sheet. */
data class TaskChoices(
    val workspace: String = "",
    val title: String = "",
    val goal: String = "",
    val worktrees: Boolean = false,
    val branches: List<BranchChoice> = emptyList(),
    val readOnly: Set<String> = emptySet(),
    val plan: Boolean = false,
    val loop: Boolean = false,
    val check: String = "",
    val maxAttempts: Int = DEFAULT_ATTEMPTS,
) {
    val canStart: Boolean get() = workspace.isNotEmpty() && title.isNotBlank()

    /** A new Workspace keeps the title and the goal; its places and loop start over. */
    fun inWorkspace(ws: Workspace): TaskChoices = copy(
        workspace = ws.name, worktrees = false, branches = emptyList(), readOnly = emptySet(),
        loop = false,
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
            plan = plan,
            loop = if (loop) LoopSpec(check = check.trim().ifEmpty { null }, maxAttempts = maxAttempts) else null,
        )
    }

    companion object {
        const val DEFAULT_ATTEMPTS = 3
        const val MAX_ATTEMPTS = 10
    }
}
