package tech.xdge.muxa.app

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import tech.xdge.muxa.demo.DemoBridge
import tech.xdge.muxa.protocol.CloseMode
import tech.xdge.muxa.protocol.DiffScope
import tech.xdge.muxa.protocol.DocInfo
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.session.AgentAlert
import tech.xdge.muxa.session.ChatLog
import tech.xdge.muxa.session.Host
import tech.xdge.muxa.session.HostStore
import tech.xdge.muxa.session.LastPane
import tech.xdge.muxa.session.PaneFit
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.RepoGroup
import tech.xdge.muxa.session.TaskDraft
import tech.xdge.muxa.session.alerts
import tech.xdge.muxa.session.counterpart
import tech.xdge.muxa.session.defaultPane
import tech.xdge.muxa.session.groupedByRepo
import tech.xdge.muxa.session.knownRepos
import tech.xdge.muxa.session.resuming
import tech.xdge.muxa.transport.BridgeChannel
import tech.xdge.muxa.transport.HostKey
import tech.xdge.muxa.transport.KeySetup
import tech.xdge.muxa.transport.SshBridge
import tech.xdge.muxa.transport.SshTarget
import tech.xdge.muxa.transport.TransportError
import java.io.File
import kotlin.time.Duration.Companion.seconds

/** App state: the saved hosts, the connection, and which pane is shown. */
class AppModel(context: Context, private val scope: CoroutineScope) {
    val session = PaneSession(Dispatchers.Main.immediate)
    private val keys = ClientKeyStore(context)
    private var savedHosts = HostStore(File(context.filesDir, "hosts.json"))

    /** The saved hosts, or during the demo its one host, kept in memory only. */
    var hosts = savedHosts
        private set

    /** A Compose copy of the session's state, so every view that reads [state] recomposes with it. */
    private var mirror by mutableStateOf(PaneSession.State())

    /**
     * The session's state now. The copy above can lag by a dispatch, and the model acts
     * right after a request returns, so the value comes from the session itself.
     */
    val state: PaneSession.State
        get() {
            mirror
            return session.state.value
        }

    private var hostsMirror by mutableStateOf(HostStore.Contents())

    /** The saved hosts now; read like [state], so views recompose and logic never lags. */
    val hostList: HostStore.Contents
        get() {
            hostsMirror
            return hosts.contents.value
        }
    var storageError by mutableStateOf<String?>(null)
        private set

    /** The host the session belongs to: connected, connecting, or last tried. */
    var currentHostId by mutableStateOf<String?>(null)
        private set
    var connecting by mutableStateOf(false)
        private set

    /** Logging in with the password to add this device's key, before connecting. */
    var addingKey by mutableStateOf(false)
        private set

    /** A host key waiting for the user's decision on first use (or after it changed). */
    var untrustedKey by mutableStateOf<HostKey?>(null)
        private set
    var connectError by mutableStateOf<String?>(null)

    /** What the user can do about [connectError], when the app knows. */
    var connectFix by mutableStateOf<ConnectFix?>(null)
        private set
    var showSwitcher by mutableStateOf(false)

    /** Bumped when a dialog closes, so the pane takes the keyboard back. */
    var focusRequest by mutableIntStateOf(0)
        private set

    /** Bumped when the user asks for the detail: on a phone it is pushed over the list. */
    var detailRequest by mutableIntStateOf(0)
        private set

    /** Agent panes the user switched to Raw. Every other agent pane shows Pretty when it can. */
    var rawPanes by mutableStateOf(emptySet<String>())
        private set
    var showReader by mutableStateOf(false)

    /** The Terminal Size the user picked, by menu or pinch. Null: the view picks. Kept through a fold. */
    var paneFit by mutableStateOf<PaneFit?>(null)

    /** The New Task sheet, with the repo it starts in. */
    var newTask by mutableStateOf<TaskDraft?>(null)

    /** An agent elsewhere that wants attention, shown for a few seconds. */
    var alert by mutableStateOf<AgentAlert?>(null)
        private set

    /** The task waiting for the user to pick how to close it. */
    var closingTask by mutableStateOf<Session?>(null)

