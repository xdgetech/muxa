package tech.xdge.muxa.v2

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.put

/** A field of an update: kept as it is (absent on the wire), cleared (`null`), or set. */
sealed interface Patch<out T> {
    data object Keep : Patch<Nothing>
    data object Clear : Patch<Nothing>
    data class Set<T>(val value: T) : Patch<T>
}

private const val REDACTED = "<write-only>"

/**
 * One change of `settings-update` (ADR 0074). Names are never renamed: remove and add instead.
 * `secret` and an env `value` go only to the server; toString never shows them.
 */
sealed class SettingsOp(val op: String) {
    data class WorkspaceAdd(val name: String, val root: String, val repos: List<String> = emptyList()) : SettingsOp("workspace-add")

    /** Null [root] or [repos] keeps them. */
    data class WorkspaceUpdate(
        val workspace: String,
        val root: String? = null,
        val repos: List<String>? = null,
        /** Cleared: the daemon's `--task-command`. */
        val agentCommand: Patch<String> = Patch.Keep,
        /** Cleared: never parked. */
        val parkAfter: Patch<String> = Patch.Keep,
        /** Found repos to leave out of the Workspace's list; null keeps them. */
        val hiddenRepos: List<String>? = null,
        /** Whether claude.ai connectors load beside its MCP servers (ADR 0087); null keeps it. */
        val claudeConnectors: Boolean? = null,
        /** Replaces both lists of `[workspace.bots]`; null keeps them. */
        val botRules: BotRules? = null,
        /** The subscriptions its agents use, in order (ADR 0091); null keeps them. */
        val subscriptions: List<String>? = null,
    ) : SettingsOp("workspace-update")

    /** Also removes the Keychain secrets of its connections; its folder and `.muxa/` stay. */
    data class WorkspaceRemove(val workspace: String) : SettingsOp("workspace-remove")

    data class EnvSet(val workspace: String, val key: String, val value: String) : SettingsOp("env-set") {
        override fun toString() = "EnvSet(workspace=$workspace, key=$key, value=$REDACTED)"
    }

    data class EnvUnset(val workspace: String, val key: String) : SettingsOp("env-unset")

    /** Null [checks]: no Task Loops. */
    data class SetLoopChecks(val workspace: String, val checks: LoopChecks?) : SettingsOp("loop-checks")

    /** Null [directWrites]: the server's default, false. */
    data class RoutinesGate(val workspace: String, val enabled: Boolean, val directWrites: Boolean? = null) : SettingsOp("routines-gate")

    data class ConnectionAdd(val workspace: String, val connection: ConnectionSpec, val secret: String? = null) : SettingsOp("connection-add") {
        override fun toString() = "ConnectionAdd(workspace=$workspace, connection=$connection, secret=${secret?.let { REDACTED }})"
    }

    /** Changes the connection named `connection.name`; a [secret] replaces its Keychain secret. */
    data class ConnectionUpdate(val workspace: String, val connection: ConnectionSpec, val secret: String? = null) :
        SettingsOp("connection-update") {
        override fun toString() = "ConnectionUpdate(workspace=$workspace, connection=$connection, secret=${secret?.let { REDACTED }})"
    }

    data class ConnectionSecret(val workspace: String, val name: String, val secret: String) : SettingsOp("connection-secret") {
        override fun toString() = "ConnectionSecret(workspace=$workspace, name=$name, secret=$REDACTED)"
    }

    data class ConnectionRemove(val workspace: String, val name: String) : SettingsOp("connection-remove")
    data class McpAdd(val workspace: String, val mcp: McpSpec) : SettingsOp("mcp-add")

    /** Replaces the server named `mcp.name`. */
    data class McpUpdate(val workspace: String, val mcp: McpSpec) : SettingsOp("mcp-update")
    data class McpRemove(val workspace: String, val name: String) : SettingsOp("mcp-remove")

    /** [instructionsText] replaces the instruction file's content. */
    data class RoutineAdd(val workspace: String, val routine: RoutineSpec, val instructionsText: String? = null) : SettingsOp("routine-add")

    /** Changes the Routine named `routine.name`. */
    data class RoutineUpdate(val workspace: String, val routine: RoutineSpec, val instructionsText: String? = null) :
        SettingsOp("routine-update")

    /** Its instruction file stays. */
    data class RoutineRemove(val workspace: String, val name: String) : SettingsOp("routine-remove")

