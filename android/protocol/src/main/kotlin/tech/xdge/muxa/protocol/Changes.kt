package tech.xdge.muxa.protocol

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/** A file of the pane's repo that differs from HEAD. */
@Serializable
data class ChangeInfo(
    /** Relative to the repo; what to send in `diff`. */
    val path: String,
    val status: Status,
    /** The path before a rename. */
    val oldPath: String? = null,
    /** Lines added and removed; null for a binary file. */
    val added: Int? = null,
    val removed: Int? = null,
) {
    /** A status added by a newer bridge reads as [OTHER]. */
    @Serializable(with = StatusSerializer::class)
    enum class Status(val wire: String) {
        MODIFIED("modified"), ADDED("added"), DELETED("deleted"), RENAMED("renamed"),
        UNTRACKED("untracked"), CONFLICTED("conflicted"), OTHER("other");
    }

    internal object StatusSerializer : KSerializer<Status> {
        override val descriptor = PrimitiveSerialDescriptor("ChangeStatus", PrimitiveKind.STRING)
        override fun serialize(encoder: Encoder, value: Status) = encoder.encodeString(value.wire)
        override fun deserialize(decoder: Decoder): Status {
            val raw = decoder.decodeString()
            return Status.entries.firstOrNull { it.wire == raw } ?: Status.OTHER
        }
    }
}

/** Which list a `diff` path is from. */
@Serializable
enum class DiffScope {
    /** Against HEAD: staged, unstaged and untracked. */
    @SerialName("uncommitted") UNCOMMITTED,
    /** The branch's commits since it left its base. */
    @SerialName("branch") BRANCH,
}

/** The files a branch's commits changed since it left its base. */
@Serializable
data class BranchChanges(
    /** The ref compared with, e.g. `origin/main`. */
    val base: String,
    /** Commits since the fork point. */
    val commits: Int,
    val entries: List<ChangeInfo>,
    val truncated: Boolean = false,
)