    /** Why the last close failed; nothing was closed then. */
    var closeError by mutableStateOf<String?>(null)
    var docListing by mutableStateOf<Message.DocList?>(null)
        private set
    var docError by mutableStateOf<String?>(null)
        private set

    /** The shown pane's uncommitted changes, as last listed. */
    var changeListing by mutableStateOf<Message.ChangeList?>(null)
        private set

    /** The shown pane's images, while the Reader is open. */
    var imageListing by mutableStateOf<Message.ImageList?>(null)
        private set

    /** The Reader's open tab, kept while the app runs. */
    var readerTab by mutableStateOf(ReaderTab.DOCS)

    /** The in-app demo bridge instead of SSH: Try the Demo, or a debug launch for the UI tests. */
    var demo = false
        private set

    /** What to do once the user trusts the presented host key. A setup password is held only meanwhile. */
    private sealed interface AfterTrust {
        data object Connect : AfterTrust
        class SetUp(val password: String) : AfterTrust
    }

    private var afterTrust: AfterTrust? = null

    /** The user pressed Disconnect: returning to the foreground does not reconnect. */
    private var stayDisconnected = false

    /** Asked for from the session switcher: shown once the switcher has closed. */
    private var newTaskAfterSwitcher: TaskDraft? = null

    /** The document last read in each session, opened again next time. */
    private val lastDocs = mutableMapOf<String, String>()
    private var demoAlerts = false
    private var demoSet = DemoBridge.Content.SHOWCASE

    /** The demo bridge serves this host only; a host added during the demo uses SSH. */
    private var demoHostId: String? = null

    /** The user started the demo, so the list offers to leave it. A debug launch starts it for good. */
    var canLeaveDemo by mutableStateOf(false)
        private set

    init {
        scope.launch {
            session.state.collect { new ->
                val old = mirror
                mirror = new
                if (old.sessions != new.sessions) sessionsChanged(old.sessions, new.sessions)
            }
        }
        watchHosts()
    }

    private fun watchHosts() {
        val store = hosts
        scope.launch { store.contents.collect { if (hosts === store) hostsMirror = it } }
        scope.launch { store.storageError.collect { if (hosts === store) storageError = it } }
    }

    /** Try the Demo: sample sessions from the in-app bridge. Nothing is saved, and no SSH or key is touched. */
    fun tryDemo() {
        beginDemo(DemoBridge.Content.SHOWCASE, alerts = true, Host(name = "Demo", target = SshTarget(host = "demo.invalid", user = "demo")))
        canLeaveDemo = true
    }

    /** A debug launch for the UI tests: their [set], and no way out of it. */
    fun startDemo(set: DemoBridge.Content, alerts: Boolean) {
        // The store screenshots show the list: a machine's name there, not the demo's.
        val host = if (set == DemoBridge.Content.SHOWCASE) {
            Host(name = "Mac mini", target = SshTarget(host = "mac-mini.local", user = "you"))
        } else {
            Host(name = "Demo", target = SshTarget(host = "demo.invalid", user = "demo"))
        }
        beginDemo(set, alerts, host)
    }

    /** One demo host, nothing saved. */
    private fun beginDemo(set: DemoBridge.Content, alerts: Boolean, host: Host) {
        // Each start begins over: UI tests share one process and so one model.
        resetHostState()
        demo = true
        canLeaveDemo = false
        demoSet = set
        demoAlerts = alerts
        hosts = HostStore(null)
        watchHosts()
        demoHostId = host.id
        hosts.upsert(host)
        hosts.markUsed(host.id)
        scope.launch {
            session.disconnect()
            connectNow(host.id)
        }
    }

    /** Back to the saved hosts, disconnected. */
    fun leaveDemo() {
        resetHostState()
        demo = false
        canLeaveDemo = false
        demoHostId = null
        hosts = savedHosts
        watchHosts()
        scope.launch { session.disconnect() }
    }

    /** Debug launches only: no saved hosts, as on a fresh install, whatever the device saved. */
    fun forgetSavedHosts() {
        savedHosts = HostStore(null)
        leaveDemo()
    }