    /** [soulText] replaces the Bot's `soul.md` (ADR 0082). */
    data class BotAdd(val workspace: String, val bot: BotSpec, val soulText: String? = null) : SettingsOp("bot-add")
    data class BotUpdate(val workspace: String, val bot: BotSpec, val soulText: String? = null) : SettingsOp("bot-update")

    /** Renames the Bot with its Routines, teams, folder and Inbox items (ADR 0089). */
    data class BotRename(val workspace: String, val from: String, val to: String) : SettingsOp("bot-rename")

    /** Its Routines must go first; its folder stays. */
    data class BotRemove(val workspace: String, val name: String) : SettingsOp("bot-remove")

    /** Brings back kept memory [version], 1 the newest. */
    data class BotMemoryRestore(val workspace: String, val name: String, val version: Int) : SettingsOp("bot-memory-restore")

    /** The Capture assistant (ADR 0084). Null [worker]: the default agent. [soulText] replaces its `soul.md`. */
    data class AssistantUpdate(
        val enabled: Boolean,
        val worker: String? = null,
        val soulText: String? = null,
        /** Null keeps the list (ADR 0091). */
        val subscriptions: List<String>? = null,
    ) : SettingsOp("assistant-update")

    /** A named agent login (ADR 0091). The login itself stays with the agent. */
    data class SubscriptionAdd(val subscription: SubscriptionSpec) : SettingsOp("subscription-add")

    /** Replaces the subscription named `subscription.name`. */
    data class SubscriptionUpdate(val subscription: SubscriptionSpec) : SettingsOp("subscription-update")

    /** Refused while a Workspace lists it. */
    data class SubscriptionRemove(val name: String) : SettingsOp("subscription-remove")
    data class WorkerAdd(val worker: WorkerSpec) : SettingsOp("worker-add")

    /** Replaces the worker named `worker.name`. */
    data class WorkerUpdate(val worker: WorkerSpec) : SettingsOp("worker-update")
    data class WorkerRemove(val name: String) : SettingsOp("worker-remove")

    /** The Workspace this op changes; null for a worker or assistant op. */
    val workspaceName: String?
        get() = when (this) {
            is WorkspaceAdd -> name
            is WorkspaceUpdate -> workspace
            is WorkspaceRemove -> workspace
            is EnvSet -> workspace
            is EnvUnset -> workspace
            is SetLoopChecks -> workspace
            is RoutinesGate -> workspace
            is ConnectionAdd -> workspace
            is ConnectionUpdate -> workspace
            is ConnectionSecret -> workspace
            is ConnectionRemove -> workspace
            is McpAdd -> workspace
            is McpUpdate -> workspace
            is McpRemove -> workspace
            is RoutineAdd -> workspace
            is RoutineUpdate -> workspace
            is RoutineRemove -> workspace
            is BotAdd -> workspace
            is BotUpdate -> workspace
            is BotRemove -> workspace
            is BotRename -> workspace
            is BotMemoryRestore -> workspace
            is AssistantUpdate, is WorkerAdd, is WorkerUpdate, is WorkerRemove,
            is SubscriptionAdd, is SubscriptionUpdate, is SubscriptionRemove -> null
        }

