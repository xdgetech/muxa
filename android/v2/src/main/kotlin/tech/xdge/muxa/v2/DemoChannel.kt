package tech.xdge.muxa.v2

import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import tech.xdge.muxa.transport.ByteChannel
import java.time.LocalDate
import java.time.format.DateTimeParseException

/**
 * Protocol v2 served in memory with fixed content, for Try the Demo, the UI
 * tests and client tests. It answers like the muxa server; a terminal echoes
 * what is typed. Capture reads only the kind, a known `#Workspace` and
 * `due today|tomorrow|YYYY-MM-DD`.
 */
class DemoChannel(content: DemoContent = DemoContent.STANDARD) : ByteChannel {
    private val out = Channel<ByteArray>(Channel.UNLIMITED)
    private val lock = Mutex()
    private val server = DemoServer(content) { out.trySend(it.encode()) }
    private val reader = FrameReader()

    override val bytes: ReceiveChannel<ByteArray> = out

    override suspend fun send(data: ByteArray) = lock.withLock {
        try {
            reader.push(data).forEach(server::handle)
        } catch (e: FrameError) {
            out.close(e)
        }
        Unit
    }

    override suspend fun close() {
        out.close()
    }
}

/** What the demo shows. [screens] holds each terminal's screen, by id. */
data class DemoContent(
    val terminals: List<Terminal>,
    val screens: Map<String, String>,
    val inbox: List<InboxItem>,
    val routines: List<Routine>,
    /** Capture knows their names as `#Workspace`. */
    val workspaces: List<Workspace>,
    /** What the Reader shows in every Task's folder, by kind. */
    val files: Map<ReaderKind, List<DemoFile>> = emptyMap(),
    /** Each Task agent's conversation, by terminal id. */
    val chats: Map<String, ChatUpdate> = emptyMap(),
    /** Review items whose Approve without force is refused with `would-lose-work`, and the message, by item id. */
    val losesWork: Map<String, String> = emptyMap(),
    /** What `settings` answers first; `settings-update` changes a copy in memory. */
    val settings: Settings = Settings("1"),
) {
    companion object {
        /** Two Tasks (one waits on the user), a shell, every Inbox kind, a v1 item to resume (`d9`), and three Routines. Approving `d8` needs force. */
        val STANDARD: DemoContent = standardContent()
    }
}

/** A Reader file of the demo: its list entry and its content. */
data class DemoFile(val entry: ReaderEntry, val content: ReaderContent)

private class DemoServer(content: DemoContent, private val write: (Frame) -> Unit) {
    private val terminals = content.terminals.toMutableList()
    private val screens = content.screens.toMutableMap()
    private val routines = content.routines.toMutableList()
    private val workspaces = content.workspaces
    private val files = content.files
    private var nextReader = 1
    private val loops = sortedMapOf<String, LoopInfo>()
    private val chats = content.chats
    private val losesWork = content.losesWork
    private var nextChat = 1
    private val settings = DemoSettings(content.settings)
    private var inbox = Inbox(1, content.inbox, emptyList())
    private var nextItem = 100
    private val channels = mutableMapOf<Int, String>()
    private val offsets = mutableMapOf<Int, Long>()
    private val applied = mutableMapOf<Int, Long>()
    private var nextChannel = 1
    private var watching = false

    fun handle(f: Frame) {
        val n = f.channel
        when {
            n == 0 && f.kind == FrameKind.DATA -> {
                val msg = runCatching { json.parseToJsonElement(f.payload.toString(Charsets.UTF_8)).jsonObject }.getOrNull()
                msg?.let(::control)
            }
            f.kind == FrameKind.DATA -> input(n, f)
            f.kind == FrameKind.CLOSE -> channels.remove(n)
            else -> {}
        }
    }

    private fun input(n: Int, f: Frame) {
        val (seq, bytes) = f.splitU64() ?: return
        val id = channels[n] ?: return
        if (seq > (applied[n] ?: 0)) {
            applied[n] = seq
            // The demo's program echoes; Return starts a new line.
            show(id, bytes.flatMap { if (it == CR) listOf(CR, LF) else listOf(it) }.toByteArray())
        }
        write(Frame.ack(n, seq))
    }