    /** What belonged to the last host's connection. */
    private fun resetHostState() {
        rawPanes = emptySet()
        paneFit = null
        showReader = false
        showSwitcher = false
        newTask = null
        closingTask = null
        closeError = null
        alert = null
        connectError = null
        connectFix = null
        stayDisconnected = false
        currentHostId = null
        lastDocs.clear()
    }

    // Connection

    val isConnected: Boolean get() = state.phase == PaneSession.Phase.Connected
    val currentHost: Host? get() = hostList.hosts.firstOrNull { it.id == currentHostId }

    fun connect(id: String) = scope.launch { connectNow(id) }

    private suspend fun connectNow(id: String) {
        val host = hosts.host(id) ?: return
        val sameHost = id == currentHostId
        currentHostId = id
        hosts.markUsed(id)
        stayDisconnected = false
        connectError = null
        connectFix = null
        connecting = true
        detailRequest++
        try {
            session.connect(samePanes = sameHost) { open(host) }
            openDefaultPane()
        } catch (e: TransportError.HostKeyMismatch) {
            afterTrust = AfterTrust.Connect
            untrustedKey = e.presented
        } catch (e: Exception) {
            fail(e)
        } finally {
            connecting = false
        }
    }

    private fun fail(error: Throwable) {
        connectError = PaneSession.describe(error)
        connectFix = ConnectFix.of(error)
    }

    private suspend fun open(host: Host): BridgeChannel {
        if (demo && host.id == demoHostId) return DemoBridge(demoSet, demoAlerts)
        return SshBridge.connect(host.target, keys.loadOrCreate())
    }

    fun trust(key: HostKey) = scope.launch {
        untrustedKey = null
        val next = afterTrust
        afterTrust = null
        val host = currentHost ?: return@launch
        val trusted = host.copy(target = host.target.copy(hostKey = key))
        hosts.upsert(trusted)
        when (next) {
            is AfterTrust.SetUp -> setUpKeyNow(trusted.id, next.password)
            else -> connectNow(trusted.id)
        }
    }

    fun cancelTrust() {
        if (afterTrust is AfterTrust.SetUp) {
            connectError = "The host key was not trusted, so this device's key was not added."
            connectFix = ConnectFix.ADD_KEY
        }
        untrustedKey = null
        afterTrust = null
        refocusPane()
    }

    /** Logs in once with [password], adds this device's key to the host, then connects. */
    fun setUpKey(id: String, password: String) = scope.launch { setUpKeyNow(id, password) }

    private suspend fun setUpKeyNow(id: String, password: String) {
        val host = hosts.host(id) ?: return
        currentHostId = id
        connectError = null
        connectFix = null
        addingKey = true
        try {
            val line = KeySetup.authorizedLine(keys.loadOrCreate(), host.target.command, KEY_COMMENT)
            KeySetup.install(line, host.target, password)
        } catch (e: TransportError.HostKeyMismatch) {
            afterTrust = AfterTrust.SetUp(password)
            untrustedKey = e.presented
            return
        } catch (e: TransportError.NotAuthorized) {
            connectError = "The host refused the password. Check the user and password."
            connectFix = ConnectFix.ADD_KEY
            return
        } catch (e: Exception) {
            fail(e)
            return
        } finally {
            addingKey = false
        }
        connectNow(id)
    }

    fun disconnect() = scope.launch {
        stayDisconnected = true
        session.disconnect()
    }

    /** Leaving for the background: close cleanly so the bridge exits, and come back later. */
    suspend fun suspend() = session.disconnect()

    /** Launch or return to the foreground: reconnect to the last host and pane. */
    suspend fun resume() {
        if (stayDisconnected || isConnected || connecting || addingKey) return
        val id = currentHostId ?: hostList.lastUsed ?: return
        if (hosts.host(id) == null) return
        connectNow(id)
    }

    /**
     * A pane view takes the watched pane's bytes from now on. The screen sent before it
     * existed went nowhere, so the bridge sends it again.
     */
    fun attach(renderer: tech.xdge.muxa.terminal.PaneRenderer) {
        session.renderer = renderer
        scope.launch { session.resync() }
    }

