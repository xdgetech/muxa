import Foundation
import MuxaProtocol
import MuxaSession
import MuxaTransport
import Observation

/// App state: the saved hosts, the connection, and which pane is shown.
@MainActor
@Observable
final class AppModel {
    let session = PaneSession()
    /// The saved hosts, or during the demo its one host, kept in memory only.
    private(set) var hosts: HostStore
    private let savedHosts: HostStore
    /// The host the session belongs to: connected, connecting, or last tried.
    private(set) var currentHostID: UUID?
    private(set) var connecting = false
    /// Logging in with the password to add this device's key, before connecting.
    private(set) var addingKey = false
    /// A host key waiting for the user's decision on first use (or after it changed).
    var untrustedKey: HostKey?
    var connectError: String?
    /// What the user can do about `connectError`, when the app knows.
    private(set) var connectFix: ConnectFix?
    var showSwitcher = false
    /// Bumped when a sheet closes, so the pane takes the keyboard back.
    private(set) var focusRequest = 0
    /// Bumped when the user opens a row or a pane. On a phone the detail is pushed
    /// over the sidebar then.
    var detailRequest = 0
    /// The in-app demo bridge instead of SSH: Try the Demo, or the UI tests' launch
    /// argument `-demo` (DEBUG only).
    private(set) var demo = false
    /// The user started the demo, so the sidebar offers to leave it. The UI tests' launch
    /// argument starts it for good.
    private(set) var canLeaveDemo = false
    /// The demo bridge serves this host only; a host added during the demo uses SSH.
    private var demoHostID: UUID?
    private var demoContent = DemoContent.showcase
    private var demoAlerts = true

    /// What to do once the user trusts the presented host key. A setup password is
    /// held only while the trust sheet is up.
    private enum AfterTrust {
        case connect
        case setUp(password: String)
    }
    private var afterTrust: AfterTrust?
    /// The user pressed Disconnect: returning to the foreground does not reconnect.
    private var stayDisconnected = false
    /// Agent panes the user switched to Raw. Every other agent pane shows Pretty when it can.
    private(set) var rawPanes: Set<String> = []
    var showReader = false
    /// The Terminal Size the user picked, by menu or pinch. Nil: the view picks.
    /// Kept here, so it survives a fold or a rotation.
    var paneFit: PaneFit?
    /// The old New Task sheet, with the repo it starts in: only for a bridge
    /// without task records.
    var newTask: TaskDraft?
    /// The New Task sheet that records the task on the host.
    var newRecord: TaskRecordDraft?
    /// Created from that sheet: shown once the sheet has closed.
    private var createdRecord: TaskRecord?
    /// An agent elsewhere that wants attention, shown for a few seconds.
    private(set) var alert: AgentAlert?
    /// The task waiting for the user to pick how to close it.
    var closingTask: Session?
    /// Why the last close failed; nothing was closed then.
    var closeError: String?
    /// The ask session waiting for the user to confirm its close (ADR 0039).
    var closingAsk: Session?
    /// Why the last ask session close failed; nothing was closed then.
    var closeAskError: String?
    /// Why an ask session from a repo header did not start.
    var askError: String?
    /// An ask session started from the New Task sheet, shown once the sheet has closed.
    private var askedSession: Session?
    /// Asked for from the session switcher: shown once the switcher has closed.
    private var newTaskAfterSwitcher: (() -> Void)?
    private(set) var docListing: DocListing?
    private(set) var docError: String?
    /// The shown pane's uncommitted changes, as last listed.
    private(set) var changeListing: ChangeListing?
    /// The shown pane's images, while the Reader is open.
    private(set) var imageListing: ImageListing?
    /// The Reader's open tab, kept while the app runs.
    var readerTab: ReaderTab = .docs
    /// What the Reader opens at by itself, once: an Inbox item's doc or image.
    var readerOpening: ReaderItem?
    /// The window has the phone layout (ADR 0012); `RootView` keeps it current.
    var phoneLayout = false
    var canUseRoutines: Bool { isConnected && session.canUseRoutines }

