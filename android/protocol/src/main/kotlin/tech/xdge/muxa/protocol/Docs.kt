package tech.xdge.muxa.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** A Markdown document a pane may read: one of its repo's, or one its agent writes. */
@Serializable
data class DocInfo(
    /** What to send in `doc`: relative to the repo, or `plans/<name>`. */
    val path: String,
    val kind: Kind,
    val title: String? = null,
    /** Epoch seconds of the last change. */
    val mtime: Long? = null,
) {
    @Serializable
    enum class Kind {
        @SerialName("repo") REPO,
        /** Written by the pane's agent outside the repo, e.g. a plan. */
        @SerialName("plan") PLAN,
    }
}