    fun refocusPane() {
        focusRequest++
    }

    /** The public key line for `~/.ssh/authorized_keys`, restricted to [command]. */
    fun authorizedKeyLine(command: String): String? =
        runCatching { KeySetup.authorizedLine(keys.loadOrCreate(), command, KEY_COMMENT) }.getOrNull()

    // Panes

    /** Sessions with a pane to show, grouped by repo like the desktop's session list. */
    val sessionGroups: List<RepoGroup> get() = state.sessions.filter { it.defaultPane != null }.groupedByRepo()

    /** The sessions in the order shown, which the session shortcuts follow. */
    val visibleSessions: List<Session> get() = sessionGroups.flatMap { it.sessions }

    val currentSession: Session?
        get() {
            val pane = state.watchedPane ?: return null
            return state.sessions.firstOrNull { s -> s.panes.any { it.id == pane } }
        }

    val currentPane: Pane?
        get() {
            val id = state.watchedPane ?: return null
            return currentSession?.panes?.firstOrNull { it.id == id }
        }

    val counterpart: Pane? get() = state.watchedPane?.let { currentSession?.counterpart(it) }

    fun open(s: Session) = scope.launch { openNow(s) }

    private suspend fun openNow(s: Session) {
        val pane = s.defaultPane ?: return
        detailRequest++
        watch(pane.id)
    }

    fun toggleAgentTerminal() = scope.launch { counterpart?.let { watch(it.id) } }

    /** Moves [offset] sessions through the list, wrapping around. */
    fun step(offset: Int) = scope.launch {
        val list = visibleSessions
        if (list.isEmpty()) return@launch
        val index = list.indexOfFirst { it.id == currentSession?.id }.coerceAtLeast(0)
        openNow(list[(index + offset + list.size) % list.size])
    }

    /** The nth session, 1-based. */
    fun openSession(number: Int) = scope.launch {
        visibleSessions.getOrNull(number - 1)?.let { openNow(it) }
    }

    /** The pane last shown on this host, else the first session. */
    private suspend fun openDefaultPane() {
        if (state.watchedPane != null) return
        val last = currentHostId?.let { hostList.lastPanes[it] }
        val pane = last?.let { state.sessions.resuming(it) }
        if (pane != null) watch(pane.id) else visibleSessions.firstOrNull()?.let { openNow(it) }
    }

    private suspend fun watch(pane: String) {
        try {
            session.watch(pane)
        } catch (e: PaneSession.Failure) {
            connectError = PaneSession.describe(e)
            return
        }
        val id = currentHostId
        val name = currentSession?.name
        if (id != null && name != null) hosts.remember(LastPane(name, pane), id)
        changeListing = null
        scope.launch { loadChanges() }
        // The pane stays watched in Pretty, so Raw is instant and pane-gone still arrives.
        val current = currentPane
        if (current != null && current.isAgentSide) session.follow(current.id) else session.unfollow()
    }

    // Pretty and Raw

    /** What the pane area shows: the agent as a chat or as its terminal, or the shell. */
    enum class PaneView(val title: String) { CHAT("Chat"), AGENT("Agent"), SHELL("Shell") }

    /** The agent side and the shell side of the shown session, whichever is open. */
    val agentPane: Pane? get() = currentPane?.let { if (it.isAgentSide) it else counterpart }
    val shellPane: Pane? get() = currentPane?.let { if (it.isAgentSide) counterpart else it }

    val paneView: PaneView
        get() {
            val pane = currentPane
            if (pane == null || !pane.isAgentSide) return PaneView.SHELL
            return if (showsPretty) PaneView.CHAT else PaneView.AGENT
        }

    /** The views the shown session has; Chat only where an adapter knows the agent. */
    val paneViews: List<PaneView>
        get() = buildList {
            agentPane?.let {
                if (chatPossible(it)) add(PaneView.CHAT)
                add(PaneView.AGENT)
            }
            if (shellPane != null) add(PaneView.SHELL)
        }