    /** Adds [bytes] to the terminal's screen and to every channel attached to it. */
    private fun show(terminal: String, bytes: ByteArray) {
        screens[terminal] = (screens[terminal] ?: "") + bytes.toString(Charsets.UTF_8)
        channels.filterValues { it == terminal }.keys.sorted().forEach { n ->
            val offset = offsets[n] ?: 0
            write(Frame.withU64(n, offset, bytes))
            offsets[n] = offset + bytes.size
        }
    }

    private fun control(m: JsonObject) {
        val id = m.long("id") ?: 0
        val item = m.str("item") ?: ""
        when (m.str("type")) {
            "hello" -> {
                if (m.long("version") != 2L) return send(error(null, "version", "this server speaks protocol 2"))
                send(buildJsonObject {
                    put("type", "welcome"); put("version", 2); put("server", "muxa demo"); put("backend", "demo")
                    put("token", "demo"); put("resumed", buildJsonArray {})
                })
            }
            "terminals" -> reply(id, "terminals" to enc(terminals.filter { it.live }))
            "attach" -> attach(id, m.str("terminal") ?: "")
            "detach" -> {
                val n = m.long("channel")?.toInt() ?: 0
                channels.remove(n)
                write(Frame(n, FrameKind.CLOSE, ByteArray(0)))
                reply(id)
            }
            "start-task" -> startTask(id, m)
            "watch" -> {
                watching = true
                reply(id)
            }
            "inbox" -> reply(id, "inbox" to enc(inbox))
            "parse-capture" -> reply(id, "capture" to enc(capture(m.str("text") ?: "", m.str("today") ?: "")))
            "inbox-add" -> {
                val kind = ItemKind.entries.firstOrNull { it.wire == m.str("kind") } ?: ItemKind.TODO
                val new = InboxItem(
                    newId(), kind, m.str("title") ?: "", note = m.str("note"), workspace = m.str("workspace"),
                    due = m.str("due"), created = now(), source = "capture",
                )
                change(id) { it.copy(items = listOf(new) + it.items) }
            }
            "inbox-move" -> {
                val index = m.long("index")?.toInt() ?: 0
                change(id) { i ->
                    val moved = i.items.firstOrNull { it.id == item } ?: return@change null
                    val rest = i.items - moved
                    i.copy(items = rest.toMutableList().apply { add(index.coerceIn(0, rest.size), moved) })
                }
            }
            "inbox-later" -> change(id) { i ->
                if (i.items.none { it.id == item }) return@change null
                i.copy(items = i.items.map { if (it.id == item) it.copy(laterUntil = m.long("until")) else it })
            }
            "inbox-done" -> change(id) { i ->
                val done = i.items.firstOrNull { it.id == item } ?: return@change null
                i.copy(items = i.items - done, done = listOf(done) + i.done)
            }
            "inbox-reopen" -> change(id) { i ->
                val open = i.done.firstOrNull { it.id == item } ?: return@change null
                i.copy(items = listOf(open) + i.items, done = i.done - open)
            }
            "inbox-delete" -> change(id) { i ->
                if (i.items.none { it.id == item } && i.done.none { it.id == item }) return@change null
                i.copy(items = i.items.filter { it.id != item }, done = i.done.filter { it.id != item })
            }
            "task-review" -> {
                val t = terminals.firstOrNull { it.id == m.str("terminal") }
                val review = InboxItem(
                    newId(), ItemKind.REVIEW, "Review ${t?.branch ?: "the Task"}", note = m.str("note"),
                    created = now(), source = "task",
                )
                change(id) { it.copy(items = listOf(review) + it.items) }
            }
            "inbox-approve" -> approve(id, item, force = m.str("worktree") == "force")
            "inbox-request-changes" -> requestChanges(id, item, m.str("comment") ?: "")
            "inbox-retry" -> retry(id, item)
            "workspaces" -> reply(id, "workspaces" to enc(workspaces.map(::withTasks)))
            "routines" -> reply(id, "routines" to enc(routines), "problems" to buildJsonArray {})
            "run-routine" -> if (startRoutine(id, routines.indexOfFirst { it.name == m.str("name") })) reply(id)
            "reader-list" -> {
                val kind = readerKind(id, m) ?: return
                reply(id, "list" to enc(ReaderList(root = "app", entries = files[kind].orEmpty().map { it.entry })))
            }
            "reader-open" -> {
                val kind = readerKind(id, m) ?: return
                val path = m.str("path")
                val file = files[kind].orEmpty().firstOrNull { it.entry.path == path }
                    ?: return send(error(id, "bad-request", "$path is not a listed file"))
                val n = nextReader++
                reply(id, "reader" to JsonPrimitive(n), "content" to enc(file.content))
            }
            "reader-close" -> reply(id)
            "chat-open" -> {
                val terminal = m.str("terminal") ?: ""
                if (terminals.none { it.id == terminal && it.isTaskAgent }) {
                    return send(error(id, "bad-request", "$terminal is not a Task's agent"))
                }
                val update = chats[terminal] ?: ChatUpdate(reset = true, entries = emptyList())
                reply(id, "chat" to JsonPrimitive(nextChat++), "update" to enc(update))
            }
            "chat-close" -> reply(id)
            "loops" -> reply(id, "loops" to enc(loops.values.toList()))
            "loop-start" -> {
                val terminal = m.str("terminal") ?: ""
                if (terminals.none { it.id == terminal && it.isTaskAgent }) {
                    return send(error(id, "bad-request", "$terminal is not a Task's agent"))
                }
                val spec = (m["loop"] as? JsonObject)?.let { json.decodeFromJsonElement<LoopSpec>(it) } ?: LoopSpec()
                val previous = if ((m["reset"] as? JsonPrimitive)?.booleanOrNull == true) null else loops[terminal]
                reply(id, "loop" to enc(armLoop(terminal, spec, previous)))
            }
            "loop-stop" -> {
                val terminal = m.str("terminal") ?: ""
                val lp = loops[terminal] ?: return send(error(id, "bad-request", "$terminal has no Task Loop"))
                val stopped = if (lp.isActive) lp.copy(state = "stopped", reason = "user", since = now()) else lp
                setLoop(stopped)
                reply(id, "loop" to enc(stopped))
            }
            "settings" -> reply(id, "settings" to enc(settings.current))
            "settings-update" -> try {
                val ops = (m["ops"] as? JsonArray).orEmpty().map { it as? JsonObject ?: JsonObject(emptyMap()) }
                reply(id, "settings" to enc(settings.update(m.str("revision") ?: "", ops)))
            } catch (e: DemoSettings.Refusal) {
                send(error(id, e.code, e.message ?: ""))
            }
            else -> send(error(id, "bad-request", "unknown request"))
        }
    }

