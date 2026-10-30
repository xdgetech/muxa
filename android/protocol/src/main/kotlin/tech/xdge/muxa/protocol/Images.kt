package tech.xdge.muxa.protocol

import kotlinx.serialization.Serializable

/** An image file in the pane's repo, ignored ones included. */
@Serializable
data class ImageInfo(
    /** Relative to the repo; what to send in `image`. */
    val path: String,
    /** Epoch seconds of the last change. */
    val mtime: Long? = null,
    /** Bytes. */
    val size: Long? = null,
)
