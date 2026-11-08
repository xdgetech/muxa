package tech.xdge.muxa.session

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.SerializationException
import tech.xdge.muxa.protocol.Bytes
import tech.xdge.muxa.protocol.CloseMode
import tech.xdge.muxa.protocol.DiffScope
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.InboxAction
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.LoopSpec
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.PROTOCOL_VERSION
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.RoutineInfo
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.RestartRecoveryAction
import tech.xdge.muxa.protocol.WORK_ROUTINES
import tech.xdge.muxa.protocol.WORK_ASK
import tech.xdge.muxa.protocol.WORK_CLOSE_ASK
import tech.xdge.muxa.protocol.WORK_ITEMS
import tech.xdge.muxa.protocol.WORK_WORKTREE_END
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.protocol.WorktreeEnd
import tech.xdge.muxa.protocol.workLevel
import tech.xdge.muxa.terminal.PaneRenderer
import tech.xdge.muxa.transport.BridgeChannel
import tech.xdge.muxa.transport.TransportError
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

/**
 * One connection to a bridge: hello, the session list, and the watched pane.
 * The UI reads [state]; the renderer gets the pane's bytes.
 *
 * Every change runs on [dispatcher], which must run one coroutine at a time (the main
 * thread in the app). That keeps requests, and so keystrokes, in the order they were made.
 */
