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
    /** How the Bots' items ended before; Done and Delete add to them. */
    val botCounts: List<BotCount> = emptyList(),
) {
    companion object {
        /**
         * Two Tasks (one waits on the user), a shell, every Inbox kind, a v1 item to resume (`d9`), and four Routines.
         * Approving `d8` needs force. The Bot `pr-watch` owns the Routine `scan` and made `d10` and `d11`; `d12` holds
         * its settings proposal. The Capture assistant suggests a Workspace, a due date and a Task for `d13`.
         */
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
    private var inbox = Inbox(1, content.inbox, emptyList(), content.botCounts)

    /** Uploads by folder and file name, so a second one with the same name gets a number. */
    private val uploads = mutableSetOf<String>()
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

    /** Settings' Workspaces whose Routines are turned off, as the daemon reports them. */
    private fun routinesOff(): List<RoutinesOff> = settings.current.workspaces
        .filter { !it.routinesEnabled && it.routines.isNotEmpty() }
        .map { RoutinesOff(it.name, it.routines.size) }

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
                val moved = i.items.firstOrNull { it.id == item } ?: return@change null
                i.copy(items = i.items - moved + moved)
            }
            "inbox-done" -> change(id) { i ->
                val done = i.items.firstOrNull { it.id == item } ?: return@change null
                i.copy(items = i.items - done, done = listOf(done) + i.done, bots = counted(i.bots, done, deleted = false))
            }
            "inbox-reopen" -> change(id) { i ->
                val open = i.done.firstOrNull { it.id == item } ?: return@change null
                i.copy(items = listOf(open) + i.items, done = i.done - open)
            }
            "inbox-delete" -> change(id) { i ->
                if (i.items.none { it.id == item } && i.done.none { it.id == item }) return@change null
                val open = i.items.firstOrNull { it.id == item }
                i.copy(
                    items = i.items.filter { it.id != item }, done = i.done.filter { it.id != item },
                    bots = open?.let { counted(i.bots, it, deleted = true) } ?: i.bots,
                )
            }
            "inbox-delete-older" -> {
                val days = m.long("days")?.takeIf { it in 1..MAX_DELETE_DAYS }
                    ?: return send(error(id, "bad-request", "days must be 1 to $MAX_DELETE_DAYS"))
                val cutoff = now() - days * DAY
                change(id) { i ->
                    val (old, kept) = i.items.partition { it.created < cutoff }
                    i.copy(
                        items = kept, done = i.done.filter { it.created >= cutoff },
                        bots = old.fold(i.bots) { counts, it -> counted(counts, it, deleted = true) },
                    )
                }
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
            "task-shell" -> taskShell(id, m.str("terminal"))
            "close-terminal" -> {
                val t = terminals.firstOrNull { it.id == m.str("terminal") }
                if (t == null || !t.isTaskShell) return send(error(id, "bad-request", "only a Task's shell closes this way"))
                remove(t.id)
                reply(id)
            }
            "close-task" -> closeTask(id, m)
            "upload" -> upload(id, m)
            "inbox-request-changes" -> requestChanges(id, item, m.str("comment") ?: "")
            "inbox-retry" -> retry(id, item)
            "inbox-apply" -> applyProposal(id, item)
            "inbox-suggestion" -> suggestion(id, item, (m["accept"] as? JsonPrimitive)?.booleanOrNull == true)
            "workspaces" -> reply(id, "workspaces" to enc(workspaces.map(::withTasks)))
            "routines" -> reply(id, "routines" to enc(routines), "problems" to buildJsonArray {}, "off" to enc(routinesOff()))
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
                ops.mapNotNull { SettingsOp.fromJson(it) as? SettingsOp.BotRename }.forEach(::renameBot)
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
        val bot = m.str("bot")
        if (bot != null && !BOT_NAME.matches(bot)) return send(error(id, "bad-request", "a Bot's name is lowercase letters, digits, - and _"))
        // Every agent gets all the Workspace's connections and servers (ADR 0075);
        // `connections` and `mcp` in the request are ignored.
        val named = workspace?.connections.orEmpty().map { it.name }
        val servers = workspace?.mcp.orEmpty().map { it.name }
        val specs = places.mapNotNull { it as? JsonObject }
        val first = specs.firstOrNull { it.str("kind") == "branch" }
        val branch = first?.str("branch")
        val checkout = specs.firstOrNull { it.str("kind") == "checkout" }?.str("repo")
        val repo = first?.str("repo") ?: checkout
        // An `existing` place or a checkout is adopted as it is: it is the Task's folder.
        val adopted = specs.firstOrNull { it.str("kind") == "existing" }?.str("path")
            ?: checkout?.let { "${workspace?.root ?: "/demo"}/$it" }
        val dir = adopted ?: if (branch != null) "${repo ?: "/demo"}-$branch" else "/demo/tasks/$title"
        terminals.firstOrNull { it.live && it.isTaskAgent && it.taskDir == dir }?.let {
            return reply(id, "terminal" to enc(it), "existing" to JsonPrimitive(true))
        }
        val labels = buildMap {
            put("muxa.kind", "task"); put("muxa.role", "agent"); put("muxa.dir", dir); put("muxa.workspace", wsName)
            put("muxa.title", title)
            if (branch != null) put("muxa.branch", branch)
            if (repo != null) put("muxa.repo", repo)
            if (checkout != null) put("muxa.checkout", checkout)
            if (named.isNotEmpty()) put("muxa.connections", enc(named).toString())
            if (servers.isNotEmpty()) put("muxa.mcp", enc(servers).toString())
            if (bot != null) put("muxa.bot", bot)
        }
        val t = Terminal(
            newTerminalId(), title, live = true, status = TerminalStatus("working", "claude-code"),
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
        removeWithShells(terminal)
        resolve(id, review)
    }

    /** A shell beside a live Task agent, in its folder; three at most (ADR 0079). */
    private fun taskShell(id: Long, terminal: String?) {
        val agent = terminals.firstOrNull { it.id == terminal && it.live && it.isTaskAgent }
        val dir = agent?.taskDir ?: return send(error(id, "bad-request", "a shell starts beside a live Task's agent"))
        if (terminals.count { it.live && it.isTaskShell && it.taskDir == dir } >= MAX_SHELLS) {
            return send(error(id, "conflict", "a Task has at most $MAX_SHELLS shells; close one first"))
        }
        val labels = buildMap {
            put("muxa.kind", "task"); put("muxa.role", "shell"); put("muxa.dir", dir)
            agent.labels["muxa.workspace"]?.let { put("muxa.workspace", it) }
            agent.labels["muxa.title"]?.let { put("muxa.title", it) }
        }
        val t = Terminal(newTerminalId(), agent.name?.let { "$it-sh" }, live = true, labels = labels, cols = 80, rows = 24)
        terminals += t
        screens[t.id] = "$dir $ "
        reply(id, "terminal" to enc(t), "existing" to JsonPrimitive(false))
        if (watching) send(buildJsonObject { put("type", "changed"); put("terminal", enc(t)) })
    }

    /** Keeps an upload's name, as the host would write it (ADR 0081); nothing is saved. */
    private fun upload(id: Long, m: JsonObject) {
        val t = terminals.firstOrNull { it.id == m.str("terminal") && it.labels["muxa.kind"] == "task" }
            ?: return send(error(id, "bad-request", "files go to a Task"))
        val name = m.str("name")?.trim().orEmpty()
        if (name.isEmpty() || name.startsWith(".") || '/' in name) return send(error(id, "bad-request", "send the file name only"))
        val data = runCatching { java.util.Base64.getDecoder().decode(m.str("data").orEmpty()) }.getOrNull()
            ?: return send(error(id, "bad-request", "data is not base64"))
        if (data.size > MAX_UPLOAD) return send(error(id, "bad-request", "a file is at most 10 MB"))
        val dir = t.taskDir.orEmpty()
        val stem = name.substringBeforeLast('.', name)
        val ext = name.substringAfterLast('.', "")
        val file = generateSequence(1) { it + 1 }
            .map { n -> if (n == 1) name else if (ext.isEmpty()) "$name $n" else "$stem $n.$ext" }
            .first { "$dir/$it" !in uploads }
        uploads += "$dir/$file"
        reply(id, "path" to JsonPrimitive(".muxa/uploads/$file"))
    }

    /** Closes a Task without its item, as Approve does, with its shells and open items. */
    private fun closeTask(id: Long, m: JsonObject) {
        val agent = terminals.firstOrNull { it.id == m.str("terminal") && it.isTaskAgent }
            ?: return send(error(id, "bad-request", "only a Task closes this way"))
        val open = inbox.items.filter { it.terminal == agent.id }
        if (m.str("worktree") != "force") {
            open.firstNotNullOfOrNull { losesWork[it.id] }?.let { return send(error(id, "would-lose-work", it)) }
        }
        removeWithShells(agent.id)
        val ids = open.map { it.id }.toSet()
        change(id) { i -> i.copy(items = i.items.filter { it.id !in ids }, done = open + i.done) }
    }

    /** Removes a Task's agent, then the shells in its folder, as the server does. */
    private fun removeWithShells(agent: String) {
        val dir = terminals.firstOrNull { it.id == agent }?.taskDir
        remove(agent)
        terminals.filter { it.isTaskShell && dir != null && it.taskDir == dir }.forEach { remove(it.id) }
    }

    /** Removes a terminal and closes its channels. */
    private fun remove(terminal: String) {
        channels.filterValues { it == terminal }.keys.sorted().forEach { n ->
            channels.remove(n)
            write(Frame(n, FrameKind.CLOSE, ByteArray(0)))
        }
        if (terminals.removeAll { it.id == terminal }) {
            screens.remove(terminal)
            if (watching) send(buildJsonObject { put("type", "removed"); put("terminal", terminal) })
        }
    }

    /** A terminal id not in use, also after one was removed. */
    private fun newTerminalId(): String =
        generateSequence(terminals.size + 1) { it + 1 }.map { "t$it" }.first { id -> terminals.none { it.id == id } }

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

    /** As the server does after a `bot-rename`: the Bot's items, counts and Routines take the new name. */
    private fun renameBot(op: SettingsOp.BotRename) {
        fun moved(item: InboxItem) = if (item.workspace == op.workspace && item.bot == op.from) item.copy(bot = op.to) else item
        val i = inbox
        inbox = i.copy(
            revision = i.revision + 1, items = i.items.map(::moved), done = i.done.map(::moved),
            bots = i.bots.map { if (it.workspace == op.workspace && it.bot == op.from) it.copy(bot = op.to) else it },
        )
        routines.replaceAll { if (it.workspace == op.workspace && it.bot == op.from) it.copy(bot = op.to) else it }
        if (watching) send(buildJsonObject { put("type", "inbox"); put("inbox", enc(inbox)) })
    }

    /** Sends the item's ops as `settings-update` with its revision, then marks it Done (ADR 0083). */
    private fun applyProposal(id: Long, item: String) {
        val decide = openItem(id, item, ItemKind.DECIDE) ?: return
        val proposal = decide.settingsProposal ?: return send(error(id, "bad-request", "$item has no settings proposal"))
        try {
            settings.update(proposal.revision, proposal.ops.map { it.json() })
        } catch (e: DemoSettings.Refusal) {
            val message = if (e.code == "conflict") "the settings changed since the Bot proposed this; ask it again" else e.message.orEmpty()
            return send(error(id, e.code, message))
        }
        resolve(id, decide)
    }

    /**
     * Accept applies the kind, Workspace, due date and note (after the item's own); a split
     * puts its parts in the item's place and the item becomes Done. Either way the suggestion goes.
     */
    private fun suggestion(id: Long, item: String, accept: Boolean) {
        val open = inbox.items.firstOrNull { it.id == item } ?: return send(error(id, "not-found", "no such item"))
        val s = open.suggestion ?: return send(error(id, "bad-request", "$item has no suggestion"))
        val cleared = open.copy(suggestion = null)
        change(id) { i ->
            val at = i.items.indexOf(open)
            val rest = i.items - open
            when {
                !accept -> i.copy(items = i.items.map { if (it === open) cleared else it })
                s.split.isNotEmpty() -> {
                    val parts = s.split.map { title ->
                        InboxItem(newId(), s.kind ?: open.kind, title, workspace = s.workspace ?: open.workspace, created = now(), source = "capture")
                    }
                    i.copy(items = rest.toMutableList().apply { addAll(at, parts) }, done = listOf(cleared) + i.done)
                }
                else -> {
                    val note = listOfNotNull(open.note, s.note).joinToString("\n\n").ifEmpty { null }
                    val accepted = cleared.copy(
                        kind = s.kind ?: open.kind, workspace = s.workspace ?: open.workspace, due = s.due ?: open.due, note = note,
                    )
                    i.copy(items = i.items.map { if (it === open) accepted else it })
                }
            }
        }
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

    /** As the daemon counts: a Bot's item that ends Done, or is deleted while open. */
    private fun counted(counts: List<BotCount>, item: InboxItem, deleted: Boolean): List<BotCount> {
        val bot = item.bot ?: return counts
        val ws = item.workspace ?: return counts
        val old = counts.firstOrNull { it.workspace == ws && it.bot == bot } ?: BotCount(ws, bot, 0, 0)
        val new = if (deleted) old.copy(deleted = old.deleted + 1) else old.copy(done = old.done + 1)
        return counts.filter { it !== old } + new
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
        val BOT_NAME = Regex("[a-z0-9_-]{1,64}")

        const val DAY = 86_400L
        const val MAX_DELETE_DAYS = 3650L
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
        links: List<ItemLink> = emptyList(),
        proposal: ItemProposal? = null,
        bot: String? = null,
        settingsProposal: SettingsProposal? = null,
        suggestion: ItemSuggestion? = null,
    ) = InboxItem(
        id, kind, title, note, workspace, due, created = 1_764_000_000, source = source, terminal = terminal, routine = routine,
        place = place, links = links, proposal = proposal, bot = bot, settingsProposal = settingsProposal, suggestion = suggestion,
    )

    fun routine(name: String, at: String?, last: RoutineLast?, schedule: String? = null, bot: String? = null) =
        Routine("app", name, enabled = true, at = at, timezone = at?.let { "Europe/Berlin" }, schedule = schedule, last = last, bot = bot)

    val scan = RoutineRef("app", "scan")

    val everyRoutine = listOf("dependency-digest", "nightly-backup", "release-notes")
    return DemoContent(
        terminals = listOf(
            agent("t1", "fix-login", "working"),
            agent("t2", "docs-audit", "blocked"),
            Terminal("t3", "shell", live = true, cols = 80, rows = 24),
            // No name and no labels: a terminal muxa did not start, as the Other terminals show it.
            Terminal("t4", live = true, cols = 80, rows = 24),
            // fix-login's shell (ADR 0079).
            Terminal(
                "t6", "fix-login-sh", live = true, cols = 80, rows = 24,
                labels = mapOf("muxa.kind" to "task", "muxa.role" to "shell", "muxa.workspace" to "app", "muxa.dir" to "/demo/app-fix-login"),
            ),
            // A talk with pr-watch (ADR 0083); d12 is its proposal.
            Terminal(
                "t7", "Talk to pr-watch", live = true, status = TerminalStatus("done", "claude-code"), cols = 80, rows = 24,
                labels = mapOf(
                    "muxa.kind" to "task", "muxa.role" to "agent", "muxa.workspace" to "app", "muxa.bot" to "pr-watch",
                    "muxa.title" to "Talk to pr-watch", "muxa.dir" to TALK_DIR,
                    "muxa.places" to """[{"kind":"folder","path":"$TALK_DIR"}]""",
                ),
            ),
            // A run of pr-watch's Routine scan (ADR 0086).
            Terminal(
                "t8", live = true, status = TerminalStatus("working", "claude-code"), cols = 80, rows = 24,
                labels = mapOf(
                    "muxa.kind" to "routine", "muxa.role" to "agent", "muxa.workspace" to "app", "muxa.bot" to "pr-watch",
                    "muxa.routine" to "scan", "muxa.dir" to "/demo/runs/scan",
                ),
            ),
        ),
        screens = mapOf(
            "t1" to "● Reading src/auth/session.ts\r\n● Editing src/auth/session.ts\r\n  The session now refreshes before it expires.\r\n",
            "t2" to "● The README links three pages that moved.\r\n\r\n  Allow edits to README.md? (y/n) ",
            "t3" to "$ ",
            "t4" to "$ ",
            "t6" to "~/demo/app-fix-login $ ",
            "t7" to "› Every night, list the PRs nobody touched for a week.\r\n\r\n" +
                "● I wrote a proposal: a nightly-sweep Routine, turned off. Apply it from the bar above or the Inbox.\r\n",
            "t8" to "● Listing the PRs that wait for review\r\n",
        ),
        inbox = listOf(
            item("d1", ItemKind.REVIEW, "Review fix-login", note = "Draft PR #42: refresh the session before it expires.", source = "task", terminal = "t1"),
            item("d2", ItemKind.REPLY, "Reply to Anna about the launch date", workspace = "Personal", due = "2026-11-27", source = "capture"),
            item(
                "d3", ItemKind.DECIDE, "Upgrade the database to Postgres 17?",
                note = "Two extensions need a new version first:\n\n- `pgvector` 0.8\n- `postgis` 3.5\n\n" +
                    "**Plan:** upgrade them, then the database. See the [release notes](https://www.postgresql.org/docs/17/release-17.html).",
                source = "routine",
            ),
            item("d4", ItemKind.READ, "Weekly dependency digest", note = "3 updates, 1 security fix.", source = "routine"),
            item("d5", ItemKind.FAILED, "nightly-backup failed", note = "gh login expired.", source = "routine", routine = RoutineRef("app", "nightly-backup")),
            item("d6", ItemKind.TODO, "Renew the domain", workspace = "Personal", due = "2026-12-01", source = "capture"),
            item("d8", ItemKind.REVIEW, "Review docs-audit", note = "Two links still to check.", source = "task", terminal = "t2"),
            item(
                "d9", ItemKind.TODO, "Resume: Tidy the release notes",
                note = "From the old muxa. Resume Task starts the agent where it worked.", source = "v1",
                place = ItemPlace("app", "/demo/tasks/tidy-the-release-notes-t-1a2b3c"),
            ),
            item(
                "d10", ItemKind.DECIDE, "Approve the vite 6.1 bump?", note = "CI passed. The changelog lists no breaking change.",
                source = "routine", routine = scan, bot = "pr-watch",
                links = listOf(ItemLink("https://github.com/example/app/pull/57", "PR #57: bump vite to 6.1")),
                proposal = ItemProposal("review-approve", "example/app#57", "CI passed and the changelog lists no breaking change. Approving."),
            ),
            item(
                "d11", ItemKind.REPLY, "Answer Sam on PR #61", note = "Sam asks why the retry waits 30 s.", due = "2026-11-26",
                source = "routine", routine = scan, bot = "pr-watch",
                links = listOf(ItemLink("https://github.com/example/app/pull/61")),
            ),
            item(
                "d12", ItemKind.DECIDE, "pr-watch proposes: a nightly sweep",
                note = "- routine-add nightly-sweep (off)\n- bot-update pr-watch\n", source = "task", bot = "pr-watch", terminal = "t7",
                settingsProposal = SettingsProposal("1", NIGHTLY_SWEEP_OPS),
            ),
            item(
                "d13", ItemKind.TODO, "Look at the flaky login test", workspace = null, source = "capture",
                suggestion = ItemSuggestion(
                    workspace = "app", due = "2026-11-28", note = "It failed twice on CI this week.",
                    taskGoal = "Find why the login test fails now and then, and fix it.", bot = "pr-watch",
                ),
            ),
        ),
        routines = listOf(
            routine("dependency-digest", "09:00", RoutineLast(1_764_000_000, ok = true, published = 1)),
            routine("nightly-backup", "02:00", RoutineLast(1_764_000_000, ok = false, reason = "gh login expired.")),
            routine("release-notes", null, null),
            // t8 runs it.
            routine("scan", "09:00", RoutineLast(1_764_000_000, ok = true, published = 2), schedule = SCAN_SCHEDULE, bot = "pr-watch")
                .copy(nextDue = 1_764_001_800, runningSince = 1_764_000_000),
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
                branches = mapOf("app" to "main", "docs" to "main"),
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
                DemoFile(
                    ReaderEntry("output/qa/report.md", ignored = true),
                    ReaderContent("output/qa/report.md", text = DEMO_QA_REPORT),
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
            "t7" to ChatUpdate(
                reset = true,
                entries = listOf(
                    ChatEntry("u1", "user", "Every night, list the PRs nobody touched for a week."),
                    ChatEntry("a1", "assistant", "I wrote a proposal: a **nightly-sweep** Routine, turned off. Apply it from the bar above or the Inbox."),
                ),
            ),
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
        // pr-watch's runs added d10 and d11 today, as the host counts them (ADR 0086).
        botCounts = listOf(BotCount("app", "pr-watch", done = 4, deleted = 1, day = LocalDate.now().toString(), added = 2)),
    )
}

private const val SCAN_SCHEDULE = "*/30 9-18 * * 1-5"

/** The folder of the demo's talk with pr-watch. */
private const val TALK_DIR = "/demo/tasks/talk-to-pr-watch"

/** The shell commands only pr-watch may run (ADR 0088). */
private val PR_WATCH_COMMANDS = listOf("gh pr list:*")

/** What pr-watch proposed in a talk: an off Routine of its own, and a description that names CI. */
private val NIGHTLY_SWEEP_OPS = listOf(
    SettingsOp.RoutineAdd(
        "app",
        RoutineSpec("nightly-sweep", at = "02:00", timezone = "Europe/Berlin", instructions = "routines/nightly-sweep.md", enabled = false, bot = "pr-watch"),
        "Close the stale PRs of example/app.\n",
    ),
    // A bot-update replaces the whole Bot, so it keeps the commands it already has.
    SettingsOp.BotUpdate("app", BotSpec("pr-watch", "Watches the app's open PRs and their CI", commands = PR_WATCH_COMMANDS)),
)

private val DEMO_QA_REPORT = """
    # QA report

    - [x] A session refreshes 60 s before it expires.
    - [x] A failed refresh retries once.

""".trimIndent()

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
            botRules = BotRules(allow = listOf("WebSearch", "WebFetch"), deny = listOf("Bash")),
            connections = listOf(
                SettingsConnection("github", "GITHUB_TOKEN", "demo@example.com", "read", secret = SecretState.SET),
            ),
            mcp = listOf(McpSpec("docs", command = "npx", args = listOf("-y", "docs-mcp"))),
            routines = listOf(
                SettingsRoutine(
                    "dependency-digest", at = "09:00", timezone = "Europe/Berlin", instructions = "routines/dependency-digest.md",
                    instructionsText = "List the dependency updates of the week.\n",
                ),
                SettingsRoutine(
                    "scan", schedule = SCAN_SCHEDULE, timezone = "Europe/Berlin", instructions = "routines/scan.md",
                    instructionsText = "Look at the open PRs of example/app since the last run.\n", bot = "pr-watch", readOnly = true,
                ),
            ),
            bots = listOf(
                SettingsBot(
                    "pr-watch", "Watches the app's open PRs",
                    soulText = "You watch the open PRs of example/app.\nPropose a review only when CI passed. Write short items.\n",
                    memoryText = "- Sam reviews the auth code.\n- Dependabot PRs merge on Fridays.\n",
                    memoryVersions = 2,
                    commands = PR_WATCH_COMMANDS,
                    memoryHistory = listOf(MemoryVersion(1, 1_764_000_240), MemoryVersion(2, 1_763_913_600)),
                ),
            ),
        ),
        SettingsWorkspace(
            "Personal", "/demo/personal", agent = SettingsAgent("claude"),
            routines = listOf(
                SettingsRoutine(
                    "weekly-review", at = "18:00", timezone = "Europe/Berlin", instructions = "routines/weekly-review.md",
                    instructionsText = "Sum up the week's Done items.\n",
                ),
            ),
        ),
    ),
    workers = listOf(WorkerSpec("reviewer", "Reviews diffs", "claude-code")),
    assistant = SettingsAssistant(
        enabled = true,
        soulText = "Sort what I capture. Name the Workspace and a due date only when the item says so.\n",
        memoryText = "- Items about the login go to app.\n",
    ),
)
private const val MAX_SHELLS = 3