    // Navigation (ADR 0035), driven by `AppModel+Navigation.swift`.

    /// What the sidebar shows, remembered on this device. Set it with `setMode`.
    var mode: SidebarMode
    /// What the detail column shows: the sidebar's selection, or a pane.
    var detail: Detail?
    /// Pushed over `detail` in the detail column: a Routine, and on a phone the pane
    /// opened from an item or a Task, so Back returns to it.
    var detailPath: [DetailRoute] = []
    var showSettings = false
    /// Bumped to show the sidebar: on a phone, the list.
    var sidebarRequest = 0
    /// Bumped by `⌘0`: shows or hides the sidebar.
    var sidebarToggle = 0
    /// Bumped when a pane is opened: the sidebar hides, so the pane gets the width.
    var paneRequest = 0
    /// Later for an item, asked for by `⌘⇧L`.
    var laterItem: InboxItem?
    /// A pane view is about to appear for a pane watched without one (`paneAppeared`).
    var paneViewPending = false
    /// The pane last shown in each session, so a Sessions row opens its last view.
    private var lastPanes: [String: String] = [:]
    /// The document last read in each session, opened again next time.
    private var lastDocs: [String: String] = [:]

    func readerClosed() { refocusPane() }

    init() {
        mode = Self.launchMode()
        #if DEBUG
        let arguments = ProcessInfo.processInfo.arguments
        if arguments.contains("-demo") {
            demo = true
            demoContent = .current
            demoAlerts = arguments.contains("-demo-alerts")
            // The store screenshots show the list: a machine's name there, not the demo's.
            let showcase = arguments.contains("-showcase")
            (hosts, demoHostID) = Self.demoHosts(showcase ? Host(name: "Mac mini", target: SSHTarget(host: "mac-mini.local", user: "you")) : nil)
            savedHosts = HostStore(file: nil)
            return
        }
        // The Try the Demo test starts from no hosts, whatever the simulator saved.
        if arguments.contains("-no-saved-hosts") {
            savedHosts = HostStore(file: nil)
            hosts = savedHosts
            return
        }
        #endif
        savedHosts = HostStore(file: HostStore.defaultFile)
        hosts = savedHosts
    }

    /// One demo host, never written to disk.
    private static func demoHosts(_ shown: Host? = nil) -> (HostStore, UUID) {
        let hosts = HostStore(file: nil)
        let host = shown ?? Host(name: "Demo", target: SSHTarget(host: "demo.invalid", user: "demo"))
        hosts.upsert(host)
        hosts.markUsed(host.id)
        return (hosts, host.id)
    }

    // MARK: Demo

    /// Try the Demo: sample sessions from the in-app bridge. Nothing is saved, and no
    /// SSH connection or key is touched.
    func startDemo() async {
        await session.disconnect()
        resetHostState()
        demo = true
        canLeaveDemo = true
        demoContent = .showcase
        demoAlerts = true
        (hosts, demoHostID) = Self.demoHosts()
        guard let id = hosts.lastUsed else { return }
        await connect(to: id)
    }

    /// Back to the saved hosts, disconnected.
    func leaveDemo() async {
        await session.disconnect()
        resetHostState()
        demo = false
        canLeaveDemo = false
        demoHostID = nil
        hosts = savedHosts
    }

    /// What belonged to the last host's connection.
    private func resetHostState() {
        currentHostID = nil
        connectError = nil
        connectFix = nil
        stayDisconnected = false
        rawPanes = []
        lastDocs = [:]
        lastPanes = [:]
        paneFit = nil
        alert = nil
        clearDetail()
    }

    // MARK: Connection

    var isConnected: Bool { session.phase == .connected }
    var currentHost: Host? { hosts.host(currentHostID) }

