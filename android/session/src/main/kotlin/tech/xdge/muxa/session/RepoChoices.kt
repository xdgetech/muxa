package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.WorkspaceInfo

/** A repo the New Task sheet offers. [dir] is the main checkout, absolute; [name] is what the list shows. */
data class RepoChoice(val dir: String, val name: String)

private val homePrefix = Regex("^/(Users|home)/[^/]+(?=/|$)|^/root(?=/|$)")

/**
 * How [dir] is shown: relative to [root] when it lies under it (ADR 0031), else with the
 * home folder as `~`, else as it is.
 */
fun repoDisplayName(dir: String, root: String?): String {
    val base = root?.trimEnd('/')
    if (!base.isNullOrEmpty() && dir.startsWith("$base/") && dir.length > base.length + 1) {
        return dir.removePrefix("$base/")
    }
    val home = homePrefix.find(dir) ?: return dir
    return "~" + dir.substring(home.range.last + 1)
}

/**
 * The repos to offer in [workspace]: its `repos`. A host whose workspaces list no repos
 * at all is older; then the repos of its sessions are offered instead.
 */
fun repoChoices(workspace: WorkspaceInfo?, workspaces: List<WorkspaceInfo>, sessions: List<Session>): List<RepoChoice> {
    if (workspaces.all { it.repos.isEmpty() }) return sessions.knownRepos.map { RepoChoice(it.dir, it.name) }
    return workspace?.repos.orEmpty().map { RepoChoice(it, repoDisplayName(it, workspace?.root)) }
}

/** The workspace [dir] belongs to: one whose `repos` lists it, else one whose root holds it. */
fun List<WorkspaceInfo>.containing(dir: String): WorkspaceInfo? =
    firstOrNull { dir in it.repos }
        ?: firstOrNull { w -> w.root?.trimEnd('/')?.takeIf { it.isNotEmpty() }?.let { dir.startsWith("$it/") } == true }
