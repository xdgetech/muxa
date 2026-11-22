package tech.xdge.muxa.app.next

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import tech.xdge.muxa.app.ClientKeyStore
import tech.xdge.muxa.session.Host
import tech.xdge.muxa.session.HostStore
import tech.xdge.muxa.transport.ByteChannel
import tech.xdge.muxa.transport.HostKey
import tech.xdge.muxa.transport.KeySetup
import tech.xdge.muxa.transport.SshExec
import tech.xdge.muxa.transport.TransportError
import tech.xdge.muxa.v2.Capture
import tech.xdge.muxa.v2.ChatEntry
import tech.xdge.muxa.v2.ChatPrompt
import tech.xdge.muxa.v2.DemoChannel
import tech.xdge.muxa.v2.DiffScope
import tech.xdge.muxa.v2.Inbox
import tech.xdge.muxa.v2.LoopInfo
import tech.xdge.muxa.v2.LoopSpec
import tech.xdge.muxa.v2.ReaderContent
import tech.xdge.muxa.v2.ReaderKind
import tech.xdge.muxa.v2.ReaderList
import tech.xdge.muxa.v2.Routine
import tech.xdge.muxa.v2.RoutineProblem
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.V2Client
import tech.xdge.muxa.v2.V2Error
import tech.xdge.muxa.v2.V2Event
import tech.xdge.muxa.v2.V2Request
import tech.xdge.muxa.v2.Workspace
import tech.xdge.muxa.v2.WorktreeChoice
import java.time.LocalDate
import kotlin.time.Duration.Companion.seconds

/**
 * The app on protocol v2 (ADR 0066, 0067): one connection to a host's muxa
 * server, resumed after a network change or a return from the background.
 * It keeps the Inbox, the terminals and the Routines current from events.
 * Its state is read on the main thread; [scope] runs there.
 */