    func connect(to id: UUID) async {
        guard let host = hosts.host(id) else { return }
        let sameHost = id == currentHostID
        currentHostID = id
        hosts.markUsed(id)
        stayDisconnected = false
        connectError = nil
        connectFix = nil
        connecting = true
        showSettings = false
        defer { connecting = false }
        do {
            try await session.connect({ try await self.open(host) }, samePanes: sameHost)
            await connected()
        } catch TransportError.hostKeyMismatch(let presented) {
            afterTrust = .connect
            untrustedKey = presented
        } catch {
            fail(error)
        }
    }

    private func fail(_ error: any Error) {
        connectError = PaneSession.describe(error)
        connectFix = ConnectFix(error)
    }

    private func open(_ host: Host) async throws -> any BridgeChannel {
        if demo, host.id == demoHostID { return DemoBridge(content: demoContent, alerts: demoAlerts) }
        return try await SSHBridge.connect(to: host.target, key: ClientKeyStore.loadOrCreate())
    }

    func trust(_ key: HostKey) async {
        untrustedKey = nil
        let next = afterTrust
        afterTrust = nil
        guard var host = currentHost else { return }
        host.target.hostKey = key
        hosts.upsert(host)
        switch next {
        case .setUp(let password): await setUpKey(on: host.id, password: password)
        case .connect, nil: await connect(to: host.id)
        }
    }

    func cancelTrust() {
        if case .setUp = afterTrust {
            connectError = "The host key was not trusted, so this device's key was not added."
            connectFix = .addKey
        }
        untrustedKey = nil
        afterTrust = nil
    }

    /// Logs in once with `password`, adds this device's key to the host, then connects.
    func setUpKey(on id: UUID, password: String) async {
        guard let host = hosts.host(id) else { return }
        currentHostID = id
        connectError = nil
        connectFix = nil
        do {
            addingKey = true
            // Runs before any catch, so a failure shows its error, not the progress.
            defer { addingKey = false }
            let key = try ClientKeyStore.loadOrCreate()
            let line = try KeySetup.authorizedLine(for: key, command: host.target.command, comment: Self.keyComment)
            _ = try await KeySetup.install(line, on: host.target, password: password)
        } catch TransportError.hostKeyMismatch(let presented) {
            afterTrust = .setUp(password: password)
            untrustedKey = presented
            return
        } catch TransportError.notAuthorized {
            connectError = "The host refused the password. Check the user and password."
            connectFix = .addKey
            return
        } catch {
            fail(error)
            return
        }
        await connect(to: id)
    }

    func disconnect() async {
        clearDetail()
        stayDisconnected = true
        await session.disconnect()
    }

    /// Leaving for the background: close cleanly so the bridge exits, and come back later.
    func suspend() async {
        await session.disconnect()
    }

    /// Launch or return to the foreground: reconnect to the last host and pane.
    func resume() async {
        guard !stayDisconnected, !isConnected, !connecting, !addingKey else { return }
        guard let id = currentHostID ?? hosts.lastUsed, hosts.host(id) != nil else { return }
        await connect(to: id)
    }

    func refocusPane() { focusRequest += 1 }

    /// The public key line for `~/.ssh/authorized_keys`, restricted to `command`.
    func authorizedKeyLine(command: String) -> String? {
        guard let key = try? ClientKeyStore.loadOrCreate() else { return nil }
        return try? KeySetup.authorizedLine(for: key, command: command, comment: Self.keyComment)
    }

    private static let keyComment = "muxa-ipad"

    // MARK: Panes

    /// Manual sessions with a pane to show, grouped by repo like the desktop's session list.
    /// Task sessions open from Inbox and Tasks; resuming, the switcher and alerts reach any session.
    var sessionGroups: [RepoGroup] {
        paneSessions.manual.groupedByRepo()
    }

    /// The sessions in the order shown, which Cmd+1…9 and Cmd+Shift+[ ] follow.
    var visibleSessions: [Session] {
        sessionGroups.flatMap(\.sessions)
    }