    fun show(view: PaneView) = scope.launch {
        when (view) {
            PaneView.CHAT, PaneView.AGENT -> {
                val agent = agentPane ?: return@launch
                if (agent.id != currentPane?.id) watch(agent.id)
                setPretty(view == PaneView.CHAT)
            }
            PaneView.SHELL -> {
                val shell = shellPane ?: return@launch
                if (shell.id != currentPane?.id) watch(shell.id)
            }
        }
    }

    private fun chatPossible(pane: Pane) = pane.isAgentSide && state.canChat && pane.id !in state.unsupportedChats

    /** The agent pane has a conversation the bridge can follow. */
    val canShowPretty: Boolean get() = currentPane?.let(::chatPossible) ?: false

    val showsPretty: Boolean get() = canShowPretty && currentPane?.id !in rawPanes

    fun setPretty(pretty: Boolean) {
        val pane = currentPane ?: return
        if (!canShowPretty) return
        rawPanes = if (pretty) rawPanes - pane.id else rawPanes + pane.id
        refocusPane()
    }

    /** The chat for the shown pane, once the bridge follows it. */
    val chat: ChatLog? get() = state.chat?.takeIf { it.pane == currentPane?.id }

    /** Sends a prompt as one paste, so a multi-line prompt stays one prompt, then Enter. */
    fun sendPrompt(text: String) {
        session.paste(text)
        session.send(listOf("Enter"))
    }

    fun retryChat() = scope.launch { currentPane?.let { session.follow(it.id) } }

    // Reader

    val canReadDocs: Boolean get() = state.canReadDocs && currentPane != null

    fun openReader() {
        if (!canReadDocs) return
        if (readerTab !in readerTabs) readerTab = ReaderTab.DOCS
        showReader = true
    }

    /** The tabs the bridge can fill. */
    val readerTabs: List<ReaderTab>
        get() = ReaderTab.entries.filter {
            when (it) {
                ReaderTab.DOCS -> true
                ReaderTab.CHANGES -> state.canShowChanges
                ReaderTab.IMAGES -> state.canShowImages
            }
        }

    /** The Reader at the agent's plan, e.g. while it asks to leave plan mode. */
    fun openPlan() {
        if (!canReadDocs) return
        currentSession?.id?.let { lastDocs.remove(it) }
        readerTab = ReaderTab.DOCS
        showReader = true
    }

    fun closeReader() {
        showReader = false
        scope.launch {
            session.closeDocument()
            session.closeDiff()
            session.closeImage()
            loadChanges()
        }
        refocusPane()
    }

    fun loadDocs() = scope.launch {
        val pane = currentPane ?: return@launch
        docError = null
        try {
            docListing = session.documents(pane.id)
        } catch (e: PaneSession.Failure) {
            docError = PaneSession.describe(e)
        }
        loadChanges()
        loadImages()
    }

    /** Lists the shown pane's images; a failure only hides them. */
    private suspend fun loadImages() {
        val pane = currentPane
        if (!state.canShowImages || pane == null) {
            imageListing = null
            return
        }
        val listing = runCatching { session.images(pane.id) }.getOrNull()
        if (currentPane?.id == pane.id) imageListing = listing
    }

    fun openImage(path: String) = scope.launch {
        val pane = currentPane ?: return@launch
        session.closeDocument()
        session.closeDiff()
        session.openImage(pane.id, path)
    }

    /** Lists the shown pane's changes; a failure only hides them. */
    private suspend fun loadChanges() {
        val pane = currentPane
        if (!state.canShowChanges || pane == null) {
            changeListing = null
            return
        }
        val listing = runCatching { session.changes(pane.id) }.getOrNull()
        // Another pane may be shown by now.
        if (currentPane?.id == pane.id) changeListing = listing
    }

    val changeCount: Int get() = changeListing?.entries?.size ?: 0

    fun openDiff(path: String, scope: DiffScope) = this.scope.launch {
        val pane = currentPane ?: return@launch
        session.closeDocument()
        session.closeImage()
        session.openDiff(pane.id, path, scope)
    }

