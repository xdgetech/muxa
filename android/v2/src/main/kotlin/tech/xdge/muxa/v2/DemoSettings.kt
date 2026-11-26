package tech.xdge.muxa.v2

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

/**
 * The demo's settings in memory, changed as the server changes the files: ops apply in order to a
 * copy, all or none. Secrets and env values are dropped; only that they are set is kept.
 */
internal class DemoSettings(initial: Settings) {
    var current: Settings = initial
        private set

    /** Why the update is refused: an error code and its message. */
    class Refusal(val code: String, message: String) : Exception(message)

    fun update(revision: String, ops: List<JsonObject>): Settings {
        if (current.readOnly) throw Refusal("read-only", "the host locked its settings")
        if (revision != current.revision) throw Refusal("conflict", "the settings changed; read them again")
        var next = current
        ops.forEachIndexed { i, o ->
            val name = (o["op"] as? JsonPrimitive)?.contentOrNull ?: "?"
            val op = SettingsOp.fromJson(o) ?: throw Refusal("bad-request", "ops[$i] $name: unknown op or a missing field")
            next = try {
                apply(next, op)
            } catch (e: IllegalArgumentException) {
                throw Refusal("bad-request", "ops[$i] $name: ${e.message}")
            }
        }
        current = next.copy(revision = ((current.revision.toLongOrNull() ?: 0) + 1).toString())
        return current
    }

    private fun apply(s: Settings, op: SettingsOp): Settings = when (op) {
        is SettingsOp.WorkspaceAdd -> {
            require(s.workspaces.none { it.name == op.name }) { "${op.name} exists" }
            s.copy(workspaces = s.workspaces + SettingsWorkspace(op.name, op.root, op.repos))
        }
        is SettingsOp.WorkspaceRemove -> s.copy(workspaces = s.workspaces.filter { it.name != workspace(s, op.workspace).name })
        is SettingsOp.WorkerAdd -> {
            require(s.workers.none { it.name == op.worker.name }) { "${op.worker.name} exists" }
            s.copy(workers = s.workers + op.worker)
        }
        is SettingsOp.WorkerUpdate -> {
            require(s.workers.any { it.name == op.worker.name }) { "no worker ${op.worker.name}" }
            s.copy(workers = s.workers.map { if (it.name == op.worker.name) op.worker else it })
        }
        is SettingsOp.WorkerRemove -> {
            require(s.workers.any { it.name == op.name }) { "no worker ${op.name}" }
            s.copy(workers = s.workers.filter { it.name != op.name })
        }
        is SettingsOp.AssistantUpdate -> {
            val old = s.assistant ?: SettingsAssistant(soulText = "", memoryText = "")
            s.copy(assistant = old.copy(enabled = op.enabled, worker = op.worker, soulText = op.soulText ?: old.soulText))
        }
        is SettingsOp.BuildUpdate -> {
            val old = s.build ?: SettingsBuild()
            val dir = patched(old.dir, op.dir)
            // The demo has no disks: a folder it was given is found.
            s.copy(
                build = old.copy(
                    dir = dir, dropAfter = patched(old.dropAfter, op.dropAfter),
                dirFound = dir != null, extraPaths = op.extraPaths ?: old.extraPaths,
            ),
        )
        }
        else -> {
            val ws = workspace(s, op.workspaceName ?: "")
            s.copy(workspaces = s.workspaces.map { if (it.name == ws.name) change(it, op) else it })
        }
    }

    private fun workspace(s: Settings, name: String) =
        s.workspaces.firstOrNull { it.name == name } ?: throw IllegalArgumentException("no Workspace $name")