    /// Every session with a pane, Task sessions too, in tmux's order. The switcher searches these.
    var paneSessions: [Session] {
        session.sessions.filter { $0.defaultPane != nil }
    }

    /// Every session runs a Task, so the Sessions list is empty.
    var noManualSessions: Bool { visibleSessions.isEmpty && !paneSessions.isEmpty }

    /// The first session in repo order, Task sessions too: the pane-first fallback.
    private var firstSession: Session? { paneSessions.groupedByRepo().first?.sessions.first }

    var currentSession: Session? {
        guard let pane = session.watchedPane else { return nil }
        return session.sessions.first { $0.panes.contains { $0.id == pane } }
    }

    var currentPane: Pane? {
        guard let id = session.watchedPane else { return nil }
        return currentSession?.panes.first { $0.id == id }
    }

    var counterpart: Pane? {
        guard let id = session.watchedPane else { return nil }
        return currentSession?.counterpart(of: id)
    }

    /// Shows `s` in its last view on this device, else its default pane (ADR 0035 §3).
    func open(_ s: Session) async {
        await showLastView(of: s, overSelection: false)
    }

    /// `s`'s last pane on this device, else its default pane.
    private func lastView(of s: Session) -> Pane? {
        let last = lastPanes[s.id].flatMap { id in s.panes.first { $0.id == id } }
        return last ?? s.defaultPane
    }

    /// Shows the pane first, then watches it: the pane's view must exist to take
    /// the screen the watch sends.
    @discardableResult
    private func showLastView(of s: Session, overSelection: Bool) async -> Bool {
        guard let pane = lastView(of: s) else { return false }
        presentPane(overSelection: overSelection)
        await watch(pane.id)
        return true
    }

    func toggleAgentTerminal() async {
        guard let other = counterpart else { return }
        await watch(other.id)
    }

    /// Moves `offset` sessions through the list, wrapping around.
    func step(_ offset: Int) async {
        let list = visibleSessions
        guard !list.isEmpty else { return }
        let index = list.firstIndex { $0.id == currentSession?.id } ?? 0
        await open(list[(index + offset + list.count) % list.count])
    }

    /// Cmd+1…9: the nth session, 1-based.
    func openSession(number: Int) async {
        let list = visibleSessions
        guard number >= 1, number <= list.count else { return }
        await open(list[number - 1])
    }

    /// The pane last shown on this host, else the first session.
    func openDefaultPane() async {
        if let pane = session.watchedPane {
            presentPane()
            await watch(pane)
        } else if let id = currentHostID, let last = hosts.lastPanes[id], let pane = session.sessions.resuming(last) {
            presentPane()
            await watch(pane.id)
        } else if let first = firstSession {
            await open(first)
        }
    }

    func watch(_ pane: String) async {
        do {
            try await session.watch(pane: pane)
        } catch {
            connectError = PaneSession.describe(error)
            return
        }
        if let id = currentHostID, let name = currentSession?.name {
            hosts.remember(LastPane(session: name, pane: pane), on: id)
        }
        if let s = currentSession { lastPanes[s.id] = pane }
        changeListing = nil
        Task { await loadChanges() }
        // The pane stays watched in Pretty, so Raw is instant and pane-gone still arrives.
        if let current = currentPane, current.isAgentSide {
            await session.follow(pane: current.id)
        } else {
            await session.unfollow()
        }
    }

    // MARK: Pretty and Raw

    /// What the pane area shows: the agent as a chat or as its terminal, or the shell.
    enum PaneView: Hashable {
        case chat, agent, shell
    }

    /// The agent side and the shell side of the shown session, whichever is open.
    var agentPane: Pane? {
        guard let pane = currentPane else { return nil }
        return pane.isAgentSide ? pane : counterpart
    }