    /** The last one read in this session, else the plan, else the README, else the first. */
    val preferredDoc: String?
        get() {
            val entries = docListing?.entries.orEmpty()
            if (entries.isEmpty()) return null
            val last = currentSession?.id?.let { lastDocs[it] }
            if (last != null && entries.any { it.path == last }) return last
            return entries.firstOrNull { it.kind == DocInfo.Kind.PLAN }?.path
                ?: entries.firstOrNull { it.path.lowercase() == "readme.md" }?.path
                ?: entries.first().path
        }

    fun openDoc(path: String) = scope.launch {
        val pane = currentPane ?: return@launch
        currentSession?.id?.let { lastDocs[it] = path }
        session.closeDiff()
        session.closeImage()
        session.openDocument(pane.id, path)
    }

    // Tasks

    val canStartTasks: Boolean get() = isConnected && state.canStartTasks

    /** Opens the New Task sheet in [repoDir], else the shown session's repo. */
    fun startNewTask(repoDir: String? = null) {
        if (!canStartTasks) return
        val dir = repoDir ?: currentSession?.repoDir ?: state.sessions.knownRepos.firstOrNull()?.dir ?: ""
        // One sheet at a time.
        if (showSwitcher) {
            newTaskAfterSwitcher = TaskDraft(repoDir = dir)
            showSwitcher = false
            switcherClosed()
        } else {
            newTask = TaskDraft(repoDir = dir)
        }
    }

    fun switcherClosed() {
        val draft = newTaskAfterSwitcher
        if (draft != null) {
            newTaskAfterSwitcher = null
            newTask = draft
        } else {
            refocusPane()
        }
    }

    /** Starts the task and shows its session. Throws what the bridge said went wrong. */
    suspend fun start(draft: TaskDraft) {
        val req = draft.request ?: return
        val name = session.newTask(req.repo, req.branch, req.base)
        newTask = null
        state.sessions.firstOrNull { it.name == name }?.let { openNow(it) }
    }

    val canCloseTasks: Boolean get() = isConnected && state.canCloseTasks

    /** Only a session with a worktree is a task. */
    fun canClose(s: Session) = canCloseTasks && s.worktree != null

    fun close(s: Session, mode: CloseMode) = scope.launch {
        closingTask = null
        val wasShown = s.id == currentSession?.id
        try {
            session.closeTask(s.id, mode)
        } catch (e: PaneSession.Failure) {
            closeError = PaneSession.describe(e)
            return@launch
        }
        if (wasShown) visibleSessions.firstOrNull()?.let { openNow(it) }
        refocusPane()
    }

    // Alerts

    /** Tells about agents elsewhere that now wait, finished, or failed. */
    private fun sessionsChanged(old: List<Session>, new: List<Session>) {
        val latest = new.alerts(old, currentSession?.id).lastOrNull() ?: return
        alert = latest
        scope.launch {
            delay(ALERT_TIME)
            if (alert == latest) alert = null
        }
    }

    fun openAlert() = scope.launch {
        val shown = alert ?: return@launch
        alert = null
        state.sessions.firstOrNull { it.id == shown.session }?.let { openNow(it) }
    }

    companion object {
        const val KEY_COMMENT = "muxa-android"
        private val ALERT_TIME = 4.seconds
    }
}

enum class ReaderTab(val title: String) { DOCS("Docs"), CHANGES("Changes"), IMAGES("Images") }

/** The next step a connect error points to. */
enum class ConnectFix {
    /** Log in once with the password to add this device's key. */
    ADD_KEY,

    /** Install muxa-bridge on the host, or fix the bridge command. */
    INSTALL_BRIDGE;

    companion object {
        fun of(error: Throwable): ConnectFix? = when (error) {
            is TransportError.NotAuthorized -> ADD_KEY
            is PaneSession.Failure.NoHello -> INSTALL_BRIDGE
            else -> null
        }
    }
}

/** The agent side of a session: tagged as the agent, or untagged and not at a shell. */
val Pane.isAgentSide: Boolean
    get() = when (role) {
        Role.AGENT -> true
        Role.SHELL -> false
        Role.HIDDEN, Role.UNKNOWN -> !shellForeground
    }
