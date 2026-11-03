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
import tech.xdge.muxa.protocol.AgentRun
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
import tech.xdge.muxa.protocol.RunReason
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskLoop
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.transport.BridgeChannel
import java.util.UUID

/**
 * An in-app bridge for a demo without a host: a few sessions, typed text echoed back,
 * and a chat for the agent pane %1. Most panes have a fixed size; the content's
 * followers, and new tasks, take the app's viewport as a host with `window-size latest`
 * does (ADR 0020). No SSH, no tmux. With [alerts], an agent elsewhere starts waiting a
 * few seconds in.
 */
/** [taskRecords] false plays a bridge from before task records, for the old New Task sheet's test. */
class DemoBridge(set: Content = Content.TESTS, alerts: Boolean = false, taskRecords: Boolean = true) : BridgeChannel {
    /** [TESTS] holds the fixtures the UI tests assert on; [SHOWCASE] looks like real work. */
    enum class Content { TESTS, SHOWCASE }

    private val content: DemoContent = when (set) {
        Content.TESTS -> TestContent
        Content.SHOWCASE -> ShowcaseContent
    }
    private val out = Channel<String>(Channel.UNLIMITED)
    override val lines: ReceiveChannel<String> = out
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val lock = Any()
    private var watch = 0L
    private var watchedPane: String? = null

    /** The app's grid at its default font, once it sent one. */
    private var viewport: Pair<Int, Int>? = null

    /** Panes whose window follows the device in use. */
    private val followers = content.followers.toMutableSet()
    private var chat: ChatFollow? = null
    private var openDoc: Long? = null
    private var openDiff: Long? = null

    /** The last paste into %1, sent as a prompt when Enter follows. */
    private var prompt: String? = null

    /** What the demo agent waits on, and the tool call it waits on. */
    private var waitingOn: Pair<ChatPrompt, ChatEntry>? = null

    /** Grows when a task starts. */
    private var sessions: List<Session> = content.sessions(now())
    private val images = content.images(now())

    /** The host's task records; grows when a task is created. */
    private var tasks: List<TaskRecord> = content.tasks(now())
    private var createdCount = 0

    private class ChatFollow(val id: Long, var seq: Long = 0, var count: Int = 0)

    init {
        if (alerts) {
            scope.launch {
                delay(content.alertDelayMillis)
                agentWaits("$4")
            }
        }
        emit(Message.Hello(PROTOCOL_VERSION, "demo", "demo", if (taskRecords) CAPABILITIES else CAPABILITIES - "task-records"))
    }