    var shellPane: Pane? {
        guard let pane = currentPane else { return nil }
        return pane.isAgentSide ? counterpart : pane
    }

    var paneView: PaneView {
        guard let pane = currentPane, pane.isAgentSide else { return .shell }
        return showsPretty ? .chat : .agent
    }

    /// The views the shown session has; Chat only where an adapter knows the agent.
    var paneViews: [PaneView] {
        var views: [PaneView] = []
        if let agent = agentPane {
            if chatPossible(agent) { views.append(.chat) }
            views.append(.agent)
        }
        if shellPane != nil { views.append(.shell) }
        return views
    }

    func show(_ view: PaneView) async {
        switch view {
        case .chat, .agent:
            guard let agent = agentPane else { return }
            if agent.id != currentPane?.id { await watch(agent.id) }
            setPretty(view == .chat)
        case .shell:
            guard let shell = shellPane, shell.id != currentPane?.id else { return }
            await watch(shell.id)
        }
    }

    private func chatPossible(_ pane: Pane) -> Bool {
        pane.isAgentSide && session.canChat && !session.unsupportedChats.contains(pane.id)
    }

    /// The agent pane has a conversation the bridge can follow.
    var canShowPretty: Bool {
        guard let pane = currentPane else { return false }
        return chatPossible(pane)
    }

    var showsPretty: Bool {
        guard let pane = currentPane else { return false }
        return canShowPretty && !rawPanes.contains(pane.id)
    }

    func setPretty(_ pretty: Bool) {
        guard let pane = currentPane, canShowPretty else { return }
        if pretty { rawPanes.remove(pane.id) } else { rawPanes.insert(pane.id) }
        refocusPane()
    }

    /// The chat for the shown pane, once the bridge follows it.
    var chat: ChatLog? {
        guard let log = session.chat, log.pane == currentPane?.id else { return nil }
        return log
    }

    /// Sends a prompt as one paste, so a multi-line prompt stays one prompt, then Enter.
    func sendPrompt(_ text: String) {
        session.paste(text)
        session.send(keys: ["Enter"])
    }

    // MARK: Reader

    var canReadDocs: Bool { session.canReadDocs && currentPane != nil }

    /// Whether the Reader was asked for the plan. A phone opens it on its list otherwise.
    private(set) var readerOpensPlan = false

    func openReader() {
        guard canReadDocs else { return }
        if !readerTabs.contains(readerTab) { readerTab = .docs }
        readerOpensPlan = false
        showReader = true
    }

    /// The tabs the bridge can fill.
    var readerTabs: [ReaderTab] {
        ReaderTab.allCases.filter { tab in
            switch tab {
            case .docs: true
            case .changes: session.canShowChanges
            case .images: session.canShowImages
            }
        }
    }

    /// The Reader at the agent's plan, e.g. while it asks to leave plan mode.
    func openPlan() {
        guard canReadDocs else { return }
        if let id = currentSession?.id { lastDocs[id] = nil }
        readerTab = .docs
        readerOpensPlan = true
        showReader = true
    }

    func closeReader() {
        showReader = false
        Task {
            await session.closeDocument()
            await session.closeDiff()
            await session.closeImage()
            await loadChanges()
        }
        refocusPane()
    }

    func loadDocs() async {
        guard let pane = currentPane else { return }
        docError = nil
        do {
            docListing = try await session.documents(pane: pane.id)
        } catch {
            docError = PaneSession.describe(error)
        }
        await loadChanges()
        await loadImages()
    }

    /// Lists the shown pane's images; a failure only hides them.
    func loadImages() async {
        guard session.canShowImages, let pane = currentPane else {
            imageListing = nil
            return
        }
        let listing = try? await session.images(pane: pane.id)
        if currentPane?.id == pane.id { imageListing = listing }
    }

    func openImage(_ path: String) async {
        guard let pane = currentPane else { return }
        await session.closeDocument()
        await session.closeDiff()
        await session.openImage(pane: pane.id, path: path)
    }

