package tech.xdge.muxa.v2

import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.ExperimentalSerializationApi
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
    /** The Capture assistant (ADR 0084); null from an older server. */
    val assistant: SettingsAssistant? = null,
    /** The build disk (ADR 0102); null from an older server. */
    val build: SettingsBuild? = null,
)

/** `[build]` in `workspaces.toml`: where Task worktrees put their build output (ADR 0102). */
@Serializable
data class SettingsBuild(
    /** As written, `~/` included; null: the build disk is off. */
    val dir: String? = null,
    /** `3d`, `12h`; null: three days. */
    @SerialName("drop_after") val dropAfter: String? = null,
    /** [dir] is a folder on the host now. A disk that is not mounted is not. */
    @SerialName("dir_found") val dirFound: Boolean = false,
    /** Paths every Task worktree sends to the build disk too, beside the build folders muxa finds from each repo's build files. */
    @SerialName("extra_paths") val extraPaths: List<String> = emptyList(),
)

/** `[assistant]` in `workspaces.toml`, and its `soul.md` and `memory.md` beside it. A text is null when it cannot be read. */
@Serializable
data class SettingsAssistant(
    val enabled: Boolean = false,
    /** Null: the default agent. */
    val worker: String? = null,
    @SerialName("soul_text") val soulText: String? = null,
    /** Read only: the assistant writes it. */
    @SerialName("memory_text") val memoryText: String? = null,
)

/** A `[[workspace]]`. Paths are as written, `~/` included. */
@Serializable
data class SettingsWorkspace(
    val name: String,
    val root: String,
    val repos: List<String> = emptyList(),
    /** Repos the host found under the root, hidden ones too (ADR 0031). */
    @SerialName("found_repos") val foundRepos: List<String> = emptyList(),
    /** Found repos the Workspace leaves out of its repo list. */
    @SerialName("hidden_repos") val hiddenRepos: List<String> = emptyList(),
    /** `30m`, `12h`, `2d`; null: never parked (ADR 0034). */
    @SerialName("park_after") val parkAfter: String? = null,
    val agent: SettingsAgent = SettingsAgent(),
    /** Null: no Task Loops. */
    val loop: WorkspaceLoop? = null,
    @SerialName("routines_enabled") val routinesEnabled: Boolean = false,
    @SerialName("direct_writes") val directWrites: Boolean = false,
    /** The user's claude.ai connectors load beside [mcp] (ADR 0087). */
    @SerialName("claude_connectors") val claudeConnectors: Boolean = false,
    /** `[workspace.bots]`: Claude Code permission rules for the Workspace's Bots only (ADR 0087). */
    @SerialName("bot_rules") val botRules: BotRules = BotRules(),
    val connections: List<SettingsConnection> = emptyList(),
    val mcp: List<McpSpec> = emptyList(),
    /** Listed also while [routinesEnabled] is false. */
    val routines: List<SettingsRoutine> = emptyList(),
    /** The `[[bot]]` tables of `.muxa/routines.toml` (ADR 0082). */
    val bots: List<SettingsBot> = emptyList(),
    /** Why `.muxa/routines.toml` cannot be used as it is now. */
    @SerialName("routines_problem") val routinesProblem: String? = null,
)

/** Claude Code permission rules, such as `Bash` or `mcp__claude_ai_Gmail`. */
@Serializable
data class BotRules(val allow: List<String> = emptyList(), val deny: List<String> = emptyList())

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
    /** The `[[bot]]` that owns it (ADR 0082). */
    val bot: String? = null,
    /** Claude Code's edit tools are denied to the run. */
    @SerialName("read_only") val readOnly: Boolean = false,
    /** `review` or null (ADR 0084): it then has no [at] or [schedule]. */
    val on: String? = null,
)

/**
 * A `[[bot]]` (ADR 0082). [name] is lowercase letters, digits, `-` and `_`.
 * [members] make it a team lead (ADR 0085); [handoffsPerDay] null is 10.
 */
@OptIn(ExperimentalSerializationApi::class)
@Serializable
data class BotSpec(
    val name: String,
    val description: String? = null,
    val worker: String? = null,
    val enabled: Boolean = true,
    // Absent when empty, as the server writes it.
    @EncodeDefault(EncodeDefault.Mode.NEVER) val members: List<String> = emptyList(),
    @SerialName("handoffs_per_day") val handoffsPerDay: Int? = null,
    /** Shell command patterns only this Bot may run, each the inside of a `Bash(...)` rule (ADR 0088). Absent when empty. */
    @EncodeDefault(EncodeDefault.Mode.NEVER) val commands: List<String> = emptyList(),
)

/** A Bot with its `soul.md`, its `memory.md` (which its runs write), and how many earlier memories can come back. */
@Serializable
data class SettingsBot(
    val name: String,
    val description: String? = null,
    val worker: String? = null,
    val enabled: Boolean = true,
    @SerialName("soul_text") val soulText: String? = null,
    @SerialName("memory_text") val memoryText: String? = null,
    @SerialName("memory_versions") val memoryVersions: Int = 0,
    val members: List<String> = emptyList(),
    @SerialName("handoffs_per_day") val handoffsPerDay: Int? = null,
    val commands: List<String> = emptyList(),
    /** When each earlier memory was replaced, newest first; empty from servers before ADR 0086. */
    @SerialName("memory_history") val memoryHistory: List<MemoryVersion> = emptyList(),
) {
    val spec: BotSpec
        get() = BotSpec(name, description, worker, enabled, members, handoffsPerDay, commands)

    companion object {
        fun of(spec: BotSpec, soulText: String?) = with(spec) {
            SettingsBot(name, description, worker, enabled, soulText, "", 0, members, handoffsPerDay, commands)
        }
    }
}

/** An earlier memory `bot-memory-restore` can bring back: [version] 1 is the newest; [at] is Unix seconds when it was replaced. */
@Serializable
data class MemoryVersion(val version: Int, val at: Long)

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
    val bot: String? = null,
    @SerialName("read_only") val readOnly: Boolean = false,
    val on: String? = null,
) {
    val spec: RoutineSpec
        get() = RoutineSpec(name, at, schedule, timezone, worker, instructions, project, enabled, items, directWrites, bot, readOnly, on)

    companion object {
        fun of(spec: RoutineSpec, instructionsText: String?) = with(spec) {
            SettingsRoutine(
                name, at, schedule, timezone, worker, instructions, project, enabled, items, directWrites, instructionsText,
                bot, readOnly, on,
            )
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
