package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.WorkerAssignment
import tech.xdge.muxa.protocol.WorkerInfo

fun WorkerInfo.allows(workspace: String): Boolean = workspaces?.contains(workspace) ?: true

val WorkerAssignment.displayName: String
    get() = when {
        !isKnown -> "Unknown worker assignment"
        kind == "workspace-default" -> "Workspace default"
        else -> name ?: "Unknown worker assignment"
    }

/** Recheck the latest public scope when submitting a selection retained by an open sheet. */
fun workerProblem(name: String?, workspace: String, workers: List<WorkerInfo>): String? {
    if (name == null) return null
    return if (workers.any { it.name == name && it.allows(workspace) }) null
    else "This worker is no longer available in this workspace. Pick a worker again."
}