    /// Lists the shown pane's changes; a failure only hides them.
    func loadChanges() async {
        guard session.canShowChanges, let pane = currentPane else {
            changeListing = nil
            return
        }
        let listing = try? await session.changes(pane: pane.id)
        // Another pane may be shown by now.
        if currentPane?.id == pane.id { changeListing = listing }
    }

    var changeCount: Int { changeListing?.entries.count ?? 0 }

    func openDiff(_ path: String, scope: DiffScope) async {
        guard let pane = currentPane else { return }
        await session.closeDocument()
        await session.closeImage()
        await session.openDiff(pane: pane.id, path: path, scope: scope)
    }

    /// The last one read in this session, else the plan, else the README, else the first.
    var preferredDoc: String? {
        guard let entries = docListing?.entries, !entries.isEmpty else { return nil }
        if let id = currentSession?.id, let last = lastDocs[id], entries.contains(where: { $0.path == last }) {
            return last
        }
        return entries.first { $0.kind == .plan }?.path
            ?? entries.first { $0.path.lowercased() == "readme.md" }?.path
            ?? entries.first?.path
    }

    func openDoc(_ path: String) async {
        guard let pane = currentPane else { return }
        if let id = currentSession?.id { lastDocs[id] = path }
        await session.closeDiff()
        await session.closeImage()
        await session.openDocument(pane: pane.id, path: path)
    }

    // MARK: Tasks

    var canStartTasks: Bool { isConnected && session.canStartTasks }

    /// Every New Task entry point can open a sheet.
    var canCreateTasks: Bool { canKeepTasks || canStartTasks }

    /// Opens the New Task sheet in `repoDir`, else the shown session's repo. A
    /// bridge with task records gets the record-based sheet.
    func startNewTask(in repoDir: String? = nil) {
        guard canCreateTasks else { return }
        let dir = repoDir ?? currentSession?.repoDir ?? session.sessions.knownRepos.first?.dir ?? ""
        let show: () -> Void = canKeepTasks
            ? { [self] in Task { await showRecordSheet(repo: dir) } }
            : { [self] in newTask = TaskDraft(repoDir: dir) }
        // One sheet at a time.
        if showSwitcher {
            newTaskAfterSwitcher = show
            showSwitcher = false
        } else {
            show()
        }
    }

    private func showRecordSheet(repo: String) async {
        // The workspaces come with the task list.
        if session.workspaces.isEmpty { await session.loadTasks() }
        newRecord = TaskRecordDraft(repo: repo, in: session.workspaces)
    }

    func recordCreated(_ record: TaskRecord) {
        createdRecord = record
        newRecord = nil
    }

    /// Shows the new task's session, or in Tasks mode its detail. A task whose agent
    /// did not start has no session: its detail says why.
    func recordSheetClosed() async {
        if let s = askedSession {
            askedSession = nil
            await openAgent(s)
            return
        }
        guard let record = createdRecord else {
            refocusPane()
            return
        }
        createdRecord = nil
        let s = session.sessions.first { $0.taskId == record.id }
            ?? record.session.flatMap { name in session.sessions.first { $0.name == name } }
        // Tasks mode shows the new Task; elsewhere its session opens, as before (ADR 0035 §1).
        if let s, mode != .tasks {
            await open(s)
        } else {
            select(.task(record.id))
        }
    }

    func switcherClosed() {
        if let show = newTaskAfterSwitcher {
            newTaskAfterSwitcher = nil
            show()
        } else {
            refocusPane()
        }
    }

    /// Starts the task and shows its session. Throws what the bridge said went wrong.
    func start(_ draft: TaskDraft) async throws {
        guard let req = draft.request else { return }
        let name = try await session.newTask(repo: req.repo, branch: req.branch, base: req.base)
        newTask = nil
        if let s = session.sessions.first(where: { $0.name == name }) { await open(s) }
    }