class PaneSession(
    private val dispatcher: CoroutineDispatcher,
    private val pingEvery: Duration = 15.seconds,
    private val helloTimeout: Duration = 10.seconds,
) {
    sealed interface Phase {
        data object Idle : Phase
        data object Connecting : Phase
        data object Connected : Phase
        data class Disconnected(val reason: String) : Phase
    }

    sealed class Failure(message: String) : Exception(message) {
        class NotConnected : Failure("not connected")
        class Bridge(val body: ErrorBody) : Failure(body.message)
        class ProtocolMismatch(val bridge: Int) : Failure("protocol $bridge")
        class NoHello : Failure("no hello")

        /** The Task was recorded, but its agent did not start; [record]'s note says why. */
        class LaunchFailed(val record: TaskRecord, val body: ErrorBody) : Failure(body.message)

        /** The agent's menu changed since the item was made; nothing was sent to the pane. */
        class ItemChanged : Failure("the item changed")
    }

    data class State(
        val phase: Phase = Phase.Idle,
        val tmuxVersion: String? = null,
        val sessions: List<Session> = emptyList(),
        /** The pane shown, and kept across reconnects. */
        val watchedPane: String? = null,
        /** Set when the watched pane disappears; cleared by the next watch. */
        val paneGoneReason: String? = null,
        /** The last failed input or command, for a short notice in the UI. */
        val lastError: String? = null,
        /** What the bridge supports beyond the base protocol, from `hello`. */
        val capabilities: List<String> = emptyList(),
        /** The followed agent conversation, kept across reconnects like the watch. */
        val chat: ChatLog? = null,
        /** Panes whose program no agent adapter knows; they show only the terminal. */
        val unsupportedChats: Set<String> = emptySet(),
        /** The document open in the Reader, kept across reconnects. */
        val doc: DocLog? = null,
        val diff: DiffLog? = null,
        val image: ImageLog? = null,
        /** The task records, once listed; null when the bridge keeps none or has not answered yet. */
        val tasks: TaskBoard? = null,
        /** The grid the pane area shows at the default font, kept across reconnects. */
        val viewport: PaneViewport? = null,
        /** The host's Routines, as last listed (`work` level 1, ADR 0026). */
        val routines: List<RoutineInfo> = emptyList(),
        /** The Inbox items, in the host's order, as last listed (`work` level 3, ADR 0033). */
        val inboxItems: List<InboxItem> = emptyList(),
        /** Why the Routines or the Inbox could not be listed; the last lists stay. */
        val workError: String? = null,
    ) {
        val canChat get() = "chat" in capabilities
        val canReadDocs get() = "docs" in capabilities
        val canShowHistory get() = "history" in capabilities
        val canShowChanges get() = "changes" in capabilities
        val canShowImages get() = "images" in capabilities
        val canStartTasks get() = "tasks" in capabilities
        val canCloseTasks get() = "close-task" in capabilities
        val canKeepTasks get() = "task-records" in capabilities
        val canRecoverRestarts get() = "task-restart-recovery" in capabilities
        val canUseWorkers get() = "workers" in capabilities
        val canRestartAgents get() = "task-handoff" in capabilities

        /** The host's bridge reached the muxa daemon, so Tasks can have a Task Loop (ADR 0019). */
        val canLoop get() = "task-loop" in capabilities

        /** The host's `work` level; 0 when it has none (ADR 0027 §5). */
        val workLevel get() = workLevel(capabilities)
        val canUseRoutines get() = workLevel >= WORK_ROUTINES

        /** The host starts ask sessions, names a branch after the title and takes relative repos (ADR 0031). */
        val canAsk get() = workLevel >= WORK_ASK

        /** The host closes ask sessions with `close-task` (ADR 0039). */
        val canCloseAskSessions get() = workLevel >= WORK_CLOSE_ASK && canCloseTasks

        /** The host keeps the Inbox as a list of questions (ADR 0033). */
        val canAnswerItems get() = workLevel >= WORK_ITEMS

        /** Done and Drop also remove the Task's worktree and branch, and ask first when that could lose work (ADR 0043). */
        val endsWorktrees get() = workLevel >= WORK_WORKTREE_END
    }

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()

    /** Gets every `screen` and `output` of the current watch. */
    @Volatile
    var renderer: PaneRenderer? = null

    private val scope = CoroutineScope(SupervisorJob() + dispatcher)
    private var channel: BridgeChannel? = null
    private var outbox: Channel<String>? = null
    private var writer: Job? = null
    private var jobs: List<Job> = emptyList()
    private var nextId = 0L
    private var watchId: Long? = null
    private var chatId: Long? = null
    private var docId: Long? = null
    private var diffId: Long? = null
    private var imageId: Long? = null
    // Lists and task names that arrive just before their `result`.
    private val docListings = mutableMapOf<Long, Message.DocList>()
    private val histories = mutableMapOf<Long, Message.History>()
    private val changeListings = mutableMapOf<Long, Message.ChangeList>()
    private val imageListings = mutableMapOf<Long, Message.ImageList>()
    private val createdTasks = mutableMapOf<Long, String>()
    private val taskListings = mutableMapOf<Long, Message.TaskList>()
    private val savedTasks = mutableMapOf<Long, TaskRecord>()
    private val routineListings = mutableMapOf<Long, List<RoutineInfo>>()
    private val inboxListings = mutableMapOf<Long, Message.InboxList>()
    private val savedRoutines = mutableMapOf<Long, RoutineInfo>()
    private val savedItems = mutableMapOf<Long, InboxItem>()
    private val startedSessions = mutableMapOf<Long, Message.SessionStarted>()
    private val pending = mutableMapOf<Long, CompletableDeferred<Message>>()

    /** Ids of input requests in flight; their results only matter when they fail. */
    private val inputs = mutableSetOf<Long>()
    private var hello: CompletableDeferred<Unit>? = null

    /** The viewport this bridge has; null after a connect. */
    private var sentViewport: PaneViewport? = null
    private var viewportJob: Job? = null

    // Lifecycle

    /**
     * Opens a channel, waits for `hello`, and lists sessions. On a reconnect it watches
     * the same pane again, unless [samePanes] is false (another host).
     */
    suspend fun connect(samePanes: Boolean = true, open: suspend () -> BridgeChannel) = withContext(dispatcher) {
        teardown(null)
        if (!samePanes) {
            _state.update {
                it.copy(
                    watchedPane = null, paneGoneReason = null, sessions = emptyList(), chat = null,
                    unsupportedChats = emptySet(), doc = null, diff = null, image = null, tasks = null,
                    routines = emptyList(), inboxItems = emptyList(), workError = null,
                )
            }
        }
        _state.update { it.copy(phase = Phase.Connecting) }
        try {
            val channel = open()
            val waitHello = CompletableDeferred<Unit>()
            hello = waitHello
            start(channel)
            withTimeoutOrNull(helloTimeout) { waitHello.await() } ?: throw Failure.NoHello()
            _state.update { it.copy(phase = Phase.Connected) }
            // Before the watch, so the pane's window is sized for this device first.
            sendViewport()
            refresh()
            // The task list is extra: a bridge that fails it still shows its sessions.
            runCatching { loadTasks() }
            loadWork()
            val before = _state.value
            before.watchedPane?.let { watch(it) }
            before.chat?.pane?.let {
                _state.update { s -> s.copy(chat = null) }
                follow(it)
            }
            before.doc?.let {
                _state.update { s -> s.copy(doc = null) }
                openDocument(it.pane, it.path)
            }
            before.diff?.let {
                _state.update { s -> s.copy(diff = null) }
                openDiff(it.pane, it.path, it.scope)
            }
            before.image?.let {
                _state.update { s -> s.copy(image = null) }
                openImage(it.pane, it.path)
            }
        } catch (e: Exception) {
            withContext(NonCancellable) { teardown(describe(e)) }
            throw e
        }
    }

    /** Sends `bye` and closes the channel. */
    suspend fun disconnect() = withContext(dispatcher) {
        if (channel != null) enqueue(Request.Bye(takeId()))
        teardown("disconnected")
    }

    // Requests

    suspend fun refresh() = withContext(dispatcher) {
        val reply = request { Request.List(it) }
        (reply as? Message.Result)?.sessions?.let { sessions -> _state.update { it.copy(sessions = sessions) } }
    }

    /** Lists the task records again, when the bridge keeps them; otherwise clears them. */
    suspend fun loadTasks() = withContext(dispatcher) {
        if (!_state.value.canKeepTasks) {
            _state.update { it.copy(tasks = null) }
            return@withContext
        }
        var requestId = 0L
        request { id ->
            requestId = id
            Request.ListTasks(id)
        }
        val listing = taskListings.remove(requestId) ?: return@withContext
        _state.update { it.copy(tasks = TaskBoard.of(listing)) }
    }

    /**
     * Records a Task and starts its agent. A failed launch keeps the record and throws
     * [Failure.LaunchFailed] with it; a refused request throws [Failure.Bridge] and keeps nothing.
     */
    suspend fun createTask(draft: TaskRecordDraft.Create): TaskRecord = withContext(dispatcher) {
        if (draft.loop != null) requireLoops()
        if (draft.worker != null) {
            requireWorkers()
            requireWorkerChoice(draft.worker, draft.workspace)
        }
        var requestId = 0L
        try {
            request { id ->
                requestId = id
                Request.CreateTask(
                    id, draft.workspace, draft.title, draft.goal, draft.project, draft.repo, draft.branch, draft.base, draft.loop, draft.worker,
                )
            }
        } catch (e: Failure.Bridge) {
            // `task-saved` before a failed result: the record exists, without a session.
            val kept = savedTasks.remove(requestId) ?: throw e
            keep(kept)
            throw Failure.LaunchFailed(kept, e.body)
        }
        val saved = savedTasks.remove(requestId)
            ?: throw Failure.Bridge(ErrorBody(ErrorCode.INTERNAL, "the bridge did not send the new task"))
        keep(saved)
        runCatching { refresh() }
        saved
    }

    /**
     * Sets [record]'s status. On `conflict` the list is loaded again before the failure is
     * thrown, so the user sees the record as it is now and can try again. Done and Drop remove
     * the worktree too; `would-lose-work` asks the user, who resends with [forceWorktree].
     */
    suspend fun updateTask(record: TaskRecord, status: TaskStatus, forceWorktree: Boolean = false): TaskRecord = withContext(dispatcher) {
        var requestId = 0L
        val worktree = worktreeEnd(status in ENDING_STATUSES, forceWorktree)
        try {
            request { id ->
                requestId = id
                Request.UpdateTask(id, record.id, record.rev, status, worktree)
            }
        } catch (e: Failure.Bridge) {
            savedTasks.remove(requestId)
            if (e.body.code == ErrorCode.CONFLICT) runCatching { loadTasks() }
            throw e
        }
        val saved = savedTasks.remove(requestId)
            ?: throw Failure.Bridge(ErrorBody(ErrorCode.INTERNAL, "the bridge did not send the saved task"))
        keep(saved)
        saved
    }

    /**
     * Restarts [record]'s agent in its pane, after a handoff (ADR 0018). [force] stops an agent
     * that is running. Throws [Failure.Bridge] with the bridge's reason, e.g. a running agent
     * without [force], or no agent pane; a bridge without `task-handoff` is never asked.
     */
    suspend fun restartAgent(record: TaskRecord, force: Boolean, worker: String? = null): TaskRecord = withContext(dispatcher) {
        if (!_state.value.canRestartAgents) {
            throw Failure.Bridge(ErrorBody(ErrorCode.UNSUPPORTED, "This host's bridge cannot restart agents. Update muxa-bridge."))
        }
        if (_state.value.tasks?.record(record.id)?.restartOperation?.blocksRestart == true) {
            throw Failure.Bridge(ErrorBody(ErrorCode.CONFLICT, "Resolve the interrupted restart before restarting again."))
        }
        if (worker != null) {
            requireWorkers()
            requireWorkerChoice(worker, record.workspace)
        }
        var requestId = 0L
        try {
            request { id ->
                requestId = id
                Request.RestartAgent(id, record.id, force, worker, if (_state.value.canUseWorkers) record.rev else null)
            }
        } catch (e: Failure.Bridge) {
            savedTasks.remove(requestId)?.let { keep(it) }
            if (e.body.code == ErrorCode.CONFLICT) runCatching { loadTasks() }
            throw e
        }
        val saved = savedTasks.remove(requestId)
            ?: throw Failure.Bridge(ErrorBody(ErrorCode.INTERNAL, "the bridge did not send the restarted task"))
        keep(saved)
        saved
    }

    /**
     * Starts [record]'s Task Loop with [spec], also a stopped or passed one. [reset] counts
     * attempts from zero. Throws [Failure.Bridge] with the host's reason, e.g. a check the
     * workspace does not allow, or no daemon; a bridge without `task-loop` is never asked.
     */
    suspend fun startLoop(record: TaskRecord, spec: LoopSpec, reset: Boolean = false): TaskRecord = withContext(dispatcher) {
        requireLoops()
        saving("the started loop") { id -> Request.StartLoop(id, record.id, spec, reset) }
    }

    /** Stops [record]'s Task Loop, with reason `user`. */
    suspend fun stopLoop(record: TaskRecord): TaskRecord = withContext(dispatcher) {
        requireLoops()
        saving("the stopped loop") { id -> Request.StopLoop(id, record.id) }
    }

    suspend fun recoverRestart(record: TaskRecord, operation: String, action: RestartRecoveryAction): TaskRecord = withContext(dispatcher) {
        if (!_state.value.canRecoverRestarts) {
            throw Failure.Bridge(ErrorBody(ErrorCode.UNSUPPORTED, "This host does not support restart recovery."))
        }
        val summary = record.restartOperation
        val current = _state.value.tasks?.record(record.id)?.restartOperation
        if (summary?.id != operation || action !in summary.recoveryActions || current?.id != operation ||
            action !in current.recoveryActions) {
            throw Failure.Bridge(ErrorBody(ErrorCode.CONFLICT, "Reload the task and review its recovery options again."))
        }
        try {
            saving("the recovered task") { Request.RecoverRestart(it, record.id, record.rev, operation, action) }
        } catch (e: Failure.Bridge) {
            runCatching { loadTasks() }
            throw e
        }
    }

    private fun requireWorkerChoice(name: String, workspace: String) {
        workerProblem(name, workspace, _state.value.tasks?.workers.orEmpty())?.let {
            throw Failure.Bridge(ErrorBody(ErrorCode.BAD_REQUEST, it))
        }
    }

    private fun requireWorkers() {
        if (!_state.value.canUseWorkers) throw Failure.Bridge(ErrorBody(ErrorCode.UNSUPPORTED, "This host does not support workers."))
    }

    private fun requireLoops() {
        if (!_state.value.canLoop) throw Failure.Bridge(ErrorBody(ErrorCode.UNSUPPORTED, LOOP_NEEDS_DAEMON))
    }

    /** Sends a request that answers with `task-saved`, and keeps the saved record, also one sent before a failure. */
    private suspend fun saving(what: String, make: (Long) -> Request): TaskRecord {
        var requestId = 0L
        try {
            request { id ->
                requestId = id
                make(id)
            }
        } catch (e: Failure.Bridge) {
            savedTasks.remove(requestId)?.let { keep(it) }
            throw e
        }
        val saved = savedTasks.remove(requestId)
            ?: throw Failure.Bridge(ErrorBody(ErrorCode.INTERNAL, "the bridge did not send $what"))
        keep(saved)
        return saved
    }

    private fun keep(saved: TaskRecord) = _state.update { s -> s.copy(tasks = (s.tasks ?: TaskBoard()).saving(saved)) }

    // Routines (`work` level 1, ADR 0026) and the Inbox items (`work` level 3, ADR 0033).

    /** Lists the Routines and the Inbox again. A failure lands in `workError`; the last lists stay. */
    suspend fun loadWork() = withContext(dispatcher) {
        if (!_state.value.canUseRoutines) return@withContext
        try {
            val routines = listing({ Request.ListRoutines(it) }) { routineListings.remove(it) }
            val inbox = listing({ Request.ListInbox(it) }) { inboxListings.remove(it) }
            _state.update { s ->
                s.copy(
                    routines = routines ?: s.routines,
                    // A host below `work` level 3 has no Inbox for this app.
                    inboxItems = if (s.canAnswerItems) inbox?.items ?: s.inboxItems else emptyList(),
                    workError = null,
                )
            }
        } catch (e: Failure) {
            _state.update { it.copy(workError = describe(e)) }
        }
    }

    /**
     * Starts a run of [routine] now. The bridge without the daemon refuses it as `unsupported`,
     * with a message that says what to run on the host. A refusal lists the Routines again,
     * so the host's state shows: a run already open, or a Routine disabled since.
     */
    suspend fun runRoutine(routine: RoutineInfo): RoutineInfo = withContext(dispatcher) {
        requireRoutines()
        var requestId = 0L
        try {
            request { id ->
                requestId = id
                Request.RunRoutine(id, routine.workspace, routine.name)
            }
        } catch (e: Failure) {
            savedRoutines.remove(requestId)
            loadWork()
            throw e
        }
        val saved = savedRoutines.remove(requestId)
        // The run's session starts with it.
        runCatching { refresh() }
        saved ?: throw Failure.Bridge(ErrorBody(ErrorCode.INTERNAL, "the bridge did not send the Routine"))
    }

    private fun requireRoutines() {
        if (!_state.value.canUseRoutines) throw Failure.Bridge(ErrorBody(ErrorCode.UNSUPPORTED, ROUTINES_UNSUPPORTED))
    }

    /** Sends a list request; its list arrives just before the result. Null when none came. */
    private suspend fun <T> listing(make: (Long) -> Request, take: (Long) -> T?): T? {
        var requestId = 0L
        try {
            request { id ->
                requestId = id
                make(id)
            }
        } catch (e: Failure) {
            take(requestId)
            throw e
        }
        return take(requestId)
    }

    /** A newer state of a listed Routine replaces it. */
    private fun keep(routine: RoutineInfo) = _state.update { s ->
        val same = { r: RoutineInfo -> r.workspace == routine.workspace && r.name == routine.name }
        s.copy(routines = if (s.routines.any(same)) s.routines.map { if (same(it)) routine else it } else s.routines + routine)
    }

    /** `worktree` for a request that may end a Task: none before level 7, so the worktree stays (ADR 0043). */
    private fun worktreeEnd(ends: Boolean, force: Boolean): WorktreeEnd? = when {
        !ends || !_state.value.endsWorktrees -> null
        force -> WorktreeEnd.FORCE
        else -> WorktreeEnd.REMOVE
    }

    /** A saved item replaces the listed one in its place; the host's next list orders it. */
    private fun keep(item: InboxItem) = _state.update { s ->
        if (s.inboxItems.none { it.id == item.id }) return@update s
        s.copy(inboxItems = s.inboxItems.map { if (it.id == item.id) item else it })
    }

    // Inbox items (`work` level 3, ADR 0033)

    /**
     * Answers [item] with [action]: [text] for `redo` and `change-course`, [option] for `choose`.
     * `later` takes no time (ADR 0040). Returns the item as the host saved it. A refusal lists the Inbox again,
     * since another device may have answered first; an `ask` whose menu changed is [Failure.ItemChanged].
     * Done and Drop end the Task with its worktree, as [updateTask] does with [forceWorktree].
     */
    suspend fun answer(
        item: InboxItem,
        action: InboxAction,
        text: String? = null,
        option: Int? = null,
        forceWorktree: Boolean = false,
    ): InboxItem = withContext(dispatcher) {
        if (!_state.value.canAnswerItems) throw Failure.Bridge(ErrorBody(ErrorCode.UNSUPPORTED, ITEMS_UNSUPPORTED))
        var requestId = 0L
        val worktree = worktreeEnd(action in ENDING_ACTIONS, forceWorktree)
        try {
            request { id ->
                requestId = id
                Request.UpdateInbox(id, item.id, action, text = text, option = option, worktree = worktree)
            }
        } catch (e: Failure) {
            savedItems.remove(requestId)
            loadWork()
            if (e is Failure.Bridge && e.body.code == ErrorCode.CONFLICT) throw Failure.ItemChanged()
            throw e
        }
        val saved = savedItems.remove(requestId)
            ?: throw Failure.Bridge(ErrorBody(ErrorCode.INTERNAL, "the bridge did not send the Inbox item"))
        // The host moved the item down (ADR 0040): its order is the list's.
        if (action == InboxAction.LATER) loadWork()
        // Accept, Drop, Retry and an approved proposal change a Task or start one.
        if (action in TASK_ACTIONS) {
            runCatching { refresh() }
            runCatching { loadTasks() }
        }
        saved
    }

    /**
     * Sends [item] and [text] to the workspace's Ask session (ADR 0041), which the host starts
     * when none is open, and lists the sessions again so the list has it. The item stays open.
     * Returns the session.
     */
    suspend fun askAgent(item: InboxItem, text: String): Message.SessionStarted = withContext(dispatcher) {
        if (!_state.value.canAnswerItems || InboxAction.ASK_AGENT !in item.answers) {
            throw Failure.Bridge(ErrorBody(ErrorCode.UNSUPPORTED, ASK_AGENT_UNSUPPORTED))
        }
        var requestId = 0L
        try {
            request { id ->
                requestId = id
                Request.UpdateInbox(id, item.id, InboxAction.ASK_AGENT, text = text)
            }
        } catch (e: Exception) {
            savedItems.remove(requestId)
            startedSessions.remove(requestId)
            throw e
        }
        savedItems.remove(requestId)
        // `session-started` arrives before `inbox-saved` and the result.
        val started = startedSessions.remove(requestId)
            ?: throw Failure.Bridge(ErrorBody(ErrorCode.INTERNAL, "The bridge did not name the Ask session."))
        refresh()
        started
    }

    /** Marks [item] read on every device. It stays open. Nothing is asked of a host without items, or for a read item. */
    suspend fun markRead(item: InboxItem) = withContext(dispatcher) {
        if (!_state.value.canAnswerItems || item.readAt != null) return@withContext
        // Read is a hint, not an answer: a failure leaves the dot, and the list is read again.
        runCatching { answer(item, InboxAction.READ) }
    }

    /** Shows this pane. Messages for any earlier watch are dropped from now on. */
    suspend fun watch(pane: String) = withContext(dispatcher) {
        _state.update { it.copy(watchedPane = pane, paneGoneReason = null) }
        request { id ->
            watchId = id
            Request.Watch(id, pane)
        }
    }

    /**
     * Follows the agent conversation in [pane]. A pane no adapter knows is remembered in
     * `unsupportedChats`; other failures end up in `chat.ended`.
     */
    suspend fun follow(pane: String) = withContext(dispatcher) {
        val s = _state.value
        if (!s.canChat || pane in s.unsupportedChats) return@withContext
        if (s.chat?.pane == pane && s.chat.ended == null) return@withContext
        _state.update { it.copy(chat = ChatLog(pane)) }
        try {
            request { id ->
                chatId = id
                Request.Chat(id, pane)
            }
        } catch (e: Failure.Bridge) {
            if (e.body.code == ErrorCode.UNSUPPORTED) {
                _state.update {
                    it.copy(unsupportedChats = it.unsupportedChats + pane, chat = it.chat.takeIf { c -> c?.pane != pane })
                }
            } else {
                endChat(pane, describe(e))
            }
        } catch (e: Failure) {
            endChat(pane, describe(e))
        }
    }

    private fun endChat(pane: String, reason: String) =
        _state.update { s -> s.copy(chat = s.chat?.let { if (it.pane == pane) it.copy(ended = reason) else it }) }

    suspend fun unfollow() = withContext(dispatcher) {
        if (_state.value.chat == null) return@withContext
        _state.update { it.copy(chat = null) }
        chatId = null
        runCatching { request { Request.Unchat(it) } }
    }

    /** The pane's scrollback, then its screen, once (capability `history`). Null when none came. */
    suspend fun history(pane: String, lines: Int? = null): Message.History? = withContext(dispatcher) {
        var requestId = 0L
        request { id ->
            requestId = id
            Request.History(id, pane, lines)
        }
        histories.remove(requestId)
    }

    /** The pane's documents: its agent's first, then its repo's. */
    suspend fun documents(pane: String): Message.DocList = withContext(dispatcher) {
        var requestId = 0L
        request { id ->
            requestId = id
            Request.Docs(id, pane)
        }
        docListings.remove(requestId) ?: Message.DocList(docs = requestId, entries = emptyList())
    }

    /** Opens a listed document and follows its changes. A failure ends up in `doc.ended`. */
    suspend fun openDocument(pane: String, path: String) = withContext(dispatcher) {
        val s = _state.value
        if (!s.canReadDocs) return@withContext
        if (s.doc?.pane == pane && s.doc.path == path && s.doc.ended == null) return@withContext
        _state.update { it.copy(doc = DocLog(pane, path)) }
        try {
            request { id ->
                docId = id
                Request.Doc(id, pane, path)
            }
        } catch (e: Failure) {
            _state.update { st -> st.copy(doc = st.doc?.let { if (it.path == path) it.copy(ended = describe(e)) else it }) }
        }
    }

    suspend fun closeDocument() = withContext(dispatcher) {
        if (_state.value.doc == null) return@withContext
        _state.update { it.copy(doc = null) }
        docId = null
        runCatching { request { Request.Undoc(it) } }
    }

    /** The files of the pane's repo that differ from HEAD. */
    suspend fun changes(pane: String): Message.ChangeList = withContext(dispatcher) {
        var requestId = 0L
        request { id ->
            requestId = id
            Request.Changes(id, pane)
        }
        changeListings.remove(requestId) ?: Message.ChangeList(changes = requestId, entries = emptyList())
    }

    /**
     * Opens a listed file's diff and follows it. A failure ends up in `diff.ended`.
     * A branch diff needs the `branch-changes` capability.
     */
    suspend fun openDiff(pane: String, path: String, scope: DiffScope = DiffScope.UNCOMMITTED) = withContext(dispatcher) {
        val s = _state.value
        if (!s.canShowChanges) return@withContext
        if (s.diff?.pane == pane && s.diff.path == path && s.diff.scope == scope && s.diff.ended == null) return@withContext
        _state.update { it.copy(diff = DiffLog(pane, path, scope)) }
        try {
            request { id ->
                diffId = id
                // Absent for uncommitted, so older bridges understand it.
                Request.Diff(id, pane, path, if (scope == DiffScope.BRANCH) DiffScope.BRANCH else null)
            }
        } catch (e: Failure) {
            _state.update { st -> st.copy(diff = st.diff?.let { if (it.path == path) it.copy(ended = describe(e)) else it }) }
        }
    }

    suspend fun closeDiff() = withContext(dispatcher) {
        if (_state.value.diff == null) return@withContext
        _state.update { it.copy(diff = null) }
        diffId = null
        runCatching { request { Request.Undiff(it) } }
    }

    /** The image files of the pane's repo, newest first. */
    suspend fun images(pane: String): Message.ImageList = withContext(dispatcher) {
        var requestId = 0L
        request { id ->
            requestId = id
            Request.Images(id, pane)
        }
        imageListings.remove(requestId) ?: Message.ImageList(images = requestId, entries = emptyList())
    }

    /** Opens a listed image and follows it. A failure ends up in `image.ended`. */
    suspend fun openImage(pane: String, path: String) = withContext(dispatcher) {
        val s = _state.value
        if (!s.canShowImages) return@withContext
        if (s.image?.pane == pane && s.image.path == path && s.image.ended == null) return@withContext
        _state.update { it.copy(image = ImageLog(pane, path)) }
        try {
            request { id ->
                imageId = id
                Request.Image(id, pane, path)
            }
        } catch (e: Failure) {
            _state.update { st -> st.copy(image = st.image?.let { if (it.path == path) it.copy(ended = describe(e)) else it }) }
        }
    }

    suspend fun closeImage() = withContext(dispatcher) {
        if (_state.value.image == null) return@withContext
        _state.update { it.copy(image = null) }
        imageId = null
        runCatching { request { Request.Unimage(it) } }
    }

    /** Closes a task's session, then lists again. Throws the bridge's reason when nothing was closed. */
    suspend fun closeTask(session: String, mode: CloseMode) = withContext(dispatcher) {
        request { Request.CloseTask(it, session, mode) }
        refresh()
    }

    /**
     * Closes the ask session [s], then lists again (ADR 0039). Its agent stops; the main
     * checkout and its files stay. A session without `ask` is never sent.
     */
    suspend fun closeAskSession(s: Session) = withContext(dispatcher) {
        if (!_state.value.canCloseAskSessions || s.ask == null) {
            throw Failure.Bridge(ErrorBody(ErrorCode.UNSUPPORTED, CLOSE_ASK_UNSUPPORTED))
        }
        request { Request.CloseTask(it, s.id, CloseMode.KEEP) }
        refresh()
    }

    /** Starts a task on [branch] of the repo at [repo], and returns its session's name once listed. */
    suspend fun newTask(repo: String, branch: String, base: String?): String = withContext(dispatcher) {
        var requestId = 0L
        request { id ->
            requestId = id
            Request.NewTask(id, repo, branch, base)
        }
        // The name arrives just before the result.
        val name = createdTasks.remove(requestId)
            ?: throw Failure.Bridge(ErrorBody(ErrorCode.INTERNAL, "the bridge did not name the task's session"))
        refresh()
        name
    }

    /**
     * Starts an ask session: [workspace]'s agent in [repo]'s main checkout, with no Task
     * (ADR 0031). Returns the session once the session list has it.
     */
    suspend fun startSession(workspace: String, repo: String): Message.SessionStarted = withContext(dispatcher) {
        if (!_state.value.canAsk) throw Failure.Bridge(ErrorBody(ErrorCode.UNSUPPORTED, ASK_UNSUPPORTED))
        var requestId = 0L
        try {
            request { id ->
                requestId = id
                Request.StartSession(id, workspace, repo)
            }
        } catch (e: Exception) {
            startedSessions.remove(requestId)
            throw e
        }
        // `session-started` arrives just before the result.
        val started = startedSessions.remove(requestId)
            ?: throw Failure.Bridge(ErrorBody(ErrorCode.INTERNAL, "The bridge did not name the session."))
        refresh()
        started
    }

    /**
     * The pane area changed (rotation, split screen, fold, keyboard). The bridge gets the
     * first viewport at once and later ones once they settle for 300 ms, so a drag or an
     * animation resizes the window once (ADR 0020).
     */
    fun setViewport(viewport: PaneViewport) {
        scope.launch {
            if (viewport == _state.value.viewport) return@launch
            _state.update { it.copy(viewport = viewport) }
            viewportJob?.cancel()
            if (sentViewport == null) {
                sendViewport()
                return@launch
            }
            viewportJob = scope.launch {
                delay(VIEWPORT_SETTLE)
                sendViewport()
            }
        }
    }

    private fun sendViewport() {
        val viewport = _state.value.viewport ?: return
        if (viewport == sentViewport || channel == null || "viewport" !in _state.value.capabilities) return
        sentViewport = viewport
        val id = takeId()
        inputs += id
        enqueue(Request.Viewport(id, viewport.cols, viewport.rows))
    }

    /** Asks for the watched pane's whole screen again, e.g. after the view lost it. */
    suspend fun resync() = withContext(dispatcher) {
        val pane = _state.value.watchedPane ?: return@withContext
        runCatching { watch(pane) }
    }

    fun send(keys: List<String>) = fire { id, pane -> Request.Keys(id, pane, keys) }
    fun send(text: String) = fire { id, pane -> Request.Text(id, pane, text) }
    fun send(bytes: ByteArray) = fire { id, pane -> Request.RawBytes(id, pane, Bytes(bytes)) }
    fun paste(text: String) = fire { id, pane -> Request.Paste(id, pane, text) }

    /** Sends what a key or the input method produced. */
    fun send(output: KeyOutput) {
        when (output) {
            is KeyOutput.Keys -> send(output.keys)
            is KeyOutput.Text -> send(output.text)
            KeyOutput.None -> {}
        }
    }

    /** Clears [State.lastError] once the UI has shown it. */
    fun clearError() = _state.update { it.copy(lastError = null) }

    /**
     * Input for the watched pane. Its result is not awaited; a failure lands in
     * `lastError`. Launched on the one-at-a-time dispatcher, so it keeps call order.
     */
    private fun fire(make: (Long, String) -> Request) {
        scope.launch {
            val pane = _state.value.watchedPane ?: return@launch
            if (channel == null) return@launch
            val req = make(takeId(), pane)
            inputs += req.id
            enqueue(req)
        }
    }

    private suspend fun request(make: (Long) -> Request): Message {
        if (channel == null) throw Failure.NotConnected()
        val req = make(takeId())
        // Registered before the line is queued, so the result cannot arrive first.
        val reply = CompletableDeferred<Message>()
        pending[req.id] = reply
        enqueue(req)
        return reply.await()
    }

    // Plumbing

    private fun takeId(): Long = ++nextId

    private fun enqueue(req: Request) {
        outbox?.trySend(Wire.encode(req))
    }

    private fun start(channel: BridgeChannel) {
        this.channel = channel
        val lines = Channel<String>(Channel.UNLIMITED)
        outbox = lines
        // One writer keeps requests in the order they were made.
        writer = scope.launch {
            for (line in lines) {
                try {
                    channel.send(line)
                } catch (_: TransportError) {
                    break
                }
            }
        }
        jobs = listOf(
            scope.launch {
                try {
                    for (line in channel.lines) receive(line)
                    channelClosed()
                } catch (e: Exception) {
                    if (e is kotlinx.coroutines.CancellationException) throw e
                    teardown(describe(e))
                }
            },
            scope.launch {
                while (true) {
                    delay(pingEvery)
                    if (this@PaneSession.channel == null) return@launch
                    enqueue(Request.Ping(takeId()))
                }
            },
        )
    }

    /** Closed before hello means the bridge command exited, most often because it is not installed. */
    private suspend fun channelClosed() {
        val waiting = hello
        if (waiting == null) {
            teardown("Connection closed.")
            return
        }
        hello = null
        waiting.completeExceptionally(Failure.NoHello())
        teardown(describe(Failure.NoHello()))
    }

    private fun receive(line: String) {
        val message = try {
            Wire.decodeMessage(line)
        } catch (_: SerializationException) {
            return
        } catch (_: IllegalArgumentException) {
            return
        }
        when (message) {
            is Message.Hello -> {
                if (message.protocol == PROTOCOL_VERSION) {
                    _state.update { it.copy(tmuxVersion = message.tmux, capabilities = message.capabilities) }
                    hello?.complete(Unit)
                } else {
                    hello?.completeExceptionally(Failure.ProtocolMismatch(message.protocol))
                }
                hello = null
            }
            is Message.Result -> result(message)
            is Message.Screen -> if (message.watch == watchId) renderer?.reset(message.cols, message.rows, message.data.array)
            is Message.Output -> if (message.watch == watchId) renderer?.feed(message.data.array)
            is Message.ChatEntries -> if (message.chat == chatId) _state.update { it.copy(chat = it.chat?.apply(message)) }
            is Message.ChatEnded -> if (message.chat == chatId) _state.update { it.copy(chat = it.chat?.copy(ended = message.reason)) }
            is Message.DocList -> docListings[message.docs] = message
            is Message.History -> histories[message.history] = message
            is Message.ChangeList -> changeListings[message.changes] = message
            is Message.ImageList -> imageListings[message.images] = message
            is Message.TaskCreated -> createdTasks[message.task] = message.session
            is Message.TaskList -> taskListings[message.tasks] = message
            is Message.DocContent -> if (message.doc == docId) {
                _state.update { it.copy(doc = it.doc?.copy(text = message.text, mtime = message.mtime, ended = null)) }
            }
            is Message.DocEnded -> if (message.doc == docId) _state.update { it.copy(doc = it.doc?.copy(ended = message.reason)) }
            is Message.DiffContent -> if (message.diff == diffId) {
                _state.update { it.copy(diff = it.diff?.copy(text = message.text, binary = message.binary, ended = null)) }
            }
            is Message.DiffEnded -> if (message.diff == diffId) _state.update { it.copy(diff = it.diff?.copy(ended = message.reason)) }
            is Message.ImageContent -> if (message.image == imageId) {
                _state.update { it.copy(image = it.image?.copy(data = message.data.array, mtime = message.mtime, ended = null)) }
            }
            is Message.ImageEnded -> if (message.image == imageId) _state.update { it.copy(image = it.image?.copy(ended = message.reason)) }
            is Message.PaneGone -> if (message.watch == watchId) {
                _state.update { it.copy(paneGoneReason = message.reason ?: "the pane closed") }
            }
            Message.SessionsChanged -> scope.launch {
                runCatching { refresh() }
                // A session that starts or ends may run a Task, and another device may have changed one.
                runCatching { loadTasks() }
                loadWork()
            }
            Message.TasksChanged -> scope.launch {
                runCatching { loadTasks() }
                // A Routine run is a task record too, so its Routine may have changed.
                loadWork()
            }
            is Message.Bye -> scope.launch { teardown(message.reason) }
            is Message.TaskSaved -> savedTasks[message.task] = message.record
            is Message.RoutineList -> routineListings[message.routines] = message.entries
            is Message.RoutineSaved -> {
                savedRoutines[message.routine] = message.entry
                keep(message.entry)
            }
            is Message.InboxList -> inboxListings[message.inbox] = message
            is Message.InboxSaved -> {
                message.item?.let {
                    savedItems[message.inbox] = it
                    keep(it)
                }
            }
            is Message.SessionStarted -> startedSessions[message.start] = message
            is Message.Unknown -> {}
        }
    }

    private fun result(message: Message.Result) {
        val id = message.id
        when {
            id == null -> _state.update { it.copy(lastError = message.error?.message) }
            inputs.remove(id) -> message.error?.let { e -> _state.update { it.copy(lastError = e.message) } }
            else -> {
                val reply = pending.remove(id) ?: return
                val error = message.error
                if (error != null) reply.completeExceptionally(Failure.Bridge(error)) else reply.complete(message)
            }
        }
    }

    private suspend fun teardown(reason: String?) {
        val channel = channel
        if (channel == null) {
            // Keep the first reason: a failed connect tears down twice.
            if (reason != null && _state.value.phase == Phase.Connecting) {
                _state.update { it.copy(phase = Phase.Disconnected(reason)) }
            }
            return
        }
        // Everything shared is reset before the first suspension: a new connect can start
        // while this one drains, and must not lose its own state to it.
        this.channel = null
        _state.update { it.copy(phase = Phase.Disconnected(reason ?: "disconnected")) }
        val running = jobs
        jobs = emptyList()
        val writer = writer
        this.writer = null
        // Let queued lines (a final `bye`) go out, but do not wait on a dead link.
        outbox?.close()
        outbox = null
        watchId = null
        chatId = null
        docId = null
        diffId = null
        imageId = null
        docListings.clear()
        histories.clear()
        changeListings.clear()
        imageListings.clear()
        createdTasks.clear()
        taskListings.clear()
        savedTasks.clear()
        routineListings.clear()
        inboxListings.clear()
        savedRoutines.clear()
        savedItems.clear()
        startedSessions.clear()
        inputs.clear()
        sentViewport = null
        viewportJob?.cancel()
        viewportJob = null
        val failed = pending.values.toList()
        pending.clear()
        failed.forEach { it.completeExceptionally(Failure.NotConnected()) }
        hello?.completeExceptionally(Failure.NotConnected())
        hello = null
        withContext(NonCancellable) {
            if (writer != null && withTimeoutOrNull(1.seconds) { writer.join() } == null) writer.cancel()
            channel.close()
        }
        running.forEach { it.cancel() }
    }

    companion object {
        /** Shown when the host cannot start ask sessions. */
        const val ASK_UNSUPPORTED = "This host cannot start ask sessions. Update muxa on the host."

        /** Shown when the host cannot close ask sessions. */
        const val CLOSE_ASK_UNSUPPORTED = "This host cannot close ask sessions. Update muxa on the host."

        /** How long a viewport must stay before the bridge gets it. */
        private val VIEWPORT_SETTLE = 300.milliseconds

        /** The answers after which the Task list and the sessions are read again. */
        private val TASK_ACTIONS = setOf(InboxAction.ACCEPT, InboxAction.DROP, InboxAction.RETRY, InboxAction.APPROVE)

        /** The statuses and the answers that end a Task, and so take `worktree` (ADR 0043). */
        private val ENDING_STATUSES = setOf(TaskStatus.DONE, TaskStatus.DROPPED)
        private val ENDING_ACTIONS = setOf(InboxAction.ACCEPT, InboxAction.DROP)

        /** Shown when an `ask` item's menu changed since it was listed. */
        const val ITEM_CHANGED = "The agent now asks something else. The Inbox shows it as it is now; answer again."

        /** Shown when the host cannot send an item to the workspace's agent (ADR 0041). */
        const val ASK_AGENT_UNSUPPORTED = "This host cannot send an item to the workspace's agent. Update muxa on the host."

        /** Shown when the host keeps no Inbox items. */
        const val ITEMS_UNSUPPORTED = "This host has no Inbox items. Update muxa on the host."

        /** Shown when the host has no Routines. */
        const val ROUTINES_UNSUPPORTED = "This host has no Routines. Update muxa on the host."

        /** A message for the user, saying what to do next where there is something to do. */
        fun describe(error: Throwable): String = when (error) {
            is Failure.Bridge ->
                if (error.body.code == ErrorCode.CONFLICT) {
                    "This task changed on another device. The list shows it as it is now; try again."
                } else {
                    error.body.message
                }
            is Failure.LaunchFailed -> "The task was saved, but its agent did not start: ${error.body.message}"
            is Failure.ItemChanged -> ITEM_CHANGED
            is Failure.ProtocolMismatch ->
                "The bridge speaks protocol ${error.bridge}; this app speaks $PROTOCOL_VERSION. Update one of them."
            is Failure.NoHello ->
                "The bridge did not start. Check that muxa-bridge is installed on the host and that the bridge command points to it."
            is Failure.NotConnected -> "Not connected."
            is TransportError.HostKeyMismatch -> "The host key changed or is not trusted yet."
            is TransportError.NotAuthorized -> "The host does not accept this device's key yet. Set it up with your password."
            is TransportError -> error.message ?: "The connection failed."
            else -> error.message ?: error.toString()
        }
    }
}