    private fun setLoop(lp: LoopInfo) {
        loops[lp.terminal] = lp
        if (watching) send(buildJsonObject { put("type", "loops"); put("loops", enc(loops.values.toList())) })
    }

    /** The kind of a Reader request on a terminal with a folder, or null after an error. */
    private fun readerKind(id: Long, m: JsonObject): ReaderKind? {
        if (terminals.firstOrNull { it.id == m.str("terminal") }?.taskDir == null) {
            send(error(id, "bad-request", "this terminal has no folder: only a Task's agent or a Routine run has a Reader"))
            return null
        }
        return ReaderKind.entries.firstOrNull { it.wire == m.str("kind") }
    }

    private fun attach(id: Long, terminal: String) {
        if (terminals.none { it.id == terminal }) return send(error(id, "not-found", "no terminal $terminal"))
        val n = nextChannel++
        channels[n] = terminal
        reply(id, "channel" to JsonPrimitive(n))
        send(buildJsonObject { put("type", "paint"); put("channel", n) })
        val screen = (screens[terminal] ?: "").toByteArray(Charsets.UTF_8)
        write(Frame.withU64(n, 0, screen))
        offsets[n] = screen.size.toLong()
    }

    /** The first branch is the working place; without one, a folder named after the title. A `loop` starts its Task Loop. */
    private fun startTask(id: Long, m: JsonObject) {
        val title = m.str("title") ?: "Task"
        val places = (m["places"] as? JsonArray).orEmpty()
        val loop = m["loop"] as? JsonObject
        val wsName = m.str("workspace") ?: "app"
        val workspace = workspaces.firstOrNull { it.name == wsName }
        // Every agent gets all the Workspace's connections and servers (ADR 0075);
        // `connections` and `mcp` in the request are ignored.
        val named = workspace?.connections.orEmpty().map { it.name }
        val servers = workspace?.mcp.orEmpty().map { it.name }
        val specs = places.mapNotNull { it as? JsonObject }
        val first = specs.firstOrNull { it.str("kind") == "branch" }
        val branch = first?.str("branch")
        val repo = first?.str("repo")
        // An `existing` place is adopted as it is: it is the Task's folder.
        val adopted = specs.firstOrNull { it.str("kind") == "existing" }?.str("path")
        val dir = adopted ?: if (branch != null) "${repo ?: "/demo"}-$branch" else "/demo/tasks/$title"
        terminals.firstOrNull { it.live && it.isTaskAgent && it.taskDir == dir }?.let {
            return reply(id, "terminal" to enc(it), "existing" to JsonPrimitive(true))
        }
        val labels = buildMap {
            put("muxa.kind", "task"); put("muxa.role", "agent"); put("muxa.dir", dir); put("muxa.workspace", wsName)
            if (branch != null) put("muxa.branch", branch)
            if (repo != null) put("muxa.repo", repo)
            if (named.isNotEmpty()) put("muxa.connections", enc(named).toString())
            if (servers.isNotEmpty()) put("muxa.mcp", enc(servers).toString())
        }
        val t = Terminal(
            "t${terminals.size + 1}", title, live = true, status = TerminalStatus("working", "claude-code"),
            labels = labels, cols = 80, rows = 24,
        )
        terminals += t
        screens[t.id] = "Starting the agent for $title…\r\n"
        reply(id, "terminal" to enc(t), "existing" to JsonPrimitive(false))
        if (watching) send(buildJsonObject { put("type", "changed"); put("terminal", enc(t)) })
        loop?.let { armLoop(t.id, json.decodeFromJsonElement<LoopSpec>(it), null) }
    }

