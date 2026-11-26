package tech.xdge.muxa.v2

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** One row of `disk` (ADR 0102): a Task place, another worktree or a build folder; sizes in bytes. */
@Serializable
data class DiskRow(
    val path: String,
    /** `task-worktree`, `task-folder`, `worktree`, `claude-worktree` or `build-folder`. */
    val kind: String,
    val workspace: String? = null,
    val branch: String? = null,
    val bytes: Long? = null,
    @SerialName("build_bytes") val buildBytes: Long? = null,
    /** `live` or `ended`; null when nothing holds it. */
    val held: String? = null,
    val clean: Boolean? = null,
    val merged: Boolean? = null,
    /** `diskClean` without `force` removes it. */
    val cleanable: Boolean = false,
)

/** A row `diskClean` did not remove, and why. */
@Serializable
data class DiskKept(val path: String, val why: String)

/** What `diskClean` removed, or would remove on a dry run. */
data class DiskCleaned(val dryRun: Boolean, val removed: List<String>, val kept: List<DiskKept>)
