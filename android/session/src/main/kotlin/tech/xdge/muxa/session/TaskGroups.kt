package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.WorkspaceInfo

/** The Tasks of one workspace in Tasks mode (ADR 0055), grouped by repo. */
data class WorkspaceTasks(val workspace: String, val repos: List<RepoTasks>) {
    /** Stable across refreshes; the client stores it to keep a group folded. */
    val foldKey: String get() = "w:$workspace"
}

/**
 * The Tasks of one repo, or of no repo ([repo] null), inside a workspace. [heading] is false
 * when the workspace has no real repo: its Tasks sit under the workspace alone.
 */
data class RepoTasks(
    val workspace: String,
    val repo: String?,
    val label: String,
    val heading: Boolean,
    val items: List<TaskItem>,
) {
    val foldKey: String get() = "r:$workspace:${repo ?: "~"}"

    companion object {
        const val NO_REPO_LABEL = "No repo"
    }
}

/** The desktop tree sorts with `LC_ALL=C`; byte order keeps both clients alike. */
private val byteOrder = Comparator<String> { a, b ->
    val x = a.encodeToByteArray()
    val y = b.encodeToByteArray()
    for (i in 0 until minOf(x.size, y.size)) {
        val d = (x[i].toInt() and 0xff) - (y[i].toInt() and 0xff)
        if (d != 0) return@Comparator d
    }
    x.size - y.size
}

/**
 * Workspaces, then repos, both in byte order with "no repo" last. Items keep the [inbox]
 * order: what they need, then the newest change.
 */
fun List<InboxSection>.groupedByWorkspace(workspaces: List<WorkspaceInfo>): List<WorkspaceTasks> {
    val items = flatMap { it.items }
    return items.map { it.record.workspace }.distinct().sortedWith(byteOrder).map { name ->
        val root = workspaces.firstOrNull { it.name == name }?.root
        val members = items.filter { it.record.workspace == name }
        val repos = members.mapNotNull { it.record.repo }.distinct()
            .map { it to repoDisplayName(it, root) }
            .sortedWith(compareBy(byteOrder) { it: Pair<String, String> -> it.second }.thenBy(byteOrder) { it.first })
            .map { (dir, label) -> RepoTasks(name, dir, label, true, members.filter { it.record.repo == dir }) }
        val loose = members.filter { it.record.repo == null }
        val noRepo = if (loose.isEmpty()) emptyList() else listOf(RepoTasks(name, null, RepoTasks.NO_REPO_LABEL, repos.isNotEmpty(), loose))
        WorkspaceTasks(name, repos + noRepo)
    }
}