    /** [ws] with its live Task agents under every connection and MCP server, as the server answers (ADR 0075). */
    private fun withTasks(ws: Workspace): Workspace {
        val agents = terminals.filter { it.live && it.isTaskAgent && it.labels["muxa.workspace"] == ws.name }.map { it.id }
        return ws.copy(
            connections = ws.connections.map { it.copy(usedBy = it.usedBy.copy(tasks = agents)) },
            mcp = ws.mcp.map { it.copy(usedBy = it.usedBy.copy(tasks = agents)) },
        )
    }

    private fun armLoop(terminal: String, spec: LoopSpec, previous: LoopInfo?): LoopInfo {
        val lp = LoopInfo(
            terminal, spec.check ?: "make test", spec.maxAttempts ?: 3, spec.timeout ?: 600, "armed",
            previous?.attempts ?: 0, now(), now(),
        )
        setLoop(lp)
        return lp
    }

    /** Marks the Routine at [at] running; false after an error. */
    private fun startRoutine(id: Long, at: Int): Boolean {
        if (at < 0) return false.also { send(error(id, "bad-request", "no such Routine")) }
        val r = routines[at]
        if (r.runningSince != null) return false.also { send(error(id, "conflict", "${r.name} already runs")) }
        routines[at] = r.copy(runningSince = now())
        return true
    }

    /** Closes the Task: its agent terminal goes away. A worktree that holds work needs [force]. */
    private fun approve(id: Long, item: String, force: Boolean) {
        val review = openItem(id, item, ItemKind.REVIEW) ?: return
        val terminal = review.terminal ?: return send(error(id, "bad-request", "$item has no Task"))
        losesWork[item]?.takeIf { !force }?.let { return send(error(id, "would-lose-work", it)) }
        channels.filterValues { it == terminal }.keys.sorted().forEach { n ->
            channels.remove(n)
            write(Frame(n, FrameKind.CLOSE, ByteArray(0)))
        }
        if (terminals.removeAll { it.id == terminal }) {
            screens.remove(terminal)
            if (watching) send(buildJsonObject { put("type", "removed"); put("terminal", terminal) })
        }
        resolve(id, review)
    }

    /** Types the comment to the Task's agent, as the user would. */
    private fun requestChanges(id: Long, item: String, comment: String) {
        val review = openItem(id, item, ItemKind.REVIEW) ?: return
        val terminal = review.terminal ?: return send(error(id, "bad-request", "$item has no Task"))
        if (comment.isBlank()) return send(error(id, "bad-request", "the comment is empty"))
        show(terminal, "$comment\r\n".toByteArray(Charsets.UTF_8))
        resolve(id, review)
    }

