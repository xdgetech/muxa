package tech.xdge.muxa.demo

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import tech.xdge.muxa.protocol.Agent
import tech.xdge.muxa.protocol.AgentState
import tech.xdge.muxa.protocol.BranchChanges
import tech.xdge.muxa.protocol.Bytes
import tech.xdge.muxa.protocol.ChatEntry
import tech.xdge.muxa.protocol.ChatPrompt
import tech.xdge.muxa.protocol.ChatTool
import tech.xdge.muxa.protocol.CloseMode
import tech.xdge.muxa.protocol.DiffScope
import tech.xdge.muxa.protocol.DocInfo
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.PROTOCOL_VERSION
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.transport.BridgeChannel

/**
 * An in-app bridge for a demo without a host: a few sessions, fixed-size panes, typed
 * text echoed back, and a chat for the agent pane %1. No SSH, no tmux. With [alerts],
 * an agent elsewhere starts waiting a few seconds in.
 */
class DemoBridge(alerts: Boolean = false) : BridgeChannel {
    private val out = Channel<String>(Channel.UNLIMITED)
    override val lines: ReceiveChannel<String> = out
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val lock = Any()
    private var watch = 0L
    private var chat: ChatFollow? = null
    private var openDoc: Long? = null
    private var openDiff: Long? = null

    /** The last paste into %1, sent as a prompt when Enter follows. */
    private var prompt: String? = null

    /** What the demo agent waits on, and the tool call it waits on. */
    private var waitingOn: Pair<ChatPrompt, ChatEntry>? = null

    /** Grows when a task starts. */
    private var sessions: List<Session> = DemoContent.sessions(now())
    private val images = DemoContent.images(now())

    private class ChatFollow(val id: Long, var seq: Long = 0, var count: Int = 0)

    init {
        if (alerts) {
            scope.launch {
                delay(DemoContent.ALERT_DELAY_MILLIS)
                agentWaits("$4")
            }
        }
        emit(Message.Hello(PROTOCOL_VERSION, "demo", "demo", CAPABILITIES))
    }

    override suspend fun send(line: String) {
        when (val req = Wire.decodeRequest(line)) {
            is Request.List -> emit(Message.Result(req.id, sessions = synchronized(lock) { sessions }))
            is Request.Watch -> watch(req)
            is Request.Text -> {
                ok(req.id)
                echo(req.pane, req.text.toByteArray())
            }
            is Request.Paste -> {
                ok(req.id)
                if (req.pane == CHAT_PANE) synchronized(lock) { prompt = req.text }
                echo(req.pane, req.text.replace("\n", "\r\n").toByteArray())
            }
            is Request.Chat -> chat(req)
            is Request.Unchat -> {
                synchronized(lock) { chat = null }
                ok(req.id)
            }
            is Request.Docs -> {
                emit(Message.DocList(req.id, DemoContent.ROOT, DemoContent.docs.map { it.info }))
                ok(req.id)
            }
            is Request.Doc -> doc(req)
            is Request.Undoc -> {
                synchronized(lock) { openDoc = null }
                ok(req.id)
            }
            is Request.NewTask -> scope.launch {
                // A launch takes a moment on a real host too.
                delay(1_000)
                startTask(req.id, req.repo, req.branch)
            }
            is Request.Changes -> {
                val branch = BranchChanges("origin/main", 3, DemoContent.branch.map { it.info })
                emit(Message.ChangeList(req.id, DemoContent.ROOT, DemoContent.changes.map { it.info }, branch = branch))
                ok(req.id)
            }
            is Request.Diff -> if (req.scope == DiffScope.BRANCH) branchDiff(req) else diff(req)
            is Request.Undiff -> {
                synchronized(lock) { openDiff = null }
                ok(req.id)
            }
            is Request.Images -> {
                emit(Message.ImageList(req.id, DemoContent.ROOT, images.map { it.info }))
                ok(req.id)
            }
            is Request.Image -> image(req)
            is Request.Unimage -> ok(req.id)
            is Request.CloseTask -> closeTask(req.id, req.session, req.mode)
            is Request.Keys -> keys(req)
            is Request.Bye -> out.close()
            else -> ok(req.id)
        }
    }

    override suspend fun close() {
        scope.cancel()
        out.close()
    }

    private fun watch(req: Request.Watch) {
        synchronized(lock) { watch = req.id }
        ok(req.id)
        val p = synchronized(lock) { sessions }.flatMap { it.panes }.firstOrNull { it.id == req.pane } ?: return
        emit(Message.Screen(req.id, req.pane, p.cols, p.rows, Bytes(screen(p))))
    }

    private fun chat(req: Request.Chat) {
        if (req.pane != CHAT_PANE) {
            fail(req.id, ErrorCode.UNSUPPORTED, "no agent adapter knows this pane")
            return
        }
        ok(req.id)
        synchronized(lock) { chat = ChatFollow(req.id) }
        emitChat(DemoContent.conversation, DemoContent.chatStatus, reset = true)
    }

