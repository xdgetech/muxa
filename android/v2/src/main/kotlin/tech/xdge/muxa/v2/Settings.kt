package tech.xdge.muxa.v2

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/**
 * The host's editable view of `workspaces.toml` and each Workspace's `.muxa/routines.toml`
 * (ADR 0074). [revision] changes when any of the files changes; a `settings-update` names it.
 */
@Serializable
data class Settings(
    val revision: String,
    /** The daemon runs with `--settings-read-only`: every update is refused. */
    @SerialName("read_only") val readOnly: Boolean = false,
    /** Why `workspaces.toml` cannot be used as it is now. */
    val problem: String? = null,
    val workspaces: List<SettingsWorkspace> = emptyList(),
    val workers: List<WorkerSpec> = emptyList(),
)

/** A `[[workspace]]`. Paths are as written, `~/` included. */
@Serializable
data class SettingsWorkspace(
    val name: String,
    val root: String,
    val repos: List<String> = emptyList(),
    /** `30m`, `12h`, `2d`; null: never parked (ADR 0034). */
    @SerialName("park_after") val parkAfter: String? = null,
    val agent: SettingsAgent = SettingsAgent(),
    /** Null: no Task Loops. */
    val loop: WorkspaceLoop? = null,
    @SerialName("routines_enabled") val routinesEnabled: Boolean = false,
    @SerialName("direct_writes") val directWrites: Boolean = false,
    val connections: List<SettingsConnection> = emptyList(),
    val mcp: List<McpSpec> = emptyList(),
    /** Listed also while [routinesEnabled] is false. */
    val routines: List<SettingsRoutine> = emptyList(),
    /** Why `.muxa/routines.toml` cannot be used as it is now. */
    @SerialName("routines_problem") val routinesProblem: String? = null,
)

/** `[workspace.agent]`. [command] null: the daemon's `--task-command`. */
@Serializable
data class SettingsAgent(val command: String? = null, val env: List<EnvKey> = emptyList())

/** An env variable's key; its value never leaves the host. */
@Serializable
data class EnvKey(val key: String, val set: Boolean = true)

/** Whether a connection's Keychain secret exists; never the secret. */
@Serializable(with = SecretStateSerializer::class)
enum class SecretState(val wire: String) {
    SET("set"),
    MISSING("missing"),

    /** The host's Keychain is locked. */
    LOCKED("locked"),

    /** The host has no macOS Keychain. */
    UNAVAILABLE("unavailable"),

    /** A state this client does not know yet. */
    UNKNOWN("unknown"),
}

/** Decodes a state this client does not know as [SecretState.UNKNOWN]. */
internal object SecretStateSerializer : KSerializer<SecretState> {
    override val descriptor: SerialDescriptor = PrimitiveSerialDescriptor("SecretState", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: SecretState) = encoder.encodeString(value.wire)

    override fun deserialize(decoder: Decoder): SecretState {
        val wire = decoder.decodeString()
        return SecretState.entries.firstOrNull { it.wire == wire } ?: SecretState.UNKNOWN
    }
}

/** A `[[workspace.connection]]` with its secret's state. */
@Serializable
data class SettingsConnection(
    val name: String,
    /** The variable the secret goes into. */
    val env: String,
    val account: String? = null,
    /** `read`, `write` or null. */
    val access: String? = null,
    val secret: SecretState = SecretState.UNKNOWN,
) {
    val spec: ConnectionSpec get() = ConnectionSpec(name, env, account, access)
}

/** A connection in an op, without its secret. Every agent of the Workspace gets it (ADR 0075). */
@Serializable
data class ConnectionSpec(
    val name: String,
    val env: String,
    val account: String? = null,
    /** `read`, `write` or null. */
    val access: String? = null,
)

/** A `[[workspace.mcp]]`: [command] with [args], or [url] with [type] `http` (default) or `sse`. Nothing here is a secret.
 * Every agent of the Workspace gets it (ADR 0075). */
@Serializable
data class McpSpec(
    val name: String,
    val command: String? = null,
    val args: List<String>? = null,
    val url: String? = null,
    val type: String? = null,
    val env: Map<String, String> = emptyMap(),
    val headers: Map<String, String> = emptyMap(),
    val connections: List<String> = emptyList(),
)

/**
 * A `[[routine]]` with `[routine.output]` flattened. [instructions] is relative to `.muxa/`.
 * A run gets all its Workspace's connections and MCP servers (ADR 0075).
 */
@Serializable
data class RoutineSpec(
    val name: String,
    val at: String? = null,
    val schedule: String? = null,
    val timezone: String? = null,
    val worker: String? = null,
    val instructions: String,
    val project: String? = null,
    val enabled: Boolean = true,
    val items: Int? = null,
    @SerialName("direct_writes") val directWrites: Boolean = false,
)

/** A Routine as the files define it, with its instruction file's content; null when it cannot be read. */
@Serializable
data class SettingsRoutine(
    val name: String,
    val at: String? = null,
    val schedule: String? = null,
    val timezone: String? = null,
    val worker: String? = null,
    val instructions: String,
    val project: String? = null,
    val enabled: Boolean = true,
    val items: Int? = null,
    @SerialName("direct_writes") val directWrites: Boolean = false,
    @SerialName("instructions_text") val instructionsText: String? = null,
) {
    val spec: RoutineSpec
        get() = RoutineSpec(name, at, schedule, timezone, worker, instructions, project, enabled, items, directWrites)

    companion object {
        fun of(spec: RoutineSpec, instructionsText: String?) = with(spec) {
            SettingsRoutine(name, at, schedule, timezone, worker, instructions, project, enabled, items, directWrites, instructionsText)
        }
    }
}

/** A `[[worker]]`. [agent] is `claude-code` or `codex`. */
@Serializable
data class WorkerSpec(
    val name: String,
    val description: String? = null,
    val agent: String,
    val args: List<String> = emptyList(),
    val instructions: String? = null,
    val workspaces: List<String>? = null,
    val loop: WorkerLoop? = null,
)

@Serializable
data class WorkerLoop(
    val check: String,
    @SerialName("max_attempts") val maxAttempts: Int? = null,
    /** Seconds. */
    val timeout: Long? = null,
)