    /** Runs the item's Routine again, or wakes its Task's agent. */
    private fun retry(id: Long, item: String) {
        val failed = openItem(id, item, ItemKind.FAILED) ?: return
        val routine = failed.routine
        val terminal = failed.terminal
        when {
            routine != null -> {
                val at = routines.indexOfFirst { it.workspace == routine.workspace && it.name == routine.name }
                if (!startRoutine(id, at)) return
            }
            terminal != null -> {
                val at = terminals.indexOfFirst { it.id == terminal }
                if (at < 0) return send(error(id, "bad-request", "the Task's agent is gone"))
                val t = terminals[at].copy(live = true, status = TerminalStatus("working", terminals[at].status?.app))
                terminals[at] = t
                if (watching) send(buildJsonObject { put("type", "changed"); put("terminal", enc(t)) })
            }
            else -> return send(error(id, "bad-request", "$item has no Routine or Task to retry"))
        }
        resolve(id, failed)
    }

    /** The open item of [kind], or null after an error. */
    private fun openItem(id: Long, item: String, kind: ItemKind): InboxItem? {
        val open = inbox.items.firstOrNull { it.id == item }
        if (open == null && inbox.done.none { it.id == item }) return null.also { send(error(id, "not-found", "no such item")) }
        if (open == null || open.kind != kind) return null.also { send(error(id, "bad-request", "$item is not an open ${kind.wire} item")) }
        return open
    }

    private fun resolve(id: Long, item: InboxItem) = change(id) { i -> i.copy(items = i.items - item, done = listOf(item) + i.done) }

    /** Applies an Inbox change; null from [f] means the item was not found. */
    private fun change(id: Long, f: (Inbox) -> Inbox?) {
        val next = f(inbox) ?: return send(error(id, "not-found", "no such item"))
        inbox = next.copy(revision = inbox.revision + 1)
        reply(id, "inbox" to enc(inbox))
        if (watching) send(buildJsonObject { put("type", "inbox"); put("inbox", enc(inbox)) })
    }

    private fun capture(text: String, today: String): Capture {
        val words = text.split(" ").filter { it.isNotEmpty() }.toMutableList()
        var kind = ItemKind.TODO
        CAPTURE_KINDS[words.firstOrNull()?.lowercase()]?.let {
            kind = it
            words.removeAt(0)
        }
        var due: String? = null
        val at = words.indexOfFirst { it.lowercase() == "due" }
        if (at >= 0 && at + 1 < words.size) {
            due = day(words[at + 1].lowercase(), today)
            if (due != null) repeat(2) { words.removeAt(at) }
        }
        var workspace: String? = null
        words.removeAll { w ->
            if (workspace != null || !w.startsWith("#")) return@removeAll false
            val ws = workspaces.firstOrNull { it.name.lowercase() == w.drop(1).lowercase() } ?: return@removeAll false
            workspace = ws.name
            true
        }
        return Capture(kind, words.joinToString(" "), workspace, due)
    }

    private fun day(word: String, today: String): String? = try {
        when (word) {
            "today" -> today
            "tomorrow" -> LocalDate.parse(today).plusDays(1).toString()
            else -> LocalDate.parse(word).toString()
        }
    } catch (_: DateTimeParseException) {
        null
    }

    private fun newId() = "d${nextItem++}"

    private fun reply(id: Long, vararg fields: Pair<String, JsonElement>) = send(buildJsonObject {
        fields.forEach { (k, v) -> put(k, v) }
        put("type", "reply")
        put("id", id)
    })

    private fun error(id: Long?, code: String, message: String) = buildJsonObject {
        put("type", "error")
        if (id != null) put("id", id)
        put("code", code)
        put("message", message)
    }

    private fun send(msg: JsonObject) = write(Frame.control(msg.toString()))

    private inline fun <reified T> enc(value: T): JsonElement = json.encodeToJsonElement(value)

    private companion object {
        const val CR: Byte = 13
        const val LF: Byte = 10
        val CAPTURE_KINDS = mapOf("reply" to ItemKind.REPLY, "read" to ItemKind.READ, "decide" to ItemKind.DECIDE)

        fun now() = System.currentTimeMillis() / 1000
    }
}

private fun JsonObject.str(key: String) = (this[key] as? JsonPrimitive)?.contentOrNull

private fun JsonObject.long(key: String) = (this[key] as? JsonPrimitive)?.longOrNull