    override suspend fun send(line: String) {
        when (val req = Wire.decodeRequest(line)) {
            is Request.List -> emit(Message.Result(req.id, sessions = synchronized(lock) { sessions }))
            is Request.Watch -> watch(req)
            is Request.Viewport -> viewport(req)
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
                // The bridge lists a Task's TASK.md, its HANDOFF.md, and its CHECK.md once a check ran.
                val brief = taskOf(req.pane)?.let {
                    listOfNotNull(
                        DocInfo(TASK_DOC, DocInfo.Kind.REPO, "TASK.md", it.createdAt),
                        DocInfo(HANDOFF_DOC, DocInfo.Kind.REPO, "HANDOFF.md", handoffTime(it)),
                        it.loop?.lastCheck?.let { check -> DocInfo(CHECK_DOC, DocInfo.Kind.REPO, "CHECK.md", check.at) },
                    )
                }
                emit(Message.DocList(req.id, content.root, content.docs.map { it.info } + brief.orEmpty()))
                ok(req.id)
            }
            is Request.CreateTask -> scope.launch {
                // A launch takes a moment on a real host too.
                delay(1_000)
                createTask(req)
            }
            is Request.UpdateTask -> updateTask(req)
            is Request.RecoverRestart -> fail(req.id, ErrorCode.UNSUPPORTED, "Restart recovery is not implemented on this host.")
            is Request.RestartAgent -> restartAgent(req)
            is Request.StartLoop -> startLoop(req)
            is Request.StopLoop -> stopLoop(req)
            is Request.ListTasks -> {
                emit(Message.TaskList(req.id, content.workspaces, synchronized(lock) { tasks }))
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
                val branch = BranchChanges("origin/main", 3, content.branch.map { it.info })
                emit(Message.ChangeList(req.id, content.root, content.changes.map { it.info }, branch = branch))
                ok(req.id)
            }
            is Request.Diff -> if (req.scope == DiffScope.BRANCH) branchDiff(req) else diff(req)
            is Request.Undiff -> {
                synchronized(lock) { openDiff = null }
                ok(req.id)
            }
            is Request.Images -> {
                emit(Message.ImageList(req.id, content.root, images.map { it.info }))
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
        synchronized(lock) {
            watch = req.id
            watchedPane = req.pane
        }
        ok(req.id)
        val p = sized(req.pane) ?: return
        emit(Message.Screen(req.id, req.pane, p.cols, p.rows, Bytes(content.screen(p))))
    }

    private fun viewport(req: Request.Viewport) {
        ok(req.id)
        val (w, pane) = synchronized(lock) {
            viewport = req.cols to req.rows
            watch to watchedPane
        }
        if (pane == null || synchronized(lock) { pane !in followers }) return
        val p = sized(pane) ?: return
        emit(Message.Screen(w, pane, p.cols, p.rows, Bytes(content.screen(p))))
    }

    /** The pane, at the viewport's size when its window follows the device. */
    private fun sized(id: String): Pane? = synchronized(lock) {
        val size = viewport?.takeIf { id in followers }
        if (size != null) {
            sessions = sessions.map { s ->
                s.copy(panes = s.panes.map { if (it.id == id) it.copy(cols = size.first, rows = size.second) else it })
            }
        }
        sessions.flatMap { it.panes }.firstOrNull { it.id == id }
    }

    private fun chat(req: Request.Chat) {
        if (req.pane != CHAT_PANE) {
            fail(req.id, ErrorCode.UNSUPPORTED, "no agent adapter knows this pane")
            return
        }
        ok(req.id)
        synchronized(lock) { chat = ChatFollow(req.id) }
        emitChat(content.conversation, content.chatStatus, reset = true)
    }

    /** The record the session of [pane] runs. */
    private fun taskOf(pane: String): TaskRecord? = synchronized(lock) {
        val session = sessions.firstOrNull { s -> s.panes.any { it.id == pane } }
        tasks.firstOrNull { it.id == session?.taskId }
    }

    private fun doc(req: Request.Doc) {
        val task = taskOf(req.pane)
        if (req.path == TASK_DOC && task != null) {
            synchronized(lock) { openDoc = req.id }
            ok(req.id)
            emit(Message.DocContent(req.id, req.path, brief(task), task.createdAt))
            return
        }
        if (req.path == HANDOFF_DOC && task != null) {
            synchronized(lock) { openDoc = req.id }
            ok(req.id)
            emit(Message.DocContent(req.id, req.path, handoff(task), handoffTime(task)))
            return
        }
        val check = task?.loop?.lastCheck
        if (req.path == CHECK_DOC && check != null) {
            synchronized(lock) { openDoc = req.id }
            ok(req.id)
            emit(Message.DocContent(req.id, req.path, checkLog(check), check.at))
            return
        }
        val doc = content.docs.firstOrNull { it.info.path == req.path }
        if (doc == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not listed")
            return
        }
        synchronized(lock) { openDoc = req.id }
        ok(req.id)
        emit(Message.DocContent(req.id, req.path, doc.text, 1))
        // The agent keeps writing its plan: an update arrives while it is open.
        val update = content.planUpdate
        if (doc.info.kind == DocInfo.Kind.PLAN && update != null) {
            scope.launch {
                delay(UPDATE_DELAY_MILLIS)
                if (synchronized(lock) { openDoc } != req.id) return@launch
                emit(Message.DocContent(req.id, req.path, doc.text + update, 2))
            }
        }
    }

    private fun branchDiff(req: Request.Diff) {
        val change = content.branch.firstOrNull { it.info.path == req.path }
        if (change == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not in the branch")
            return
        }
        synchronized(lock) { openDiff = req.id }
        ok(req.id)
        emit(Message.DiffContent(req.id, req.path, change.diff, binary = false))
    }

    private fun diff(req: Request.Diff) {
        val change = content.changes.firstOrNull { it.info.path == req.path }
        if (change == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.path} is not changed")
            return
        }
        synchronized(lock) { openDiff = req.id }
        ok(req.id)
        val diff = change.diff
        emit(Message.DiffContent(req.id, req.path, diff, binary = diff == null))
        // The agent keeps editing: the diff grows while it is open.
        val update = content.diffUpdate
        if (diff != null && update != null) {
            scope.launch {
                delay(UPDATE_DELAY_MILLIS)
                if (synchronized(lock) { openDiff } != req.id) return@launch
                emit(Message.DiffContent(req.id, req.path, diff + update, binary = false))
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
                // Made detached at tmux's default size; its window follows the device.
                val pane = Pane("%$n", "@$n", 0, Role.AGENT, 80, 24, "claude", repo, false, false)
                followers += pane.id
                val repoName = repo.split("/").lastOrNull { it.isNotEmpty() }
                sessions = sessions + Session("$$n", name, repoName, repo, branch, panes = listOf(pane))
            }
        }
        emit(Message.TaskCreated(id, name))
        ok(id)
        emit(Message.SessionsChanged)
    }

