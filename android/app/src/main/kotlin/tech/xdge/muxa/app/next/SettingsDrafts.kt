package tech.xdge.muxa.app.next

import tech.xdge.muxa.v2.BotSpec
import tech.xdge.muxa.v2.ConnectionSpec
import tech.xdge.muxa.v2.McpSpec
import tech.xdge.muxa.v2.RoutineSpec
import tech.xdge.muxa.v2.SettingsAssistant
import tech.xdge.muxa.v2.SettingsBot
import tech.xdge.muxa.v2.SettingsConnection
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsRoutine
import tech.xdge.muxa.v2.WorkerLoop
import tech.xdge.muxa.v2.WorkerSpec

// The Settings forms' fields as text, and the op each one saves as. The host checks the rest.

/** The non-blank lines of [text], trimmed. */
fun lines(text: String): List<String> = text.lines().map { it.trim() }.filter { it.isNotEmpty() }

/** `KEY<sep>value` lines as a map; null when a line has no [sep] or no key. */
fun pairs(text: String, sep: Char): Map<String, String>? = lines(text).associate { line ->
    val at = line.indexOf(sep)
    if (at <= 0) return null
    line.substring(0, at).trim() to line.substring(at + 1).trim()
}

fun pairsText(map: Map<String, String>, sep: String): String = map.entries.joinToString("\n") { "${it.key}$sep${it.value}" }

data class WorkspaceDraft(val name: String = "", val root: String = "", val repos: String = "") {
    val isValid: Boolean get() = name.isNotBlank() && root.isNotBlank()

    fun op() = SettingsOp.WorkspaceAdd(name.trim(), root.trim(), lines(repos))
}

/** A connection's fields. [secret] is write-only: blank keeps the one in the host's Keychain. */
data class ConnectionDraft(
    val isNew: Boolean,
    val name: String = "",
    val env: String = "",
    val account: String = "",
    /** `read`, `write` or null. */
    val access: String? = null,
    val secret: String = "",
) {
    val isValid: Boolean get() = name.isNotBlank() && env.isNotBlank()

    fun op(workspace: String): SettingsOp {
        val spec = ConnectionSpec(name.trim(), env.trim(), account.trim().ifEmpty { null }, access)
        val s = secret.ifEmpty { null }
        return if (isNew) SettingsOp.ConnectionAdd(workspace, spec, s) else SettingsOp.ConnectionUpdate(workspace, spec, s)
    }

    override fun toString() = "ConnectionDraft(isNew=$isNew, name=$name, env=$env, account=$account, access=$access)"

    companion object {
        fun of(c: SettingsConnection) = ConnectionDraft(false, c.name, c.env, c.account.orEmpty(), c.access)
    }
}

/** An MCP server's fields: a command with args and env, or a url with a type and headers. */
data class McpDraft(
    val isNew: Boolean,
    val name: String = "",
    val remote: Boolean = false,
    val command: String = "",
    /** One argument per line. */
    val args: String = "",
    /** `KEY=value` per line. */
    val env: String = "",
    val url: String = "",
    /** `http` or `sse`. */
    val type: String = HTTP,
    /** `Name: value` per line. */
    val headers: String = "",
    val connections: Set<String> = emptySet(),
) {
    /** What keeps it from being saved, or null. */
    val problem: String?
        get() = when {
            name.isBlank() -> "Give it a name."
            remote && url.isBlank() -> "Give it a url."
            !remote && command.isBlank() -> "Give it a command."
            remote && pairs(headers, ':') == null -> "Write each header as Name: value."
            !remote && pairs(env, '=') == null -> "Write each variable as KEY=value."
            else -> null
        }

    fun spec(): McpSpec = if (remote) {
        McpSpec(name.trim(), url = url.trim(), type = type, headers = pairs(headers, ':').orEmpty(), connections = connections.sorted())
    } else {
        McpSpec(name.trim(), command = command.trim(), args = lines(args), env = pairs(env, '=').orEmpty(), connections = connections.sorted())
    }

    fun op(workspace: String): SettingsOp = if (isNew) SettingsOp.McpAdd(workspace, spec()) else SettingsOp.McpUpdate(workspace, spec())

    companion object {
        const val HTTP = "http"
        const val SSE = "sse"

        fun of(m: McpSpec) = McpDraft(
            false, m.name, m.url != null, m.command.orEmpty(), m.args.orEmpty().joinToString("\n"), pairsText(m.env, "="),
            m.url.orEmpty(), m.type ?: HTTP, pairsText(m.headers, ": "), m.connections.toSet(),
        )
    }
}

/** When a Routine runs: at a time, on a cron schedule, or each time a Task asks for review (ADR 0084). */
enum class RoutineTiming(private val tag: String) {
    AT("at"),
    CRON("cron"),
    REVIEW("review"),
    ;