    // MARK: Ask sessions (ADR 0031)

    var canAsk: Bool { isConnected && session.canAsk }

    /// The workspace an ask in the repo at `dir` runs in; nil when none holds it.
    func askWorkspace(for dir: String) -> String? {
        canAsk ? session.workspaces.workspace(of: dir)?.name : nil
    }

    /// Starts an ask session in `repo` and shows its agent pane once every sheet that
    /// asked for it has closed. Throws what the host said went wrong.
    func ask(workspace: String, repo: String) async throws {
        let started = try await session.startSession(workspace: workspace, repo: repo)
        guard let s = session.sessions.first(where: { $0.id == started.id }) else { return }
        if newRecord != nil {
            askedSession = s
            newRecord = nil
        } else {
            await openAgent(s)
        }
    }

    /// The quick action on a repo header: the error shows as an alert.
    func ask(about dir: String) async {
        guard let workspace = askWorkspace(for: dir) else { return }
        do {
            try await ask(workspace: workspace, repo: dir)
        } catch {
            askError = PaneSession.describe(error)
        }
    }

    /// An ask session opens on its pane, not the chat.
    private func openAgent(_ s: Session) async {
        await open(s)
        if agentPane != nil { await show(.agent) }
    }

    var canCloseTasks: Bool { isConnected && session.canCloseTasks }

    /// Only a session with a worktree is a task.
    func canClose(_ s: Session) -> Bool { canCloseTasks && s.worktree != nil }

    func close(_ s: Session, mode: CloseMode) async {
        closingTask = nil
        let wasShown = s.id == currentSession?.id
        do {
            try await session.closeTask(session: s.id, mode: mode)
        } catch {
            closeError = PaneSession.describe(error)
            return
        }
        if wasShown, let next = firstSession { await open(next) }
        refocusPane()
    }

    /// Only an ask session closes this way: other sessions without a worktree are the user's.
    func canCloseAsk(_ s: Session) -> Bool {
        isConnected && session.canCloseAskSessions && s.ask != nil && s.worktree == nil
    }

    func closeAsk(_ s: Session) async {
        closingAsk = nil
        let wasShown = s.id == currentSession?.id
        do {
            try await session.closeAskSession(s)
        } catch {
            closeAskError = PaneSession.describe(error)
            return
        }
        if wasShown, let next = firstSession { await open(next) }
        refocusPane()
    }

    // MARK: Task records

    var canKeepTasks: Bool { isConnected && session.canKeepTasks }

    /// The task records by Inbox group, with the sessions that run them.
    var inbox: [InboxSection] { session.taskRecords.inbox(sessions: session.sessions) }

    func inboxItem(_ id: String) -> TaskItem? {
        inbox.lazy.flatMap(\.items).first { $0.id == id }
    }

    /// The bridge reached the muxa daemon, so tasks can have a Task Loop (ADR 0019).
    var canLoop: Bool { isConnected && session.canLoop }

    /// The allowed checks of `workspace`; nil means it has no loops.
    func checks(of workspace: String) -> WorkspaceChecks? {
        session.workspaces.first { $0.name == workspace }?.checks
    }

    /// What a task opens: its agent as a chat or a terminal, or the Reader at its
    /// TASK.md, its HANDOFF.md, its last check or its changes.
    enum TaskOpening: Equatable {
        case chat, terminal, taskFile, handoffFile, checkFile, changes
        /// An Inbox item's evidence: a document or an image of its pane.
        case doc(String), image(String)
    }

    /// Where TASK.md is in a task's directory, as the Reader lists it (ADR 0015).
    static let taskFile = ".muxa/TASK.md"
    /// Where the bridge writes the handoff before it restarts an agent (ADR 0018).
    static let handoffFile = ".muxa/HANDOFF.md"
    /// Where the daemon writes the last check's result (ADR 0019).
    static let checkFile = ".muxa/CHECK.md"