private fun JsonObject.names(key: String) = (this[key] as? JsonArray).orEmpty().mapNotNull { (it as? JsonPrimitive)?.contentOrNull }

internal fun agentLabels(branch: String, repo: String, dir: String) = mapOf(
    "muxa.kind" to "task", "muxa.role" to "agent", "muxa.branch" to branch, "muxa.repo" to repo, "muxa.dir" to dir,
    "muxa.workspace" to "app",
)

private fun standardContent(): DemoContent {
    fun agent(id: String, branch: String, state: String) = Terminal(
        id, branch, live = true, status = TerminalStatus(state, "claude-code"),
        labels = agentLabels(branch, "/demo/app", "/demo/app-$branch"), cols = 80, rows = 24,
    )

    fun item(
        id: String,
        kind: ItemKind,
        title: String,
        note: String? = null,
        workspace: String? = "app",
        due: String? = null,
        source: String,
        terminal: String? = null,
        routine: RoutineRef? = null,
        place: ItemPlace? = null,
    ) = InboxItem(
        id, kind, title, note, workspace, due, created = 1_764_000_000, source = source, terminal = terminal, routine = routine,
        place = place,
    )

    fun routine(name: String, at: String?, last: RoutineLast?) =
        Routine("app", name, enabled = true, at = at, timezone = at?.let { "Europe/Berlin" }, last = last)

    val everyRoutine = listOf("dependency-digest", "nightly-backup", "release-notes")
    return DemoContent(
        terminals = listOf(
            agent("t1", "fix-login", "working"),
            agent("t2", "docs-audit", "blocked"),
            Terminal("t3", "shell", live = true, cols = 80, rows = 24),
            // No name and no labels: a terminal muxa did not start, as the Other terminals show it.
            Terminal("t4", live = true, cols = 80, rows = 24),
        ),
        screens = mapOf(
            "t1" to "● Reading src/auth/session.ts\r\n● Editing src/auth/session.ts\r\n  The session now refreshes before it expires.\r\n",
            "t2" to "● The README links three pages that moved.\r\n\r\n  Allow edits to README.md? (y/n) ",
            "t3" to "$ ",
            "t4" to "$ ",
        ),
        inbox = listOf(
            item("d1", ItemKind.REVIEW, "Review fix-login", note = "Draft PR #42: refresh the session before it expires.", source = "task", terminal = "t1"),
            item("d2", ItemKind.REPLY, "Reply to Anna about the launch date", workspace = "Personal", due = "2026-11-27", source = "capture"),
            item("d3", ItemKind.DECIDE, "Upgrade the database to Postgres 17?", note = "Two extensions need a new version first.", source = "routine"),
            item("d4", ItemKind.READ, "Weekly dependency digest", note = "3 updates, 1 security fix.", source = "routine"),
            item("d5", ItemKind.FAILED, "nightly-backup failed", note = "gh login expired.", source = "routine", routine = RoutineRef("app", "nightly-backup")),
            item("d6", ItemKind.TODO, "Renew the domain", workspace = "Personal", due = "2026-12-01", source = "capture"),
            item("d8", ItemKind.REVIEW, "Review docs-audit", note = "Two links still to check.", source = "task", terminal = "t2"),
            item(
                "d9", ItemKind.TODO, "Resume: Tidy the release notes",
                note = "From the old muxa. Resume Task starts the agent where it worked.", source = "v1",
                place = ItemPlace("app", "/demo/tasks/tidy-the-release-notes-t-1a2b3c"),
            ),
        ),
        routines = listOf(
            routine("dependency-digest", "09:00", RoutineLast(1_764_000_000, ok = true, published = 1)),
            routine("nightly-backup", "02:00", RoutineLast(1_764_000_000, ok = false, reason = "gh login expired.")),
            routine("release-notes", null, null),
        ),
        workspaces = listOf(
            Workspace(
                "app", "/demo", listOf("app", "docs"), agent = "claude",
                loop = WorkspaceLoop(LoopChecks.List(listOf("make test", "make lint"))),
                connections = listOf(
                    WorkspaceConnection("github", isDefault = true, usedBy = UsedBy(routines = everyRoutine), lastUsed = 1_764_000_000),
                    WorkspaceConnection("linear", isDefault = true, usedBy = UsedBy(routines = everyRoutine), lastUsed = 1_763_990_000),
                ),
                mcp = listOf(WorkspaceMcp("docs", isDefault = true, usedBy = UsedBy(routines = everyRoutine), lastUsed = 1_764_000_000)),
            ),
            Workspace("Personal", "/demo/personal", emptyList(), agent = "claude"),
        ),
        files = mapOf(
            ReaderKind.DOCS to listOf(
                DemoFile(ReaderEntry("PLAN.md"), ReaderContent("PLAN.md", text = DEMO_PLAN)),
                DemoFile(
                    ReaderEntry(".muxa/TASK.md"),
                    ReaderContent(".muxa/TASK.md", text = "# fix-login\n\nRefresh the session before it expires.\n"),
                ),
            ),
            ReaderKind.CHANGES to listOf(
                DemoFile(
                    ReaderEntry("src/auth/session.ts", status = "modified", added = 2, removed = 1),
                    ReaderContent("src/auth/session.ts", text = DEMO_DIFF),
                ),
            ),
            ReaderKind.IMAGES to listOf(
                DemoFile(ReaderEntry("screenshots/login.png", size = 70), ReaderContent("screenshots/login.png", data = DEMO_PIXEL)),
            ),
        ),
        chats = mapOf(
            "t1" to ChatUpdate(
                reset = true,
                entries = listOf(
                    ChatEntry("u1", "user", "Refresh the session before it expires."),
                    ChatEntry("a1", "assistant", "I'll schedule the refresh **60 s** early."),
                    ChatEntry("t:1", "tool", tool = ChatTool("Edit", "src/auth/session.ts", result = "Updated.")),
                ),
            ),
            "t2" to ChatUpdate(
                reset = true,
                entries = listOf(
                    ChatEntry("u1", "user", "Fix the README links."),
                    ChatEntry("a1", "assistant", "Three pages moved. I'll update the links."),
                    ChatEntry("t:1", "tool", tool = ChatTool("Edit", "README.md")),
                ),
                prompt = ChatPrompt("t:1", "permission", options = listOf(ChatOption("Allow", "y"), ChatOption("Deny", "n", cancel = true))),
            ),
        ),
        losesWork = mapOf("d8" to "branch docs-audit in /demo/app-docs-audit has uncommitted changes. Approve with force to remove them anyway"),
        settings = DEMO_SETTINGS,
    )
}

