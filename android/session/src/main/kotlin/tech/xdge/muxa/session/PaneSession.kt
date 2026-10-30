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
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.PROTOCOL_VERSION
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.terminal.PaneRenderer
import tech.xdge.muxa.transport.BridgeChannel
import tech.xdge.muxa.transport.TransportError
import kotlin.time.Duration
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
    ) {
        val canChat get() = "chat" in capabilities
        val canReadDocs get() = "docs" in capabilities
        val canShowChanges get() = "changes" in capabilities
        val canShowImages get() = "images" in capabilities
        val canStartTasks get() = "tasks" in capabilities
        val canCloseTasks get() = "close-task" in capabilities
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
    private val changeListings = mutableMapOf<Long, Message.ChangeList>()
    private val imageListings = mutableMapOf<Long, Message.ImageList>()
    private val createdTasks = mutableMapOf<Long, String>()
    private val pending = mutableMapOf<Long, CompletableDeferred<Message>>()

    /** Ids of input requests in flight; their results only matter when they fail. */
    private val inputs = mutableSetOf<Long>()
    private var hello: CompletableDeferred<Unit>? = null

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
                    unsupportedChats = emptySet(), doc = null, diff = null, image = null,
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
            refresh()
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
            is Message.ChangeList -> changeListings[message.changes] = message
            is Message.ImageList -> imageListings[message.images] = message
            is Message.TaskCreated -> createdTasks[message.task] = message.session
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
            Message.SessionsChanged -> scope.launch { runCatching { refresh() } }
            is Message.Bye -> scope.launch { teardown(message.reason) }
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
        changeListings.clear()
        imageListings.clear()
        createdTasks.clear()
        inputs.clear()
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
        /** A message for the user, saying what to do next where there is something to do. */
        fun describe(error: Throwable): String = when (error) {
            is Failure.Bridge -> error.body.message
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