    /// Opens a Task's session as asked: its pane over the Task, or the Reader.
    func open(_ item: TaskItem, as opening: TaskOpening) {
        guard let s = item.session else { return }
        Task { await open(s, as: opening) }
    }

    /// Opens an item's session, or its evidence in the Reader for the item's pane.
    /// Closing the Reader returns to the item, which stays in the detail.
    func open(_ item: InboxItem, as opening: TaskOpening) {
        guard let s = itemSession(item) else { return }
        Task { await open(s, as: opening) }
    }

    /// Chat and Terminal show the pane, over the selection when `overSelection`; the
    /// rest opens the Reader for the session's pane and leaves the detail as it is.
    func open(_ s: Session, as opening: TaskOpening, overSelection: Bool = true) async {
        guard isConnected else { return }
        switch opening {
        case .chat, .terminal:
            guard await showLastView(of: s, overSelection: overSelection) else { return }
            if opening == .chat, let agent = agentPane, chatPossible(agent) {
                await show(.chat)
            } else if agentPane != nil {
                await show(.agent)
            }
            return
        default:
            // The Reader reads the session's pane; the detail stays as it is.
            guard let pane = lastView(of: s) else { return }
            if pane.id != session.watchedPane { await watch(pane.id) }
        }
        switch opening {
        case .chat, .terminal: break
        case .taskFile, .handoffFile, .checkFile:
            guard canReadDocs else { return }
            lastDocs[s.id] = switch opening {
            case .handoffFile: Self.handoffFile
            case .checkFile: Self.checkFile
            default: Self.taskFile
            }
            readerTab = .docs
            // Like the plan, it opens by itself, also on a phone.
            readerOpensPlan = true
            showReader = true
        case .changes:
            guard canReadDocs, session.canShowChanges else { return }
            readerTab = .changes
            readerOpensPlan = false
            showReader = true
        case .doc(let path), .image(let path):
            guard canReadDocs else { return }
            if case .image = opening {
                guard session.canShowImages else { return }
                readerTab = .images
                readerOpening = .image(path)
            } else {
                readerTab = .docs
                readerOpening = .doc(path)
            }
            readerOpensPlan = false
            showReader = true
        }
    }

    // MARK: Alerts

    private static let alertSeconds: Duration = .seconds(4)

    /// Called with each new session list: tells about agents elsewhere that
    /// now wait, finished, or failed.
    func sessionsChanged(from old: [Session], to new: [Session]) {
        guard let latest = new.alerts(since: old, shown: currentSession?.id).last else { return }
        alert = latest
        Task {
            try? await Task.sleep(for: Self.alertSeconds)
            if alert == latest { alert = nil }
        }
    }

    /// The banner opens the session's item when it has one, else its Chat (ADR 0035 §3).
    func openAlert() async {
        guard let shown = alert else { return }
        alert = nil
        guard let s = session.sessions.first(where: { $0.id == shown.session }) else { return }
        if let item = session.openItems.first(where: { itemSession($0)?.id == s.id }) {
            select(.item(item.id))
        } else {
            await open(s, as: .chat, overSelection: false)
        }
    }

    func retryChat() async {
        guard let pane = currentPane else { return }
        await session.follow(pane: pane.id)
    }
}

/// The next step a connect error points to.
enum ConnectFix {
    /// Log in once with the password to add this device's key.
    case addKey
    /// Install muxa-bridge on the host, or fix the bridge command.
    case installBridge

    init?(_ error: any Error) {
        switch error {
        case TransportError.notAuthorized: self = .addKey
        case PaneSession.Failure.noHello: self = .installBridge
        default: return nil
        }
    }
}

extension Pane {
    /// The agent side of a session: tagged as the agent, or untagged and not at a shell.
    var isAgentSide: Bool {
        switch role {
        case .agent: true
        case .shell: false
        case .hidden, .unknown: !shellForeground
        }
    }
}