class NextModel(
    val scope: CoroutineScope,
    val hosts: HostStore,
    private val keys: ClientKeyStore,
    private val clientName: String,
) {
    sealed interface Link {
        data object Offline : Link
        data object Connecting : Link
        data object Connected : Link

        /** The connection is gone; it is resumed on its own. */
        data class Reconnecting(val why: String) : Link
        data class Failed(val why: String) : Link
    }

    var link: Link by mutableStateOf(Link.Offline)
        private set
    var hostId: String? by mutableStateOf(null)
        private set
    var demo by mutableStateOf(false)
        private set

    /** A host key the user has not trusted yet; [trust] pins it. */
    var untrustedKey: HostKey? by mutableStateOf(null)
        private set

    /** Logging in with the password to add this device's key. */
    var addingKey by mutableStateOf(false)
        private set
    var inbox by mutableStateOf(Inbox.EMPTY)
        private set
    var terminals by mutableStateOf(emptyList<Terminal>())
        private set
    var routines by mutableStateOf(emptyList<Routine>())
        private set
    var routineProblems by mutableStateOf(emptyList<RoutineProblem>())
        private set

    /** Why the last request failed, shown until the next one. */
    var requestError: String? by mutableStateOf(null)

    /** Followed conversations by chat number. */
    var chats by mutableStateOf(emptyMap<Int, List<ChatEntry>>())
        private set
    var chatPrompts by mutableStateOf(emptyMap<Int, ChatPrompt>())
        private set

    /** Why a chat ended, until it is closed. */
    var chatEnded by mutableStateOf(emptyMap<Int, String>())
        private set

    /** Task Loops by agent terminal. */
    var loops by mutableStateOf(emptyMap<String, LoopInfo>())
        private set

    /** Open Reader files by reader number, as the server last sent them. */
    var readerContents by mutableStateOf(emptyMap<Int, ReaderContent>())
        private set

    /** Why a Reader file ended, by reader number, until it is closed. */
    var readerEnded by mutableStateOf(emptyMap<Int, String>())
        private set

    /** The password of a key setup that waits for the user to trust the host key. */
    private var setUpAfterTrust: String? = null

    /** The user pressed Disconnect: coming back to the foreground does not connect. */
    private var stayDisconnected = false
    private var client: V2Client? = null
    private var resume = V2Client.State()
    private var events: Job? = null
    private var retry: Job? = null

    /** Where each attached channel's events go (Focus). */
    private val sinks = mutableMapOf<Int, (V2Event) -> Unit>()

    /** The channel each shown terminal is attached on, for Chat's input. */
    private val channelOf = mutableMapOf<String, Int>()

    /** Agents waiting on the user: a short-lived state, never an Inbox item. */
    val needsYou: List<Terminal>
        get() = terminals.filter { it.live && it.isTaskAgent && it.status?.state in WAITING }

    val isConnected: Boolean get() = link == Link.Connected
    val currentHost: Host? get() = hosts.host(hostId)

    // Connecting

    fun connect(id: String) = scope.launch { connectNow(id) }

    private suspend fun connectNow(id: String) {
        val host = hosts.host(id) ?: return
        stayDisconnected = false
        if (hostId != id) forget()
        hostId = id
        demo = false
        hosts.markUsed(id)
        open { SshExec.connect(host.target, keys.loadOrCreate(), host.target.command + " connect") }
    }

    /** Try the Demo: the in-app server, no host. */
    fun startDemo() = scope.launch { startDemoNow() }

    private suspend fun startDemoNow() {
        stayDisconnected = false
        forget()
        hostId = null
        demo = true
        open { DemoChannel() }
    }

    /** Pins the presented host key, then goes on with what it stopped. */
    fun trust() = scope.launch {
        val key = untrustedKey ?: return@launch
        val host = currentHost ?: return@launch
        hosts.upsert(host.copy(target = host.target.copy(hostKey = key)))
        untrustedKey = null
        val password = setUpAfterTrust
        setUpAfterTrust = null
        if (password != null) setUpKeyNow(host.id, password) else connectNow(host.id)
    }

    fun cancelTrust() {
        untrustedKey = null
        setUpAfterTrust = null
        link = Link.Offline
    }

    /**
     * Logs in once with the password to add this device's key, restricted to
     * the bridge, then connects. The password is not kept.
     */
    fun setUpKey(id: String, password: String) = scope.launch { setUpKeyNow(id, password) }

    private suspend fun setUpKeyNow(id: String, password: String) {
        val host = hosts.host(id) ?: return
        hostId = id
        addingKey = true
        try {
            val line = KeySetup.authorizedLine(keys.loadOrCreate(), host.target.command, KEY_COMMENT)
            KeySetup.install(line, host.target, password)
        } catch (e: TransportError.HostKeyMismatch) {
            setUpAfterTrust = password
            untrustedKey = e.presented
            return
        } catch (e: TransportError.NotAuthorized) {
            link = Link.Failed("The host refused the password. Check the user and password.")
            return
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            link = Link.Failed(describe(e))
            return
        } finally {
            addingKey = false
        }
        connectNow(id)
    }

    /** The `authorized_keys` line for this device, to add by hand. */
    fun authorizedKeyLine(command: String): String? =
        runCatching { KeySetup.authorizedLine(keys.loadOrCreate(), command, KEY_COMMENT) }.getOrNull()

    fun disconnect() = scope.launch {
        stayDisconnected = true
        retry?.cancel()
        drop()
        link = Link.Offline
    }

    /** Leaves for the background: the server keeps the session for a resume. */
    suspend fun suspend() {
        retry?.cancel()
        drop()
        if (link == Link.Connected) link = Link.Offline
    }

    /** Back in the foreground, or the network came back. */
    suspend fun resumeIfNeeded() {
        if (client != null || link == Link.Connecting || stayDisconnected) return
        if (demo) return startDemoNow()
        hostId?.let { connectNow(it) }
    }

    /** A window's start: resume what it showed, else connect to the host last used. */
    suspend fun resumeOrLastHost() {
        if (client != null || link == Link.Connecting || stayDisconnected) return
        if (demo || hostId != null) return resumeIfNeeded()
        hosts.contents.value.lastUsed?.let { connectNow(it) }
    }

    private suspend fun open(channel: suspend () -> ByteChannel) {
        retry?.cancel()
        drop()
        link = Link.Connecting
        try {
            val c = V2Client.open(channel(), clientName, resume)
            client = c
            listen(c)
            c.watch()
            link = Link.Connected
            reload()
        } catch (e: TransportError.HostKeyMismatch) {
            untrustedKey = e.presented
            link = Link.Failed("Check the host key before you trust it.")
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            link = Link.Failed(describe(e))
        }
    }

    private fun listen(c: V2Client) {
        events = scope.launch {
            for (e in c.events) handle(e, c)
        }
    }

    private suspend fun handle(e: V2Event, c: V2Client) {
        when (e) {
            is V2Event.InboxChanged -> inbox = e.inbox
            is V2Event.Changed -> {
                val at = terminals.indexOfFirst { it.id == e.terminal.id }
                terminals = if (at < 0) terminals + e.terminal else terminals.toMutableList().also { it[at] = e.terminal }
            }
            is V2Event.Removed -> terminals = terminals.filter { it.id != e.terminal }
            V2Event.Resync -> terminals = runCatching { c.terminals() }.getOrDefault(terminals)
            is V2Event.Output -> sinks[e.channel]?.invoke(e)
            is V2Event.Paint -> sinks[e.channel]?.invoke(e)
            is V2Event.Closed -> sinks[e.channel]?.invoke(e)
            is V2Event.Chat -> chats[e.chat]?.let { entries ->
                chats = chats + (e.chat to e.update.appliedTo(entries))
                chatPrompts = e.update.prompt?.let { chatPrompts + (e.chat to it) } ?: (chatPrompts - e.chat)
            }
            is V2Event.ChatEnded -> {
                chats = chats - e.chat
                chatPrompts = chatPrompts - e.chat
                chatEnded = chatEnded + (e.chat to e.reason)
            }
            is V2Event.Loops -> loops = e.loops.associateBy { it.terminal }
            is V2Event.Reader -> if (e.reader in readerContents) readerContents = readerContents + (e.reader to e.content)
            is V2Event.ReaderEnded -> {
                readerContents = readerContents - e.reader
                readerEnded = readerEnded + (e.reader to e.reason)
            }
            is V2Event.Lost -> {
                // The server closes chats and Reader files with the connection.
                chatEnded = chatEnded + chats.keys.associateWith { "The connection was lost. Open the chat again." }
                chats = emptyMap()
                chatPrompts = emptyMap()
                readerEnded = readerEnded + readerContents.keys.associateWith { "The connection was lost. Open the file again." }
                readerContents = emptyMap()
                resume = c.state()
                client = null
                link = Link.Reconnecting(e.reason)
                reconnectSoon()
            }
        }
    }

    /** Tries again with growing waits while the connection is down. */
    private fun reconnectSoon() {
        retry = scope.launch {
            var wait = 1.seconds
            while (isActive) {
                delay(wait)
                if (client != null) return@launch
                resumeIfNeeded()
                if (isConnected) return@launch
                wait = minOf(wait * 2, 30.seconds)
            }
        }
    }

    private suspend fun reload() = request { c ->
        inbox = c.inbox()
        terminals = c.terminals()
        c.routines().let { (r, p) -> routines = r; routineProblems = p }
        loops = c.loops().associateBy { it.terminal }
    }

    private suspend fun drop() {
        events?.cancel()
        client?.let {
            resume = it.state()
            it.close()
        }
        client = null
    }

    /** A new host or the demo: nothing of the old session carries over. */
    private fun forget() {
        resume = V2Client.State()
        sinks.clear()
        inbox = Inbox.EMPTY
        terminals = emptyList()
        routines = emptyList()
        routineProblems = emptyList()
        readerContents = emptyMap()
        readerEnded = emptyMap()
        loops = emptyMap()
        chats = emptyMap()
        chatPrompts = emptyMap()
        chatEnded = emptyMap()
        channelOf.clear()
    }

    // Requests

    /** Runs an Inbox request; the reply replaces the Inbox. */
    fun inbox(r: V2Request) = scope.launch { inboxNow(r) }

    suspend fun inboxNow(r: V2Request) = request { inbox = it.inbox(r) }

    suspend fun parseCapture(text: String): Capture? {
        var parsed: Capture? = null
        request { parsed = it.parseCapture(text, today()) }
        return parsed
    }

    fun runRoutine(r: Routine) = scope.launch {
        request {
            it.runRoutine(r.workspace, r.name)
            it.routines().let { (list, problems) -> routines = list; routineProblems = problems }
        }
    }

    // Task actions (ADR 0072)

    /**
     * Closes a Review item's Task. Returns the server's message when closing it
     * would lose work and [force] was not given; other failures go to [requestError].
     */
    suspend fun approve(item: String, force: Boolean): String? {
        var loss: String? = null
        val choice = if (force) WorktreeChoice.FORCE else WorktreeChoice.REMOVE
        request { c ->
            try {
                inbox = c.inbox(V2Request.InboxApprove(item, choice))
            } catch (e: V2Error.Refused) {
                if (e.code != WOULD_LOSE_WORK) throw e
                loss = e.message?.removeSuffix(" (${e.code})") ?: e.code
            }
        }
        return loss
    }

    /** Runs a Failed item's Routine or Task again; true when it did. */
    suspend fun retry(item: String): Boolean {
        var ok = false
        request { c ->
            inbox = c.inbox(V2Request.InboxRetry(item))
            c.routines().let { (list, problems) -> routines = list; routineProblems = problems }
            ok = true
        }
        return ok
    }

    suspend fun workspaces(): List<Workspace>? {
        var list: List<Workspace>? = null
        request { list = it.workspaces() }
        return list
    }

    /** Starts a Task; its agent terminal, or null with [requestError] set. */
    suspend fun startTask(draft: TaskDraft): Terminal? {
        var started: Terminal? = null
        request { c ->
            val (t, _) = c.startTask(draft.workspace, draft.title, draft.goal, draft.places, draft.connections, draft.mcp, draft.plan, draft.loop)
            if (terminals.none { it.id == t.id }) terminals = terminals + t
            started = t
        }
        return started
    }

    fun refreshRoutines() = scope.launch {
        request { it.routines().let { (list, problems) -> routines = list; routineProblems = problems } }
    }

    // Chat

    /** Follows the conversation of a Task's agent; it stays in [chats] until [closeChat]. */
    suspend fun openChat(terminal: String): Int? {
        var opened: Int? = null
        request {
            val (n, update) = it.chatOpen(terminal)
            chats = chats + (n to update.appliedTo(emptyList()))
            update.prompt?.let { p -> chatPrompts = chatPrompts + (n to p) }
            opened = n
        }
        return opened
    }

    fun closeChat(n: Int) {
        val open = n in chats
        chats = chats - n
        chatPrompts = chatPrompts - n
        chatEnded = chatEnded - n
        if (open) scope.launch { runCatching { client?.chatClose(n) } }
    }

    /**
     * Types [text] into the terminal through the channel Focus shows it on.
     * Attaching only for this would resize the agent's terminal.
     */
    fun send(text: String, terminal: String) {
        val ch = channelOf[terminal]
        if (ch == null) {
            requestError = "Show the Task's terminal to answer it."
            return
        }
        requestError = null
        input(ch, text.toByteArray())
    }

    // Task Loop

    /** Starts or restarts the loop of a Task's agent. */
    suspend fun startLoop(terminal: String, spec: LoopSpec, reset: Boolean) = request {
        loops = loops + (terminal to it.loopStart(terminal, spec, reset))
    }

    fun stopLoop(terminal: String) = scope.launch {
        request { loops = loops + (terminal to it.loopStop(terminal)) }
    }

    // Reader

    /** The files of [kind] in the terminal's folder; documents with HTML ones. */
    suspend fun readerList(terminal: String, kind: ReaderKind): ReaderList? {
        var list: ReaderList? = null
        request { list = it.readerList(terminal, kind, html = kind == ReaderKind.DOCS) }
        return list
    }

    /** Opens a listed file; its content stays in [readerContents] until [closeReader]. */
    suspend fun openReader(terminal: String, kind: ReaderKind, path: String, scope: DiffScope): Int? {
        var opened: Int? = null
        request {
            val (n, content) = it.readerOpen(terminal, kind, path, html = kind == ReaderKind.DOCS, scope = scope)
            readerContents = readerContents + (n to content)
            opened = n
        }
        return opened
    }

    fun closeReader(n: Int) {
        val open = n in readerContents
        readerContents = readerContents - n
        readerEnded = readerEnded - n
        if (open) scope.launch { runCatching { client?.readerClose(n) } }
    }

    private suspend fun request(body: suspend (V2Client) -> Unit) {
        val c = client
        if (c == null) {
            requestError = "Not connected."
            return
        }
        try {
            requestError = null
            body(c)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            requestError = describe(e)
        }
    }

    // Terminals (Focus)

    /** Attaches [terminal]; its events go to [sink] until [detach]. */
    suspend fun attach(terminal: String, cols: Int, rows: Int, sink: (V2Event) -> Unit): Int? {
        val c = client ?: return null
        return try {
            c.attach(terminal, cols, rows, HISTORY).also {
                sinks[it] = sink
                channelOf[terminal] = it
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            requestError = describe(e)
            null
        }
    }

    fun detach(channel: Int) = scope.launch {
        sinks.remove(channel)
        channelOf.entries.removeAll { it.value == channel }
        runCatching { client?.detach(channel) }
    }

    fun input(channel: Int, bytes: ByteArray) = scope.launch { runCatching { client?.input(channel, bytes) } }

    fun resize(channel: Int, cols: Int, rows: Int) = scope.launch { client?.resize(channel, cols, rows) }

    companion object {
        private val WAITING = setOf("blocked", "waiting")
        private const val KEY_COMMENT = "muxa-android"
        private const val HISTORY = 2000
        private const val WOULD_LOSE_WORK = "would-lose-work"

        /** The device's local date, `YYYY-MM-DD`, as Capture reads `due`. */
        fun today(): String = LocalDate.now().toString()

        fun describe(e: Throwable): String = when (e) {
            is V2Error.Refused if e.code == "version" ->
                "The host runs an older muxa: ${e.message}. Update it with make install on the host."
            is V2Error.Refused -> e.message ?: e.code
            is V2Error.Lost -> "The connection was lost: ${e.message}"
            is TransportError.NotAuthorized -> "The host refused this device's key. Add it in the host's settings."
            else -> e.message ?: e.toString()
        }
    }
}