    private fun change(w: SettingsWorkspace, op: SettingsOp): SettingsWorkspace = when (op) {
        is SettingsOp.WorkspaceUpdate -> w.copy(
            root = op.root ?: w.root,
            repos = op.repos ?: w.repos,
            hiddenRepos = op.hiddenRepos ?: w.hiddenRepos,
            parkAfter = patched(w.parkAfter, op.parkAfter),
            agent = w.agent.copy(command = patched(w.agent.command, op.agentCommand)),
            claudeConnectors = op.claudeConnectors ?: w.claudeConnectors,
            botRules = op.botRules ?: w.botRules,
        )
        is SettingsOp.EnvSet -> w.copy(
            agent = w.agent.copy(env = (w.agent.env.filter { it.key != op.key } + EnvKey(op.key, true)).sortedBy { it.key }),
        )
        is SettingsOp.EnvUnset -> {
            require(w.agent.env.any { it.key == op.key }) { "no variable ${op.key}" }
            w.copy(agent = w.agent.copy(env = w.agent.env.filter { it.key != op.key }))
        }
        is SettingsOp.SetLoopChecks -> w.copy(loop = op.checks?.let(::WorkspaceLoop))
        is SettingsOp.RoutinesGate -> w.copy(routinesEnabled = op.enabled, directWrites = op.directWrites ?: false)
        is SettingsOp.ConnectionAdd -> {
            require(w.connections.none { it.name == op.connection.name }) { "${op.connection.name} exists" }
            w.copy(connections = w.connections + connection(op.connection, if (op.secret != null) SecretState.SET else SecretState.MISSING))
        }
        is SettingsOp.ConnectionUpdate -> {
            val old = w.connections.firstOrNull { it.name == op.connection.name }
                ?: throw IllegalArgumentException("no connection ${op.connection.name}")
            val updated = connection(op.connection, if (op.secret != null) SecretState.SET else old.secret)
            w.copy(connections = w.connections.map { if (it.name == old.name) updated else it })
        }
        is SettingsOp.ConnectionSecret -> {
            require(w.connections.any { it.name == op.name }) { "no connection ${op.name}" }
            w.copy(connections = w.connections.map { if (it.name == op.name) it.copy(secret = SecretState.SET) else it })
        }
        is SettingsOp.ConnectionRemove -> {
            require(w.connections.any { it.name == op.name }) { "no connection ${op.name}" }
            w.copy(connections = w.connections.filter { it.name != op.name })
        }
        is SettingsOp.McpAdd -> {
            require(w.mcp.none { it.name == op.mcp.name }) { "${op.mcp.name} exists" }
            w.copy(mcp = w.mcp + op.mcp)
        }
        is SettingsOp.McpUpdate -> {
            require(w.mcp.any { it.name == op.mcp.name }) { "no MCP server ${op.mcp.name}" }
            w.copy(mcp = w.mcp.map { if (it.name == op.mcp.name) op.mcp else it })
        }
        is SettingsOp.McpRemove -> {
            require(w.mcp.any { it.name == op.name }) { "no MCP server ${op.name}" }
            w.copy(mcp = w.mcp.filter { it.name != op.name })
        }
        is SettingsOp.RoutineAdd -> {
            requireTiming(op.routine)
            require(w.routines.none { it.name == op.routine.name }) { "${op.routine.name} exists" }
            w.copy(routines = w.routines + SettingsRoutine.of(op.routine, op.instructionsText ?: ""))
        }
        is SettingsOp.RoutineUpdate -> {
            requireTiming(op.routine)
            val old = w.routines.firstOrNull { it.name == op.routine.name }
                ?: throw IllegalArgumentException("no Routine ${op.routine.name}")
            val updated = SettingsRoutine.of(op.routine, op.instructionsText ?: old.instructionsText)
            w.copy(routines = w.routines.map { if (it.name == old.name) updated else it })
        }
        is SettingsOp.RoutineRemove -> {
            require(w.routines.any { it.name == op.name }) { "no Routine ${op.name}" }
            w.copy(routines = w.routines.filter { it.name != op.name })
        }
        is SettingsOp.BotAdd -> {
            require(BOT_NAME.matches(op.bot.name)) { "a Bot's name is 1 to 64 lowercase letters, digits, - and _" }
            requireTeam(w, op.bot)
            require(w.bots.none { it.name == op.bot.name }) { "${op.bot.name} exists" }
            w.copy(bots = w.bots + SettingsBot.of(op.bot, op.soulText ?: ""))
        }
        is SettingsOp.BotUpdate -> {
            requireTeam(w, op.bot)
            val old = w.bots.firstOrNull { it.name == op.bot.name } ?: throw IllegalArgumentException("no Bot ${op.bot.name}")
            val updated = SettingsBot.of(op.bot, op.soulText ?: old.soulText)
                .copy(memoryText = old.memoryText, memoryVersions = old.memoryVersions, memoryHistory = old.memoryHistory)
            w.copy(bots = w.bots.map { if (it.name == old.name) updated else it })
        }
        is SettingsOp.BotRename -> {
            require(w.bots.any { it.name == op.from }) { "no Bot ${op.from}" }
            require(BOT_NAME.matches(op.to)) { "a Bot's name is 1 to 64 lowercase letters, digits, - and _" }
            require(w.bots.none { it.name == op.to }) { "${op.to} exists" }
            fun renamed(n: String) = if (n == op.from) op.to else n
            w.copy(
                bots = w.bots.map { b -> b.copy(name = renamed(b.name), members = b.members.map(::renamed)) },
                routines = w.routines.map { r -> if (r.bot == op.from) r.copy(bot = op.to) else r },
            )
        }
        is SettingsOp.BotRemove -> {
            require(w.bots.any { it.name == op.name }) { "no Bot ${op.name}" }
            require(w.routines.none { it.bot == op.name }) { "${op.name} still owns Routines; remove them first" }
            w.copy(bots = w.bots.filter { it.name != op.name })
        }
        is SettingsOp.BotMemoryRestore -> {
            val bot = w.bots.firstOrNull { it.name == op.name } ?: throw IllegalArgumentException("no Bot ${op.name}")
            require(op.version in 1..bot.memoryVersions) { "no kept memory ${op.version}" }
            w
        }
        else -> throw IllegalArgumentException("not a Workspace op")
    }