private val DEMO_PLAN = """
    # Plan

    1. Refresh the session 60 s before it expires.
    2. Retry once when the refresh fails.
    3. Add a test for a session that expired while the app slept.

    The goal is in [the Task](.muxa/TASK.md).

""".trimIndent()

private val DEMO_DIFF = """
    diff --git a/src/auth/session.ts b/src/auth/session.ts
    --- a/src/auth/session.ts
    +++ b/src/auth/session.ts
    @@ -10,3 +10,4 @@ export function schedule(session: Session) {
    -  const at = session.expiresAt;
    +  const margin = 60_000;
    +  const at = session.expiresAt - margin;
       timer = setTimeout(refresh, at - Date.now());

""".trimIndent()

/** A 1×1 PNG. */
private const val DEMO_PIXEL = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII="

/** The demo's `workspaces.toml` and `routines.toml`, as `settings` shows them; the secret is only "set". */
private val DEMO_SETTINGS = Settings(
    revision = "1",
    workspaces = listOf(
        SettingsWorkspace(
            "app", "/demo", listOf("app", "docs"),
            agent = SettingsAgent("claude", listOf(EnvKey("CLAUDE_CONFIG_DIR", true))),
            loop = WorkspaceLoop(LoopChecks.List(listOf("make test", "make lint"))),
            routinesEnabled = true,
            connections = listOf(
                SettingsConnection("github", "GITHUB_TOKEN", "demo@example.com", "read", secret = SecretState.SET),
            ),
            mcp = listOf(McpSpec("docs", command = "npx", args = listOf("-y", "docs-mcp"))),
            routines = listOf(
                SettingsRoutine(
                    "dependency-digest", at = "09:00", timezone = "Europe/Berlin", instructions = "routines/dependency-digest.md",
                    instructionsText = "List the dependency updates of the week.\n",
                ),
            ),
        ),
        SettingsWorkspace("Personal", "/demo/personal", agent = SettingsAgent("claude")),
    ),
    workers = listOf(WorkerSpec("reviewer", "Reviews diffs", "claude-code")),
)
