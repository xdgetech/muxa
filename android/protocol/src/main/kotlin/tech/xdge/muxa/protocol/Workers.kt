package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

/** The reserved worker name for the workspace default agent (ADR 0027). A host worker may not use it. */
const val DEFAULT_WORKER = "default"

/** Public host metadata; no args, instructions or environment values. */
@Serializable
data class WorkerInfo(
    val name: String,
    val agent: String,
    val description: String? = null,
    val workspaces: List<String>? = null,
    val loop: LoopSpec? = null,
)
