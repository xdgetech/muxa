package tech.xdge.muxa.app

import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
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
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WORK_ITEMS
import tech.xdge.muxa.protocol.WorkspaceChecks
import tech.xdge.muxa.session.AgentAlert
import tech.xdge.muxa.session.ChatLog
import tech.xdge.muxa.session.Host
import tech.xdge.muxa.session.HostStore
import tech.xdge.muxa.session.InboxSection
import tech.xdge.muxa.session.LastPane
import tech.xdge.muxa.session.LoopDraft
import tech.xdge.muxa.session.PaneFit
import tech.xdge.muxa.session.PaneSession
import tech.xdge.muxa.session.RepoGroup
import tech.xdge.muxa.session.TaskDraft
import tech.xdge.muxa.session.TaskRecordDraft
import tech.xdge.muxa.session.alerts
import tech.xdge.muxa.session.containing
import tech.xdge.muxa.session.counterpart
import tech.xdge.muxa.session.defaultPane
import tech.xdge.muxa.session.groupedByRepo
import tech.xdge.muxa.session.waiting
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.app.reader.ReaderItem
import tech.xdge.muxa.session.inbox
import tech.xdge.muxa.session.runningTask
import tech.xdge.muxa.session.knownRepos
import tech.xdge.muxa.session.resuming
import tech.xdge.muxa.session.manual
import tech.xdge.muxa.transport.BridgeChannel
import tech.xdge.muxa.transport.HostKey
import tech.xdge.muxa.transport.KeySetup
import tech.xdge.muxa.transport.SshBridge
import tech.xdge.muxa.transport.SshTarget
import tech.xdge.muxa.transport.TransportError
import kotlin.time.Duration.Companion.seconds

/**
 * One window's state: its connection and which pane it shows. The saved hosts and the
 * device key are the app's, shared by every window (ADR 0029).
 */