    /** The choice's test tag ends in it. */
    override fun toString() = tag
}

/** A Routine's fields. [project] and [directWrites] are kept as the file has them. */
data class RoutineDraft(
    val isNew: Boolean,
    val name: String = "",
    val timing: RoutineTiming = RoutineTiming.AT,
    val at: String = "",
    val schedule: String = "",
    val timezone: String = "",
    val worker: String? = null,
    /** The instruction file, relative to `.muxa/`; blank: `routines/<name>.md`. */
    val instructions: String = "",
    val text: String = "",
    /** The file's content as read; null when it could not be read. A new Routine has none. */
    val originalText: String? = null,
    val enabled: Boolean = true,
    val items: String = "",
    val project: String? = null,
    val directWrites: Boolean = false,
    /** The Bot that owns it; null: none. */
    val bot: String? = null,
    val readOnly: Boolean = false,
) {
    val problem: String?
        get() = when {
            name.isBlank() -> "Give it a name."
            items.isNotBlank() && (items.trim().toIntOrNull() ?: 0) <= 0 -> "Items is a number above 0."
            else -> null
        }

    fun spec() = RoutineSpec(
        name = name.trim(),
        at = at.trim().takeIf { timing == RoutineTiming.AT && it.isNotEmpty() },
        schedule = schedule.trim().takeIf { timing == RoutineTiming.CRON && it.isNotEmpty() },
        // A Routine run on review has no time of day.
        timezone = timezone.trim().takeIf { timing != RoutineTiming.REVIEW && it.isNotEmpty() },
        worker = worker,
        instructions = instructions.trim().ifEmpty { "routines/${name.trim()}.md" },
        project = project,
        enabled = enabled,
        items = items.trim().toIntOrNull(),
        directWrites = directWrites,
        bot = bot,
        readOnly = readOnly,
        on = REVIEW.takeIf { timing == RoutineTiming.REVIEW },
    )

    /** The text goes only when it is new or changed, so an unreadable file is never emptied. */
    fun op(workspace: String): SettingsOp {
        val sent = text.takeIf { isNew || it != (originalText ?: "") }
        return if (isNew) SettingsOp.RoutineAdd(workspace, spec(), sent) else SettingsOp.RoutineUpdate(workspace, spec(), sent)
    }

    companion object {
        /** The only event a Routine runs on. */
        const val REVIEW = "review"

        fun of(r: SettingsRoutine) = RoutineDraft(
            false, r.name, timingOf(r), r.at.orEmpty(), r.schedule.orEmpty(), r.timezone.orEmpty(), r.worker,
            r.instructions, r.instructionsText.orEmpty(), r.instructionsText, r.enabled, r.items?.toString().orEmpty(),
            r.project, r.directWrites, r.bot, r.readOnly,
        )

        private fun timingOf(r: SettingsRoutine) = when {
            r.on != null -> RoutineTiming.REVIEW
            r.schedule != null -> RoutineTiming.CRON
            else -> RoutineTiming.AT
        }
    }
}

/**
 * A Bot's fields (ADR 0082). [soul] is its `soul.md`.
 * [members] make it a team lead (ADR 0085); blank [handoffs] is the host's default, 10.
 */