    fun json(): JsonObject = buildJsonObject {
        put("op", op)
        when (val o = this@SettingsOp) {
            is WorkspaceAdd -> { put("name", o.name); put("root", o.root); put("repos", enc(o.repos)) }
            is WorkspaceUpdate -> {
                put("workspace", o.workspace)
                o.root?.let { put("root", it) }
                o.repos?.let { put("repos", enc(it)) }
                o.hiddenRepos?.let { put("hidden_repos", enc(it)) }
                patch("agent_command", o.agentCommand)
                patch("park_after", o.parkAfter)
                o.claudeConnectors?.let { put("claude_connectors", it) }
                o.botRules?.let { put("bot_rules", enc(it)) }
                o.subscriptions?.let { put("subscriptions", enc(it)) }
            }
            is WorkspaceRemove -> put("workspace", o.workspace)
            is EnvSet -> { put("workspace", o.workspace); put("key", o.key); put("value", o.value) }
            is EnvUnset -> { put("workspace", o.workspace); put("key", o.key) }
            is SetLoopChecks -> { put("workspace", o.workspace); put("checks", o.checks?.let(::enc) ?: JsonNull) }
            is RoutinesGate -> {
                put("workspace", o.workspace); put("enabled", o.enabled)
                o.directWrites?.let { put("direct_writes", it) }
            }
            is ConnectionAdd -> connection(o.workspace, o.connection, o.secret)
            is ConnectionUpdate -> connection(o.workspace, o.connection, o.secret)
            is ConnectionSecret -> { put("workspace", o.workspace); put("name", o.name); put("secret", o.secret) }
            is ConnectionRemove -> { put("workspace", o.workspace); put("name", o.name) }
            is McpAdd -> { put("workspace", o.workspace); put("mcp", enc(o.mcp)) }
            is McpUpdate -> { put("workspace", o.workspace); put("mcp", enc(o.mcp)) }
            is McpRemove -> { put("workspace", o.workspace); put("name", o.name) }
            is RoutineAdd -> routine(o.workspace, o.routine, o.instructionsText)
            is RoutineUpdate -> routine(o.workspace, o.routine, o.instructionsText)
            is RoutineRemove -> { put("workspace", o.workspace); put("name", o.name) }
            is BotAdd -> bot(o.workspace, o.bot, o.soulText)
            is BotUpdate -> bot(o.workspace, o.bot, o.soulText)
            is BotRemove -> { put("workspace", o.workspace); put("name", o.name) }
            is BotRename -> { put("workspace", o.workspace); put("from", o.from); put("to", o.to) }
            is BotMemoryRestore -> { put("workspace", o.workspace); put("name", o.name); put("version", o.version) }
            is AssistantUpdate -> {
                put("enabled", o.enabled)
                o.worker?.let { put("worker", it) }
                o.soulText?.let { put("soul_text", it) }
                o.subscriptions?.let { put("subscriptions", enc(it)) }
            }
            is SubscriptionAdd -> put("subscription", enc(o.subscription))
            is SubscriptionUpdate -> put("subscription", enc(o.subscription))
            is SubscriptionRemove -> put("name", o.name)
            is WorkerAdd -> put("worker", enc(o.worker))
            is WorkerUpdate -> put("worker", enc(o.worker))
            is WorkerRemove -> put("name", o.name)
        }
    }

