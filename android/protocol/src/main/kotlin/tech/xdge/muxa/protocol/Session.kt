package tech.xdge.muxa.protocol

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/** A tmux session as the bridge reports it. */
@Serializable
data class Session(
    val id: String,
    val name: String,
    val repo: String? = null,
    /** The repo's main checkout, absolute. */
    val repoDir: String? = null,
    val branch: String? = null,
    val worktree: String? = null,
    val task: String? = null,
    val panes: List<Pane>,
)

@Serializable
data class Pane(
    val id: String,
    val window: String,
    val index: Int,
    val role: Role,
    val cols: Int,
    val rows: Int,
    val command: String,
    val path: String,
    val dead: Boolean,
    /** The foreground process is a shell; an agent state is then likely stale. */
    val shellForeground: Boolean,
    val agent: Agent? = null,
)

/** A role added by a newer bridge reads as [UNKNOWN]. */
@Serializable(with = RoleSerializer::class)
enum class Role(val wire: String) {
    AGENT("agent"), SHELL("shell"), HIDDEN("hidden"), UNKNOWN("unknown");
}

internal object RoleSerializer : KSerializer<Role> {
    override val descriptor = PrimitiveSerialDescriptor("Role", PrimitiveKind.STRING)
    override fun serialize(encoder: Encoder, value: Role) = encoder.encodeString(value.wire)
    override fun deserialize(decoder: Decoder): Role {
        val raw = decoder.decodeString()
        return Role.entries.firstOrNull { it.wire == raw } ?: Role.UNKNOWN
    }
}

@Serializable
data class Agent(
    val state: AgentState,
    /** Epoch seconds of the last state change. */
    val since: Long? = null,
)

@Serializable
enum class AgentState {
    @SerialName("running") RUNNING,
    @SerialName("waiting") WAITING,
    @SerialName("idle") IDLE,
    @SerialName("done") DONE,
    @SerialName("error") ERROR,
}