    private fun doc(req: Request.Doc) {
        val doc = DemoContent.docs.firstOrNull { it.info.path == req.path }
        if (doc == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not listed")
            return
        }
        synchronized(lock) { openDoc = req.id }
        ok(req.id)
        emit(Message.DocContent(req.id, req.path, doc.text, 1))
        // The agent keeps writing its plan: an update arrives while it is open.
        if (doc.info.kind == DocInfo.Kind.PLAN) {
            scope.launch {
                delay(UPDATE_DELAY_MILLIS)
                if (synchronized(lock) { openDoc } != req.id) return@launch
                emit(Message.DocContent(req.id, req.path, doc.text + DemoContent.PLAN_UPDATE, 2))
            }
        }
    }

    private fun branchDiff(req: Request.Diff) {
        val change = DemoContent.branch.firstOrNull { it.info.path == req.path }
        if (change == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not in the branch")
            return
        }
        synchronized(lock) { openDiff = req.id }
        ok(req.id)
        emit(Message.DiffContent(req.id, req.path, change.diff, binary = false))
    }

    private fun diff(req: Request.Diff) {
        val change = DemoContent.changes.firstOrNull { it.info.path == req.path }
        if (change == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not changed")
            return
        }
        synchronized(lock) { openDiff = req.id }
        ok(req.id)
        val diff = change.diff
        emit(Message.DiffContent(req.id, req.path, diff, binary = diff == null))
        // The agent keeps editing: the diff grows while it is open.
        if (diff != null) {
            scope.launch {
                delay(UPDATE_DELAY_MILLIS)
                if (synchronized(lock) { openDiff } != req.id) return@launch
                emit(Message.DiffContent(req.id, req.path, diff + DemoContent.DIFF_UPDATE, binary = false))
            }
        }
    }

    private fun image(req: Request.Image) {
        val image = images.firstOrNull { it.info.path == req.path }
        if (image == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not listed")
            return
        }
        ok(req.id)
        emit(Message.ImageContent(req.id, req.path, Bytes(Png.screenshot(image.color)), image.info.mtime))
    }

    private fun keys(req: Request.Keys) {
        ok(req.id)
        if (req.pane == CHAT_PANE && req.keys == listOf("Enter")) {
            val text = synchronized(lock) { prompt.also { prompt = null } }
            if (text != null) answer(text)
        }
        if (req.pane == CHAT_PANE) pick(req.keys)
        for (key in req.keys) {
            when (key) {
                "Enter" -> echo(req.pane, "\r\n$ ".toByteArray())
                "BSpace" -> echo(req.pane, "\b \b".toByteArray())
                else -> echo(req.pane, "\u001b[7m<$key>\u001b[0m".toByteArray())
            }
        }
    }

    private fun startTask(id: Long, repo: String, branch: String) {
        if (branch == FAILING_BRANCH) {
            fail(id, ErrorCode.TASK_FAILED, "could not create a worktree for $branch: a branch named '$branch' already exists")
            return
        }
        val name = branch.replace("/", "-")
        synchronized(lock) {
            if (sessions.none { it.name == name }) {
                val n = 20 + sessions.size
                val pane = Pane("%$n", "@$n", 0, Role.AGENT, 100, 30, "claude", repo, false, false)
                val repoName = repo.split("/").lastOrNull { it.isNotEmpty() }
                sessions = sessions + Session("$$n", name, repoName, repo, branch, panes = listOf(pane))
            }
        }
        emit(Message.TaskCreated(id, name))
        ok(id)
        emit(Message.SessionsChanged)
    }

    private fun agentWaits(session: String) {
        synchronized(lock) {
            sessions = sessions.map { s ->
                if (s.id != session || s.panes.isEmpty()) return@map s
                val first = s.panes[0].copy(agent = Agent(AgentState.WAITING, now()))
                s.copy(panes = listOf(first) + s.panes.drop(1))
            }
        }
        emit(Message.SessionsChanged)
    }

    /** fix-login is not merged, so only keep and force close it. */
    private fun closeTask(id: Long, session: String, mode: CloseMode) {
        val found = synchronized(lock) { sessions.firstOrNull { it.id == session } }
        if (found?.worktree == null) {
            fail(id, ErrorCode.BAD_REQUEST, "$session is not a task")
            return
        }
        if (found.name == "fix-login" && mode == CloseMode.CLOSE) {
            fail(
                id, ErrorCode.TASK_FAILED,
                "fix-login is not merged. Close it and keep the worktree, or remove the worktree anyway.",
            )
            return
        }
        synchronized(lock) { sessions = sessions.filter { it.id != session } }
        ok(id)
        emit(Message.SessionsChanged)
    }

    private fun ok(id: Long) = emit(Message.Result(id))

    private fun fail(id: Long, code: ErrorCode, message: String) = emit(Message.Result(id, ErrorBody(code, message)))