    companion object {
        /** Reads an op as the server does; null for an unknown op or a missing field. */
        internal fun fromJson(o: JsonObject): SettingsOp? = runCatching { decode(o) }.getOrNull()

        private fun decode(o: JsonObject): SettingsOp? {
            fun s(k: String) = (o[k] as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull
            fun req(k: String) = s(k) ?: throw IllegalArgumentException("no $k")
            fun bool(k: String) = (o[k] as? JsonPrimitive)?.booleanOrNull
            fun strings(k: String) = (o[k] as? JsonArray)?.let { a -> a.map { (it as JsonPrimitive).content } }
            fun patch(k: String): Patch<String> = when (val e = o[k]) {
                null -> Patch.Keep
                JsonNull -> Patch.Clear
                else -> Patch.Set((e as JsonPrimitive).content)
            }
            fun <T> obj(k: String, f: (JsonElement) -> T): T = f(o[k] ?: throw IllegalArgumentException("no $k"))
            val ws = { req("workspace") }
            return when (s("op")) {
                "workspace-add" -> WorkspaceAdd(req("name"), req("root"), strings("repos").orEmpty())
                "workspace-update" -> WorkspaceUpdate(
                    ws(), s("root"), strings("repos"), patch("agent_command"), patch("park_after"), strings("hidden_repos"),
                    bool("claude_connectors"), o["bot_rules"]?.let { opsJson.decodeFromJsonElement<BotRules>(it) },
                    strings("subscriptions"),
                )
                "workspace-remove" -> WorkspaceRemove(ws())
                "env-set" -> EnvSet(ws(), req("key"), req("value"))
                "env-unset" -> EnvUnset(ws(), req("key"))
                "loop-checks" -> SetLoopChecks(ws(), o["checks"]?.takeIf { it != JsonNull }?.let { opsJson.decodeFromJsonElement<LoopChecks>(it) })
                "routines-gate" -> RoutinesGate(ws(), bool("enabled") ?: throw IllegalArgumentException("no enabled"), bool("direct_writes"))
                "connection-add" -> ConnectionAdd(ws(), obj("connection") { opsJson.decodeFromJsonElement(it) }, s("secret"))
                "connection-update" -> ConnectionUpdate(ws(), obj("connection") { opsJson.decodeFromJsonElement(it) }, s("secret"))
                "connection-secret" -> ConnectionSecret(ws(), req("name"), req("secret"))
                "connection-remove" -> ConnectionRemove(ws(), req("name"))
                "mcp-add" -> McpAdd(ws(), obj("mcp") { opsJson.decodeFromJsonElement(it) })
                "mcp-update" -> McpUpdate(ws(), obj("mcp") { opsJson.decodeFromJsonElement(it) })
                "mcp-remove" -> McpRemove(ws(), req("name"))
                "routine-add" -> RoutineAdd(ws(), obj("routine") { opsJson.decodeFromJsonElement(it) }, s("instructions_text"))
                "routine-update" -> RoutineUpdate(ws(), obj("routine") { opsJson.decodeFromJsonElement(it) }, s("instructions_text"))
                "routine-remove" -> RoutineRemove(ws(), req("name"))
                "bot-add" -> BotAdd(ws(), obj("bot") { opsJson.decodeFromJsonElement(it) }, s("soul_text"))
                "bot-update" -> BotUpdate(ws(), obj("bot") { opsJson.decodeFromJsonElement(it) }, s("soul_text"))
                "bot-remove" -> BotRemove(ws(), req("name"))
                "bot-rename" -> BotRename(ws(), req("from"), req("to"))
                "bot-memory-restore" -> BotMemoryRestore(
                    ws(), req("name"), (o["version"] as? JsonPrimitive)?.intOrNull ?: throw IllegalArgumentException("no version"),
                )
                "assistant-update" -> AssistantUpdate(
                    bool("enabled") ?: throw IllegalArgumentException("no enabled"), s("worker"), s("soul_text"),
                    strings("subscriptions"),
                )
                "subscription-add" -> SubscriptionAdd(obj("subscription") { opsJson.decodeFromJsonElement(it) })
                "subscription-update" -> SubscriptionUpdate(obj("subscription") { opsJson.decodeFromJsonElement(it) })
                "subscription-remove" -> SubscriptionRemove(req("name"))
                "worker-add" -> WorkerAdd(obj("worker") { opsJson.decodeFromJsonElement(it) })
                "worker-update" -> WorkerUpdate(obj("worker") { opsJson.decodeFromJsonElement(it) })
                "worker-remove" -> WorkerRemove(req("name"))
                else -> null
            }
        }
    }
}

/** Ops as JSON: one this client does not know is left out when read. */
internal object SettingsOpListSerializer : KSerializer<List<SettingsOp>> {
    override val descriptor: SerialDescriptor = JsonElement.serializer().descriptor

    override fun serialize(encoder: Encoder, value: List<SettingsOp>) {
        val out = encoder as? JsonEncoder ?: throw SerializationException("settings ops are JSON only")
        out.encodeJsonElement(JsonArray(value.map { it.json() }))
    }

    override fun deserialize(decoder: Decoder): List<SettingsOp> {
        val input = decoder as? JsonDecoder ?: throw SerializationException("settings ops are JSON only")
        val list = input.decodeJsonElement() as? JsonArray ?: throw SerializationException("ops are a list")
        return list.mapNotNull { (it as? JsonObject)?.let(SettingsOp::fromJson) }
    }
}

/** Ops carry every field the server reads, defaults included; a null optional stays absent. */
internal val opsJson = Json(json) { encodeDefaults = true }

private inline fun <reified T> enc(value: T): JsonElement = opsJson.encodeToJsonElement(value)

private fun JsonObjectBuilder.patch(key: String, p: Patch<String>) {
    when (p) {
        Patch.Keep -> {}
        Patch.Clear -> put(key, JsonNull)
        is Patch.Set -> put(key, p.value)
    }
}

private fun JsonObjectBuilder.connection(workspace: String, spec: ConnectionSpec, secret: String?) {
    put("workspace", workspace); put("connection", enc(spec))
    secret?.let { put("secret", it) }
}

private fun JsonObjectBuilder.bot(workspace: String, spec: BotSpec, soulText: String?) {
    put("workspace", workspace); put("bot", enc(spec))
    soulText?.let { put("soul_text", it) }
}

private fun JsonObjectBuilder.routine(workspace: String, spec: RoutineSpec, instructionsText: String?) {
    put("workspace", workspace); put("routine", enc(spec))
    instructionsText?.let { put("instructions_text", it) }
}