    /**
     * Records a Task and starts a session for it, with a running agent. [FAILING_BRANCH]
     * keeps the record with a note and no session, as a failed launch does on a host.
     */
    private fun createTask(req: Request.CreateTask) {
        if (req.worker != null) {
            fail(req.id, ErrorCode.UNSUPPORTED, "Workers are not implemented in the demo.")
            return
        }
        val workspace = content.workspaces.firstOrNull { it.name == req.workspace }
        if (workspace == null) {
            fail(req.id, ErrorCode.BAD_REQUEST, "${req.workspace} is not a workspace")
            return
        }
        req.loop?.let { spec -> loopRefusal(workspace, spec) }?.let {
            fail(req.id, ErrorCode.BAD_REQUEST, it)
            return
        }
        val now = now()
        val failed = req.branch == FAILING_BRANCH
        val record = synchronized(lock) {
            val id = "t-new%03d".format(++createdCount)
            val name = slug(req.title)
            val dir = req.repo?.let { "/src/.worktrees/$name" } ?: "/${req.workspace}/tasks/$name-$id"
            val record = TaskRecord(
                id, rev = 1, workspace = req.workspace, title = req.title, goal = req.goal, status = TaskStatus.OPEN,
                createdAt = now, updatedAt = now, project = req.project, repo = req.repo, branch = req.branch,
                dir = dir, session = name.takeUnless { failed }, loop = req.loop?.let { startedLoop(null, it, reset = true, now) },
                note = if (failed) "could not create a worktree for $FAILING_BRANCH: a branch named '$FAILING_BRANCH' already exists" else null,
            )
            tasks = tasks + record
            if (!failed) {
                val n = 20 + sessions.size
                val pane = Pane("%$n", "@$n", 0, Role.AGENT, 80, 24, "claude", dir, false, false, Agent(AgentState.RUNNING, now))
                followers += pane.id
                val repoName = req.repo?.split("/")?.lastOrNull { it.isNotEmpty() }
                sessions = sessions + Session("$$n", name, repoName, req.repo, req.branch, taskId = id, panes = listOf(pane))
            }
            record
        }
        emit(Message.TaskSaved(req.id, record))
        if (failed) {
            fail(req.id, ErrorCode.TASK_FAILED, record.note ?: "")
            return
        }
        ok(req.id)
        emit(Message.SessionsChanged)
    }

    /** Sets a status, refusing a stale [Request.UpdateTask.rev] as the bridge does. */
    private fun updateTask(req: Request.UpdateTask) {
        // The demo does not advertise `task-inbox`.
        val status = req.status ?: return fail(req.id, ErrorCode.UNSUPPORTED, "the demo keeps no Inbox order")
        val saved = synchronized(lock) {
            val current = tasks.firstOrNull { it.id == req.task } ?: return fail(req.id, ErrorCode.BAD_REQUEST, "${req.task} is not a task")
            if (current.rev != req.rev) return fail(req.id, ErrorCode.CONFLICT, "the task changed since you read it")
            val now = now()
            val saved = current.copy(rev = current.rev + 1, status = status, updatedAt = now, loop = current.loop?.afterStatus(status, now))
            tasks = tasks.map { if (it.id == saved.id) saved else it }
            saved
        }
        emit(Message.TaskSaved(req.id, saved))
        ok(req.id)
    }

    /** Starts a Task's loop, also a stopped or passed one, when its workspace allows the check. */
    private fun startLoop(req: Request.StartLoop) = changeLoop(req.id, req.task) { current, now ->
        val refusal = loopRefusal(content.workspaces.firstOrNull { it.name == current.workspace }, req.loop)
        if (refusal != null) LoopChange.Refused(refusal) else LoopChange.Saved(startedLoop(current.loop, req.loop, req.reset, now))
    }

    private fun stopLoop(req: Request.StopLoop) = changeLoop(req.id, req.task) { current, now ->
        current.loop?.let { LoopChange.Saved(it.stopped(now)) } ?: LoopChange.Refused("${req.task} has no loop")
    }

    private sealed interface LoopChange {
        data class Saved(val loop: TaskLoop) : LoopChange
        data class Refused(val reason: String) : LoopChange
    }

    /**
     * Saves the loop [change] makes, with a new rev, then `task-saved`, `result`, and
     * `tasks-changed` as the host's poll would send it to every bridge.
     */
    private fun changeLoop(id: Long, task: String, change: (TaskRecord, Long) -> LoopChange) {
        val now = now()
        val saved = synchronized(lock) {
            val current = tasks.firstOrNull { it.id == task } ?: return fail(id, ErrorCode.BAD_REQUEST, "$task is not a task")
            val loop = when (val result = change(current, now)) {
                is LoopChange.Refused -> return fail(id, ErrorCode.BAD_REQUEST, result.reason)
                is LoopChange.Saved -> result.loop
            }
            val saved = current.copy(rev = current.rev + 1, updatedAt = now, loop = loop)
            tasks = tasks.map { if (it.id == saved.id) saved else it }
            saved
        }
        emit(Message.TaskSaved(id, saved))
        ok(id)
        emit(Message.TasksChanged)
    }