class AppModel(
    val scope: CoroutineScope,
    saved: HostStore,
    private val keys: ClientKeyStore,
    private val modes: ModeStore = ModeStore(null),
) {
    val session = PaneSession(Dispatchers.Main.immediate)
    private var savedHosts = saved

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

    private var modeState by mutableStateOf(modes.mode)

    /** What the sidebar shows (ADR 0035). The device remembers it for the next window. */
    var mode: SidebarMode
        get() = modeState
        set(value) {
            modeState = value
            modes.mode = value
        }

    /** What the detail shows when no pane is shown over it. Kept through a fold or a resize. */
    var selection by mutableStateOf<Selection?>(null)

    /** The detail shows the watched pane, over [selection] when it has one. */
    var paneShown by mutableStateOf(false)

    /** The sidebar shows: on a phone the list instead of the detail, elsewhere beside it. */
    var listShown by mutableStateOf(true)

    /** Settings: the hosts, the demo and the version. */
    var showSettings by mutableStateOf(false)

    /** The window has the phone's stack, so a selection pushes its screen. Set by the layout. */
    var phoneLayout = false

    /** The view each session showed last on this device, by session id: a Sessions row opens it again. */
    private val lastViews = mutableMapOf<String, PaneView>()

    /** Agent panes the user switched to Raw. Every other agent pane shows Pretty when it can. */
    var rawPanes by mutableStateOf(emptySet<String>())
        private set
    var showReader by mutableStateOf(false)

    /** The Terminal Size the user picked, by menu or pinch. Null: the view picks. Kept through a fold. */
    var paneFit by mutableStateOf<PaneFit?>(null)

    /** The old New Task sheet, with the repo it starts in: only for a bridge without task records. */
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

    val selectedItem: String? get() = (selection as? Selection.Item)?.id
    val selectedTask: String? get() = (selection as? Selection.Task)?.id
    val selectedRoutine: String? get() = (selection as? Selection.Routine)?.key

    /** An Inbox action that asks first: Later's times, a picked time, or an approval. */
    var inboxPrompt by mutableStateOf<InboxPrompt?>(null)

    /** Why the last Inbox action failed, from the host. */
    var inboxError by mutableStateOf<String?>(null)

    /** The Inbox item the keys act on, by id. */
    var itemFocus by mutableStateOf<String?>(null)

    /** The reply box's text, per item, so a fold or the Reader keeps a draft. */
    var itemReplies by mutableStateOf<Map<String, String>>(emptyMap())

    /** The item whose answer is on its way; its buttons wait. */
    var answeringItem by mutableStateOf<String?>(null)

    /** The host keeps the Inbox as a list of questions, so the Inbox screen shows its items. */
    val usesItems: Boolean get() = isConnected && state.canAnswerItems

    /** The items that wait for the user, in the host's order. */
    val inboxItems: List<InboxItem> get() = if (usesItems) state.inboxItems.waiting() else emptyList()

    val canUseRoutines: Boolean get() = isConnected && state.canUseRoutines

    /** Inbox mode, at item [id] when it waits: an item's, or a Task's from a session's Show in Inbox. */
    fun showInbox(id: String? = null) {
        showMode(SidebarMode.INBOX)
        val target = id?.let { i -> inboxItems.firstOrNull { it.id == i || it.task == i }?.id }
        itemFocus = target ?: inboxItems.firstOrNull()?.id
        target?.let { openItem(it) }
    }

    /** Tasks mode at the Routines. */
    fun showRoutines() {
        showMode(SidebarMode.TASKS)
        select(Selection.Routines)
    }

    /** [s]'s agent terminal: a Routine run's. */
    fun openTerminal(s: Session) = scope.launch { openNow(s, PaneView.AGENT) }

    /** [s]'s pane in [view], over the selection: an item's or a Task's Chat or Terminal (ADR 0035). */
    fun openPane(s: Session, view: PaneView) = scope.launch { openNow(s, view) }

    /** The New Task sheet for a task record, from any New Task entry point. */
    var newTaskRecord by mutableStateOf<TaskRecordDraft?>(null)

    /** Why the last ask session from a repo header did not start, from the bridge. */
    var askError by mutableStateOf<String?>(null)

    /** Why the last status change failed, e.g. another device changed the Task first. */
    var taskError by mutableStateOf<String?>(null)

    /** The Task whose agent restart waits for the user's confirmation, by id. */
    var restartingTask by mutableStateOf<String?>(null)

    /** Why the last agent restart failed, from the bridge. */
    var restartError by mutableStateOf<String?>(null)

    /** The Start loop sheet: the Task, by id, and the loop it edits. */
    var startingLoop by mutableStateOf<Pair<String, LoopDraft>?>(null)

    /** Why the last loop start or stop failed, from the bridge. */
    var loopError by mutableStateOf<String?>(null)

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
    private var newTaskAfterSwitcher: (() -> Unit)? = null

    /** The document last read in each session, opened again next time. */
    private val lastDocs = mutableMapOf<String, String>()
    private var demoAlerts = false
    private var demoTaskRecords = true
    private var demoWorkLevel = WORK_ITEMS
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
        showSettings = false
        demoTaskRecords = true
        demoWorkLevel = WORK_ITEMS
        beginDemo(DemoBridge.Content.SHOWCASE, alerts = true, Host(name = "Demo", target = SshTarget(host = "demo.invalid", user = "demo")))
        canLeaveDemo = true
    }

    /** A debug launch for the UI tests: their [set], and no way out of it. */
    fun startDemo(
        set: DemoBridge.Content,
        alerts: Boolean,
        taskRecords: Boolean = true,
        workLevel: Int = WORK_ITEMS,
    ) {
        demoTaskRecords = taskRecords
        demoWorkLevel = workLevel
        // The store screenshots show the list: a machine's name there, not the demo's.
        val host = if (set == DemoBridge.Content.SHOWCASE) {
            Host(name = "Mac mini", target = SshTarget(host = "mac-mini.local", user = "you"))
        } else {
            Host(name = "Demo", target = SshTarget(host = "demo.invalid", user = "demo"))
        }
        beginDemo(set, alerts, host)
    }

    /** What a new window needs to show this window's pane on its host. */
    class WindowStart(val host: Host, val pane: String?, val demo: Demo?) {
        class Demo(
            val set: DemoBridge.Content,
            val alerts: Boolean,
            val taskRecords: Boolean,
            val canLeave: Boolean,
            val workLevel: Int = WORK_ITEMS,
        )
    }

    /** This window's host and pane, for a new window; null when no host is shown. */
    fun windowStart(): WindowStart? {
        val host = currentHost ?: return null
        val demo = if (demo && host.id == demoHostId) {
            WindowStart.Demo(demoSet, demoAlerts, demoTaskRecords, canLeaveDemo, demoWorkLevel)
        } else {
            null
        }
        return WindowStart(host, state.watchedPane.takeIf { paneShown }, demo)
    }

    /** A new window: its own connection to [start]'s host, at its pane. */
    fun begin(start: WindowStart) {
        val demo = start.demo
        if (demo == null) {
            scope.launch { connectNow(start.host.id, start.pane) }
            return
        }
        demoTaskRecords = demo.taskRecords
        demoWorkLevel = demo.workLevel
        beginDemo(demo.set, demo.alerts, start.host, start.pane)
        canLeaveDemo = demo.canLeave
    }

    /** One demo host, nothing saved. */
    private fun beginDemo(set: DemoBridge.Content, alerts: Boolean, host: Host, pane: String? = null) {
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
            connectNow(host.id, pane)
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
        readerReturnsToItem = false
        showReader = false
        selection = null
        paneShown = false
        listShown = true
        itemFocus = null
        inboxPrompt = null
        inboxError = null
        itemReplies = emptyMap()
        answeringItem = null
        newTaskRecord = null
        taskError = null
        askError = null
        restartingTask = null
        restartError = null
        startingLoop = null
        loopError = null
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
        lastViews.clear()
    }

    // Connection

    val isConnected: Boolean get() = state.phase == PaneSession.Phase.Connected
    val currentHost: Host? get() = hostList.hosts.firstOrNull { it.id == currentHostId }

    fun connect(id: String) = scope.launch { connectNow(id) }

    /** Connects to host [id] and shows [pane] when it is there, else the pane last shown. */
    private suspend fun connectNow(id: String, pane: String? = null) {
        val host = hosts.host(id) ?: return
        val sameHost = id == currentHostId
        currentHostId = id
        hosts.markUsed(id)
        stayDisconnected = false
        connectError = null
        connectFix = null
        connecting = true
        if (!sameHost) {
            selection = null
            paneShown = false
            listShown = true
        }
        try {
            session.connect(samePanes = sameHost) { open(host) }
            // Inbox and Tasks mode stay on their list; Sessions mode shows the pane (ADR 0035).
            if (pane != null && state.sessions.any { s -> s.panes.any { it.id == pane } }) {
                watch(pane)
                showPane()
            } else if (mode == SidebarMode.SESSIONS) {
                openDefaultPane()
            }
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
        if (demo && host.id == demoHostId) return DemoBridge(demoSet, demoAlerts, demoTaskRecords, demoWorkLevel)
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
        selection = null
        paneShown = false
        listShown = true
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

    /**
     * Manual sessions with a pane to show, grouped by repo like the desktop's session list.
     * Task sessions open from Inbox and Tasks; resuming, the switcher and alerts reach any session.
     */
    val sessionGroups: List<RepoGroup> get() = paneSessions.manual.groupedByRepo()

    /** The sessions in the order shown, which the session shortcuts follow. */
    val visibleSessions: List<Session> get() = sessionGroups.flatMap { it.sessions }

    /** Every session with a pane, Task sessions too, in tmux's order. The switcher searches these. */
    val paneSessions: List<Session> get() = state.sessions.filter { it.defaultPane != null }

    /** Every session runs a Task, so the Sessions list is empty. */
    val noManualSessions: Boolean get() = visibleSessions.isEmpty() && paneSessions.isNotEmpty()

    /** The first session in repo order, Task sessions too: the pane-first fallback. */
    private val firstSession: Session? get() = paneSessions.groupedByRepo().firstOrNull()?.sessions?.firstOrNull()

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

    /** A Sessions row, the switcher or a session key: [s] in the view it showed last (ADR 0035). */
    fun open(s: Session) = scope.launch { openNow(s) }

    /** Shows [s]'s pane in [view], else in its last view, else Chat, Agent or Shell as it has them. */
    private suspend fun openNow(s: Session, view: PaneView? = lastViews[s.id]) {
        val pane = s.defaultPane ?: return
        watch(pane.id)
        if (view != null && view != paneView) showNow(view)
        showPane()
    }

    /** The detail shows the pane; the list makes room for it (ADR 0012, 0020). */
    private fun showPane() {
        paneShown = true
        listShown = false
    }

    /** [s] has an agent whose conversation the bridge can follow. */
    fun canChat(s: Session): Boolean = s.panes.any(::chatPossible)

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
        if (pane != null) {
            watch(pane.id)
            showPane()
        } else {
            firstSession?.let { openNow(it) }
        }
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
        rememberView()
    }

    private fun rememberView() {
        currentSession?.id?.let { lastViews[it] = paneView }
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

    fun show(view: PaneView) = scope.launch { showNow(view) }

    private suspend fun showNow(view: PaneView) {
        when (view) {
            PaneView.CHAT, PaneView.AGENT -> {
                val agent = agentPane ?: return
                if (agent.id != currentPane?.id) watch(agent.id)
                setPretty(view == PaneView.CHAT)
            }
            PaneView.SHELL -> {
                val shell = shellPane ?: return
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
        rememberView()
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

    /** What the Reader opens on, once: an Inbox item's doc, diff or image. */
    var readerTarget: ReaderItem? = null

    /**
     * The Reader of [item]'s pane, at [tab] or at [target]. The pane is watched but not shown,
     * so closing the Reader returns to the item (ADR 0035).
     */
    fun readItem(item: InboxItem, tab: ReaderTab, target: ReaderItem? = null) = scope.launch {
        val pane = item.pane ?: return@launch
        if (state.sessions.none { s -> s.panes.any { it.id == pane } }) {
            inboxError = "The pane of this question is gone. Open All Tasks to find its Task."
            return@launch
        }
        watch(pane)
        if (!canReadDocs) return@launch
        readerTab = if (tab in readerTabs) tab else ReaderTab.DOCS
        readerTarget = target
        readerOpensPlan = target != null
        readerReturnsToItem = target != null
        showReader = true
    }

    /** Whether the Reader was asked for the plan or a Task's brief. A phone opens it on its list otherwise. */
    var readerOpensPlan = false
        private set

    /** The Reader shows an Inbox item's evidence: Back closes it and returns to the item, not to the Reader's list. */
    var readerReturnsToItem = false
        private set

    fun openReader() {
        if (!canReadDocs) return
        if (readerTab !in readerTabs) readerTab = ReaderTab.DOCS
        readerOpensPlan = false
        readerReturnsToItem = false
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
        readerOpensPlan = true
        readerReturnsToItem = false
        showReader = true
    }

    fun closeReader() {
        showReader = false
        readerReturnsToItem = false
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

    /** Every New Task entry point can open a sheet. */
    val canCreateTasks: Boolean get() = canShowTasks || canStartTasks

    /**
     * Opens the New Task sheet in [repoDir], else the shown pane's repo. Away from a pane the
     * record-based sheet starts in the first workspace with no repo; the old sheet, for a bridge
     * without task records, needs a repo and takes the first one known.
     */
    fun startNewTask(repoDir: String? = null) {
        if (!canCreateTasks) return
        val shown = currentSession?.repoDir?.takeIf { paneShown }
        val dir = repoDir ?: shown ?: (if (canShowTasks) null else state.sessions.knownRepos.firstOrNull()?.dir) ?: ""
        val show: () -> Unit = if (canShowTasks) {
            { showRecordSheet(dir) }
        } else {
            { newTask = TaskDraft(repoDir = dir) }
        }
        // One sheet at a time.
        if (showSwitcher) {
            newTaskAfterSwitcher = show
            showSwitcher = false
            switcherClosed()
        } else {
            show()
        }
    }

    private fun showRecordSheet(repoDir: String) = scope.launch {
        // The workspaces come with the task list.
        if (state.tasks == null) runCatching { session.loadTasks() }
        newTaskRecord = TaskRecordDraft.inRepo(repoDir, state.tasks?.workspaces.orEmpty())
    }

    fun switcherClosed() {
        val show = newTaskAfterSwitcher
        if (show != null) {
            newTaskAfterSwitcher = null
            show()
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
        if (wasShown) firstSession?.let { openNow(it) }
        refocusPane()
    }

    // Task records

    /** The bridge keeps Tasks, so the Tasks screen has something to show. */
    val canShowTasks: Boolean get() = isConnected && state.canKeepTasks

    /** The Tasks by Inbox group, from the records and their sessions' agent states. */
    val inbox: List<InboxSection> get() = inbox(state.tasks?.records.orEmpty(), state.sessions)

    /**
     * Records the Task and shows it: its detail in Tasks mode, else its session. A failed
     * launch shows the Task's detail, with the note that says why. Throws only when nothing
     * was recorded.
     */
    suspend fun create(draft: TaskRecordDraft) {
        val req = draft.request ?: return
        val record = try {
            session.createTask(req)
        } catch (e: PaneSession.Failure.LaunchFailed) {
            e.record
        }
        newTaskRecord = null
        val running = sessionOf(record)
        // From Tasks mode the new Task's detail; elsewhere its session, as the old sheet did.
        if ((mode == SidebarMode.TASKS && !paneShown) || running == null) {
            showTask(record.id)
            refreshWork()
        } else {
            openNow(running)
        }
    }

    // Ask sessions (ADR 0031)

    /** The host starts ask sessions: the workspace's agent in a repo's main checkout, with no Task. */
    val canAsk: Boolean get() = isConnected && state.canAsk

    /** The workspace a repo header's Ask starts in; null when no workspace holds [repoDir]. */
    fun askWorkspace(repoDir: String?): String? =
        repoDir?.takeIf { canAsk }?.let { state.tasks?.workspaces.orEmpty().containing(it)?.name }

    /**
     * Starts an ask session in [repoDir] and shows its agent pane, as a manual session.
     * Throws what the bridge said went wrong.
     */
    suspend fun ask(workspace: String, repoDir: String) {
        val started = session.startSession(workspace, repoDir)
        newTaskRecord = null
        val s = state.sessions.firstOrNull { it.id == started.session } ?: return
        openNow(s, PaneView.AGENT)
    }

    /** Ask from a repo header: a failure lands in [askError]. */
    fun askAbout(repoDir: String) = scope.launch {
        val workspace = askWorkspace(repoDir) ?: return@launch
        try {
            ask(workspace, repoDir)
        } catch (e: PaneSession.Failure) {
            askError = PaneSession.describe(e)
        }
    }

    fun setStatus(record: TaskRecord, status: TaskStatus) = scope.launch {
        if (record.status == status) return@launch
        try {
            session.updateTask(record, status)
        } catch (e: PaneSession.Failure) {
            taskError = PaneSession.describe(e)
        }
    }

    /** The session that runs [record], when it has one. */
    fun sessionOf(record: TaskRecord): Session? = state.sessions.runningTask(record)

    /** The Task's session over its detail, in [view]: the terminal, or its Chat. */
    fun openTask(record: TaskRecord, view: PaneView) = scope.launch {
        val s = sessionOf(record) ?: return@launch
        openNow(s, view)
    }

    /** The bridge can restart a Task's agent with a handoff (ADR 0018). */
    val canRestartAgents: Boolean get() = isConnected && state.canRestartAgents

    /** Restarts [record]'s agent; [force] stops one that is running. A failure shows the bridge's reason. */
    fun restartAgent(record: TaskRecord, force: Boolean, worker: String? = null) = scope.launch {
        restartingTask = null
        try {
            session.restartAgent(record, force, worker)
        } catch (e: PaneSession.Failure) {
            restartError = PaneSession.describe(e)
        }
    }

    /** The bridge reached the muxa daemon, so Tasks can have a Task Loop (ADR 0019). */
    val canLoop: Boolean get() = isConnected && state.canLoop

    /** The allowed checks of [workspace]; null means it has no loops. */
    fun checksOf(workspace: String): WorkspaceChecks? = state.tasks?.workspaces?.firstOrNull { it.name == workspace }?.checks

    /** Opens the Start loop sheet, with the Task's last check when the workspace still allows it. */
    fun startLoopSheet(record: TaskRecord) {
        val fresh = LoopDraft(checksOf(record.workspace))
        val last = record.loop
        val draft = if (last != null && fresh.copy(check = last.check).problem == null) {
            fresh.copy(check = last.check, maxAttempts = last.maxAttempts)
        } else {
            fresh
        }
        startingLoop = record.id to draft
    }

    /** Starts the Task's loop; a failure shows the host's reason. */
    fun startLoop(record: TaskRecord, draft: LoopDraft, reset: Boolean) = scope.launch {
        val spec = draft.spec ?: return@launch
        startingLoop = null
        try {
            session.startLoop(record, spec, reset)
        } catch (e: PaneSession.Failure) {
            loopError = PaneSession.describe(e)
        }
    }

    fun stopLoop(record: TaskRecord) = scope.launch {
        try {
            session.stopLoop(record)
        } catch (e: PaneSession.Failure) {
            loopError = PaneSession.describe(e)
        }
    }

    /**
     * The Reader of the Task's session, at its changes or at [doc]: its brief, its handoff, or
     * its last check. Closing it returns to the Task.
     */
    fun readTask(record: TaskRecord, tab: ReaderTab, doc: String = TASK_DOC) = scope.launch {
        val s = sessionOf(record) ?: return@launch
        s.defaultPane?.let { watch(it.id) }
        if (!canReadDocs) return@launch
        readerTab = if (tab in readerTabs) tab else ReaderTab.DOCS
        // The brief is the document to open; the Reader opens the one last read here.
        if (tab == ReaderTab.DOCS) lastDocs[s.id] = doc
        readerOpensPlan = tab == ReaderTab.DOCS
        readerReturnsToItem = false
        showReader = true
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

    /** The banner opens the session's Inbox item when it has one, else its Chat (ADR 0008, 0035). */
    fun openAlert() = scope.launch {
        val shown = alert ?: return@launch
        alert = null
        val s = state.sessions.firstOrNull { it.id == shown.session } ?: return@launch
        val item = inboxItems.firstOrNull { (it.task != null && it.task == s.taskId) || it.session == s.id }
        if (item != null) {
            openItem(item.id)
        } else {
            openNow(s, PaneView.CHAT.takeIf { canChat(s) })
        }
    }

    companion object {
        const val KEY_COMMENT = "muxa-android"

        /** Where the bridge writes a Task's brief, relative to its directory (ADR 0015). */
        const val TASK_DOC = ".muxa/TASK.md"

        /** Where the bridge writes the facts for a restarted agent, beside the brief (ADR 0018). */
        const val HANDOFF_DOC = ".muxa/HANDOFF.md"

        /** Where the daemon writes the last check's result, beside the brief (ADR 0019). */
        const val CHECK_DOC = ".muxa/CHECK.md"
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