    /** As the server checks `on` (ADR 0084): only `review`, and then no time. */
    private fun requireTiming(r: RoutineSpec) {
        val on = r.on ?: return
        require(on == "review") { "on is review or absent" }
        require(r.at == null && r.schedule == null) { "a Routine with on has no at or schedule" }
    }

    /** As the server checks a team (ADR 0085): other Bots of the Workspace that lead none. */
    private fun requireTeam(w: SettingsWorkspace, bot: BotSpec) {
        require(bot.members.size <= MAX_MEMBERS) { "a team has at most $MAX_MEMBERS members" }
        for (m in bot.members) {
            require(m != bot.name) { "a Bot is not its own member" }
            val member = w.bots.firstOrNull { it.name == m } ?: throw IllegalArgumentException("no Bot $m")
            require(member.members.isEmpty()) { "$m leads a team itself" }
        }
        require(bot.handoffsPerDay == null || bot.handoffsPerDay in 1..MAX_HANDOFFS) { "handoffs_per_day is 1 to $MAX_HANDOFFS" }
        requireCommands(bot.commands)
    }

    /** As the server checks a Bot's commands (ADR 0088): the inside of a `Bash(...)` rule, one line each. */
    private fun requireCommands(commands: List<String>) {
        require(commands.size <= MAX_COMMANDS) { "a Bot has at most $MAX_COMMANDS commands" }
        for (c in commands) {
            require(c.isNotBlank() && '\n' !in c) { "a command is one line" }
            require(c.toByteArray(Charsets.UTF_8).size <= MAX_COMMAND_BYTES) { "a command is at most $MAX_COMMAND_BYTES bytes" }
            require('(' !in c && ')' !in c) { "write a command without Bash( and )" }
        }
    }

    private fun patched(old: String?, p: Patch<String>): String? = when (p) {
        Patch.Keep -> old
        Patch.Clear -> null
        is Patch.Set -> p.value
    }

    private fun connection(spec: ConnectionSpec, secret: SecretState) =
        SettingsConnection(spec.name, spec.env, spec.account, spec.access, secret)

    private companion object {
        const val MAX_MEMBERS = 6
        const val MAX_HANDOFFS = 50
        const val MAX_COMMANDS = 50
        val BOT_NAME = Regex("[a-z0-9_-]{1,64}")
        const val MAX_COMMAND_BYTES = 500
    }
}
