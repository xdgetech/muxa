package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.DEFAULT_WORKER
import tech.xdge.muxa.protocol.WorkerInfo

fun WorkerInfo.allows(workspace: String): Boolean = workspaces?.contains(workspace) ?: true

/** A run's or operation's requested worker. Null is a legacy run: unrecorded. */
fun workerLabel(name: String?): String = when (name) {
    null -> "Worker unrecorded"
    DEFAULT_WORKER -> "Workspace default"
    else -> name
}

/** Recheck the latest public scope when submitting a selection retained by an open sheet. */
fun workerProblem(name: String?, workspace: String, workers: List<WorkerInfo>): String? {
    if (name == null || name == DEFAULT_WORKER) return null
    return if (workers.any { it.name == name && it.allows(workspace) }) null
    else "This worker is no longer available in this workspace. Pick a worker again."
}