    /**
     * Restarts a Task's agent as the bridge does (ADR 0018): ends the open run, adds a
     * restart run, and the agent runs again. A running agent needs [Request.RestartAgent.force].
     */
    private fun restartAgent(req: Request.RestartAgent) {
        if (req.worker != null || req.rev != null) {
            fail(req.id, ErrorCode.UNSUPPORTED, "Workers are not implemented in the demo.")
            return
        }
        val now = now()
        val saved = synchronized(lock) {
            val current = tasks.firstOrNull { it.id == req.task } ?: return fail(req.id, ErrorCode.BAD_REQUEST, "${req.task} is not a task")
            val session = sessions.firstOrNull { it.taskId == current.id }
            val pane = session?.panes?.firstOrNull { it.role == Role.AGENT }
                ?: return fail(req.id, ErrorCode.TASK_FAILED, "No session runs this task's agent.")
            if (pane.agent?.state == AgentState.RUNNING && !req.force) {
                return fail(req.id, ErrorCode.BAD_REQUEST, "The agent is running. Restart with force to stop it.")
            }
            val ended = current.runs.map { if (it.endedAt == null) it.copy(endedAt = now) else it }
            val run = AgentRun(
                id = (current.runs.maxOfOrNull { it.id } ?: 0) + 1, agent = "claude-code", startedAt = now,
                agentSession = UUID.randomUUID().toString(), reason = RunReason.RESTART,
            )
            val saved = current.copy(rev = current.rev + 1, updatedAt = now, note = null, runs = ended + run)
            tasks = tasks.map { if (it.id == saved.id) saved else it }
            val running = pane.copy(agent = Agent(AgentState.RUNNING, now))
            sessions = sessions.map { s ->
                if (s.id != session.id) s else s.copy(panes = s.panes.map { if (it.id == pane.id) running else it })
            }
            saved
        }
        emit(Message.TaskSaved(req.id, saved))
        ok(req.id)
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

        /** Where the bridge writes a Task's brief for its agent (ADR 0015). */
        const val TASK_DOC = ".muxa/TASK.md"

        /** Where the bridge writes the facts for a restarted agent (ADR 0018). */
        const val HANDOFF_DOC = ".muxa/HANDOFF.md"

        /** Where the daemon writes the last check's result (ADR 0019). */
        const val CHECK_DOC = ".muxa/CHECK.md"

        /** A branch whose task fails, to show the error. */
        const val FAILING_BRANCH = "taken"

        private val CAPABILITIES =
            listOf(
                "chat", "docs", "tasks", "changes", "close-task", "images", "branch-changes", "task-records", "task-handoff",
                "task-loop", "tasks-changed", "viewport",
            )
        private const val UPDATE_DELAY_MILLIS = 2_000L

        private fun now() = System.currentTimeMillis() / 1_000

        /** A session name from a title: lowercase letters, digits and dashes, at most 20 characters. */
        internal fun slug(title: String): String =
            title.lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-').take(20).trim('-').ifEmpty { "task" }

        /** What the bridge would write into `TASK.md`. */
        internal fun brief(task: TaskRecord) = buildString {
            append("# ").append(task.title).append("\n\n")
            append("## Goal\n\n").append(task.goal).append("\n\n")
            append("## Workspace\n\n").append(task.workspace)
            task.project?.let { append(" · ").append(it) }
            task.repo?.let { append("\n\n## Resources\n\n- Repo: `").append(it).append("`, branch `").append(task.branch ?: "").append("`") }
            append("\n")
        }

        /** When the bridge last wrote HANDOFF.md: the newest restart, else when the Task began. */
        internal fun handoffTime(task: TaskRecord): Long =
            task.runs.filter { it.reason == RunReason.RESTART }.maxOfOrNull { it.startedAt } ?: task.createdAt

        /** What the bridge would write into `HANDOFF.md`, with sample git facts. The iOS demo writes the same. */
        internal fun handoff(task: TaskRecord) = buildString {
            append("# Handoff\n\n## Task\n\n")
            append("- Status: ").append(task.status.raw).append("\n")
            append("- Note: ").append(task.note ?: "none").append("\n\n## Runs\n\n")
            for (run in task.runs) {
                append("- Run ").append(run.id).append(" · ").append(run.agent ?: "unknown agent")
                append(" · ").append(run.agentSession ?: "no session id").append("\n")
            }
            task.branch?.let { append("\n## Git\n\n- Branch: ").append(it).append("\n- 3 commits since origin/main\n") }
        }

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

    }
}