data class BotDraft(
    val isNew: Boolean,
    val name: String = "",
    val description: String = "",
    val worker: String? = null,
    val enabled: Boolean = true,
    val soul: String = "",
    /** The Soul as read; null when it could not be read. A new Bot has none. */
    val originalSoul: String? = null,
    val members: Set<String> = emptySet(),
    val handoffs: String = "",
    /** Shell commands only this Bot may run (ADR 0088), as the inside of `Bash(...)`. */
    val commands: List<String> = emptyList(),
) {
    val problem: String?
        get() = when {
            name.isBlank() -> "Give it a name."
            !isValidName(name.trim()) -> NAME_RULE
            members.isNotEmpty() && handoffs.isNotBlank() && handoffs.trim().toIntOrNull() !in PER_DAY ->
                "Handoffs a day is a number from 1 to 50."
            members.size > MAX_MEMBERS -> "A team has at most $MAX_MEMBERS members."
            commands.size > MAX_COMMANDS -> "A Bot has at most $MAX_COMMANDS commands."
            commands.any { commandProblem(it) != null } -> commands.firstNotNullOf { commandProblem(it) }
            soul.toByteArray(Charsets.UTF_8).size > SOUL_BYTES -> "The Soul is at most 12 KiB."
            else -> null
        }

    fun spec() = BotSpec(
        name.trim(), description.trim().ifEmpty { null }, worker, enabled,
        // Handoffs mean nothing without a team.
        members.sorted(), handoffs.trim().toIntOrNull().takeIf { members.isNotEmpty() }, commands,
    )

    /** As a Routine's text: the Soul goes only when it is new or changed, so an unreadable file is never emptied. */
    fun op(workspace: String): SettingsOp {
        val sent = soul.takeIf { isNew || it != (originalSoul ?: "") }
        return if (isNew) SettingsOp.BotAdd(workspace, spec(), sent) else SettingsOp.BotUpdate(workspace, spec(), sent)
    }

    companion object {
        const val NAME_RULE = "A name has only lowercase letters, digits, - and _."
        private val BOT_NAME = Regex("[a-z0-9_-]{1,64}")
        private val PER_DAY = 1..50
        private const val MAX_MEMBERS = 6
        private const val MAX_COMMANDS = 50
        private const val MAX_COMMAND_BYTES = 500
        private const val SOUL_BYTES = 12 * 1024

        fun isValidName(name: String) = BOT_NAME.matches(name)

        /** A name as typed for a rename: lowercase, with spaces as `-`. */
        fun typedName(text: String): String = text.lowercase().replace(' ', '-')

        /** Whether [from] can be renamed to [to] among the Workspace's [names]. */
        fun canRename(from: String, to: String, names: List<String>): Boolean =
            to != from && isValidName(to) && to !in names

        /** Why [line] cannot be one of a Bot's commands, or null. The host writes it as `Bash(<line>)`. */
        fun commandProblem(line: String): String? = when {
            line.isBlank() -> "Write a command."
            '\n' in line -> "A command is one line."
            line.toByteArray(Charsets.UTF_8).size > MAX_COMMAND_BYTES -> "A command is at most $MAX_COMMAND_BYTES bytes."
            '(' in line || ')' in line -> "Write the command without Bash( and ), and without parentheses."
            else -> null
        }

        /**
         * The Bots of [bots] that [bot] may lead: the others that lead no team. Its current members stay,
         * so a choice the host would now refuse still shows. Empty when [bot] is itself a member of a lead.
         */
        fun memberChoices(bot: String, bots: List<SettingsBot>): List<String> {
            if (bots.any { it.name != bot && bot in it.members }) return emptyList()
            val current = bots.firstOrNull { it.name == bot }?.members.orEmpty()
            return bots.filter { it.name != bot && (it.members.isEmpty() || it.name in current) }.map { it.name }
        }

        fun of(b: SettingsBot) = BotDraft(
            false, b.name, b.description.orEmpty(), b.worker, b.enabled,
            b.soulText.orEmpty(), b.soulText, b.members.toSet(), b.handoffsPerDay?.toString().orEmpty(), b.commands,
        )
    }
}

/** The Capture assistant's fields (ADR 0084). As a Bot's, the Soul goes only when it changed. */
data class AssistantDraft(
    val enabled: Boolean = false,
    val worker: String? = null,
    val soul: String = "",
    /** The Soul as read; null when it could not be read. */
    val originalSoul: String? = null,
) {
    fun op() = SettingsOp.AssistantUpdate(enabled, worker, soul.takeIf { it != (originalSoul ?: "") })

    companion object {
        fun of(a: SettingsAssistant) = AssistantDraft(a.enabled, a.worker, a.soulText.orEmpty(), a.soulText)
    }
}

/** A worker's fields. An empty [workspaces] means every Workspace. A blank [check] means no loop. */
data class WorkerDraft(
    val isNew: Boolean,
    val name: String = "",
    val description: String = "",
    val agent: String = CLAUDE_CODE,
    /** One argument per line. */
    val args: String = "",
    val instructions: String = "",
    val workspaces: Set<String> = emptySet(),
    val check: String = "",
    val loop: WorkerLoop? = null,
) {
    val isValid: Boolean get() = name.isNotBlank()

    fun spec() = WorkerSpec(
        name = name.trim(),
        description = description.trim().ifEmpty { null },
        agent = agent,
        args = lines(args),
        instructions = instructions.trim().ifEmpty { null },
        workspaces = workspaces.sorted().ifEmpty { null },
        loop = check.trim().ifEmpty { null }?.let { c -> loop?.copy(check = c) ?: WorkerLoop(c) },
    )

    fun op(): SettingsOp = if (isNew) SettingsOp.WorkerAdd(spec()) else SettingsOp.WorkerUpdate(spec())

    companion object {
        const val CLAUDE_CODE = "claude-code"
        const val CODEX = "codex"

        fun of(w: WorkerSpec) = WorkerDraft(
            false, w.name, w.description.orEmpty(), w.agent, w.args.joinToString("\n"), w.instructions.orEmpty(),
            w.workspaces.orEmpty().toSet(), w.loop?.check.orEmpty(), w.loop,
        )
    }
}