    /**
     * The demo agent: echoes a prompt. `/wait` makes it wait on a menu it cannot
     * name, and `/allow`, `/ask` and `/plan` on a permission, a question and a plan.
     */
    private fun answer(text: String) {
        val n = synchronized(lock) {
            chat?.let { it.count += 1 }
            chat?.count ?: 0
        }
        val user = ChatEntry("u:demo$n", ChatEntry.Kind.USER, text)
        val waiting = demoPrompt(text, n)
        when {
            text == "/wait" -> emitChat(listOf(user), AgentState.WAITING)
            waiting != null -> {
                synchronized(lock) { waitingOn = waiting }
                emitChat(listOf(user, waiting.second), AgentState.WAITING, prompt = waiting.first)
            }
            else -> {
                val reply = ChatEntry("a:demo$n", ChatEntry.Kind.ASSISTANT, "You said: **$text**")
                emitChat(listOf(user, reply), AgentState.IDLE)
            }
        }
    }

    /** Keys that pick an option of the waiting prompt answer it. */
    private fun pick(keys: List<String>) {
        val (prompt, tool) = synchronized(lock) { waitingOn } ?: return
        val option = prompt.options.firstOrNull { it.keys == keys } ?: return
        synchronized(lock) { waitingOn = null }
        val result = if (option.cancel) "Declined" else "Chose ${option.label}"
        val done = tool.copy(tool = tool.tool?.copy(result = result, error = option.cancel))
        emitChat(listOf(done), AgentState.IDLE)
    }

    private fun emitChat(
        entries: List<ChatEntry>,
        status: AgentState,
        reset: Boolean = false,
        prompt: ChatPrompt? = null,
    ) {
        val (id, seq) = synchronized(lock) {
            val c = chat ?: return
            c.seq += 1
            c.id to c.seq
        }
        emit(
            Message.ChatEntries(
                chat = id, seq = seq, reset = reset, entries = entries, status = Agent(status, now()),
                agent = if (reset) "claude-code" else null, prompt = prompt,
            ),
        )
    }

    private fun echo(pane: String, data: ByteArray) {
        val w = synchronized(lock) { watch }
        emit(Message.Output(w, pane, Bytes(data)))
    }

    private fun emit(m: Message) {
        out.trySend(Wire.encode(m))
    }

    companion object {
        const val CHAT_PANE = "%1"

        /** A branch whose task fails, to show the error. */
        const val FAILING_BRANCH = "taken"

        private val CAPABILITIES =
            listOf("chat", "docs", "tasks", "changes", "close-task", "images", "branch-changes")
        private const val UPDATE_DELAY_MILLIS = 2_000L

        private fun now() = System.currentTimeMillis() / 1_000

        internal fun demoPrompt(text: String, n: Int): Pair<ChatPrompt, ChatEntry>? {
            val entry = "t:demo$n"
            fun tool(name: String, summary: String) = ChatEntry(entry, ChatEntry.Kind.TOOL, tool = ChatTool(name, summary))
            return when (text) {
                "/allow" -> ChatPrompt(
                    entry, ChatPrompt.Kind.PERMISSION,
                    options = listOf(
                        ChatPrompt.Option("Allow", listOf("1")),
                        ChatPrompt.Option("Deny", listOf("Escape"), cancel = true),
                    ),
                ) to tool("Bash", "rm -rf build")
                "/ask" -> ChatPrompt(
                    entry, ChatPrompt.Kind.QUESTION, "Which format should the config use?",
                    listOf(ChatPrompt.Option("TOML", listOf("1")), ChatPrompt.Option("JSON", listOf("2"))),
                ) to tool("AskUserQuestion", "Which format should the config use?")
                "/plan" -> ChatPrompt(
                    entry, ChatPrompt.Kind.PLAN,
                    options = listOf(ChatPrompt.Option("Keep planning", listOf("Escape"), cancel = true)),
                ) to tool("ExitPlanMode", "Reader plan")
                else -> null
            }
        }

        /** A frame that shows whether the grid is exact: corners, a ruler and the last row. */
        internal fun screen(p: Pane): ByteArray {
            val cols = p.cols
            val rows = p.rows
            val title = "${p.command} — pane ${p.id} ${cols}x$rows"
            val s = StringBuilder("\u001bc")
            if (p.id in DemoContent.freshShells) {
                s.append("\u001b[1;1H\u001b[7m").append(title.padEnd(cols).take(cols))
                s.append("\u001b[0m\u001b[2;1H$ ")
                return s.toString().toByteArray()
            }
            val ruler = (0 until cols).joinToString("") { (it % 10).toString() }
            s.append("\u001b[1;1H\u001b[36m").append(ruler).append("\u001b[0m")
            s.append("\u001b[2;1H\u001b[1m").append(title).append("\u001b[0m")
            s.append("\u001b[4;1H日本語 ✓ é  \u001b[31mred\u001b[32m green\u001b[34m blue\u001b[0m")
            s.append("\u001b[$rows;1H\u001b[7m").append(" ".repeat(cols - 1)).append("\u001b[0m")
            s.append("\u001b[$rows;${cols}HX")
            // A tall pane keeps its prompt near the bottom, like an agent's composer.
            s.append("\u001b[${if (rows > 50) rows - 1 else 6};1H$ ")
            return s.toString().toByteArray()
        }
    }
}
