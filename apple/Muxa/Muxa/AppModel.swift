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
    let hosts: HostStore
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
    /// Set by a DEBUG launch argument: an in-app bridge instead of SSH.
    let demo: Bool

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
    /// The New Task sheet, with the repo it starts in.
    var newTask: TaskDraft?
    /// An agent elsewhere that wants attention, shown for a few seconds.
    private(set) var alert: AgentAlert?
    /// The task waiting for the user to pick how to close it.
    var closingTask: Session?
    /// Why the last close failed; nothing was closed then.
    var closeError: String?
    /// Asked for from the session switcher: shown once the switcher has closed.
    private var newTaskAfterSwitcher: TaskDraft?
    private(set) var docListing: DocListing?
    private(set) var docError: String?
    /// The shown pane's uncommitted changes, as last listed.
    private(set) var changeListing: ChangeListing?
    /// The shown pane's images, while the Reader is open.
    private(set) var imageListing: ImageListing?
    /// The Reader's open tab, kept while the app runs.
    var readerTab: ReaderTab = .docs
    /// The document last read in each session, opened again next time.
    private var lastDocs: [String: String] = [:]

    init() {
        #if DEBUG
        demo = ProcessInfo.processInfo.arguments.contains("-demo")
        #else
        demo = false
        #endif
        if demo {
            hosts = HostStore(file: nil)
            let host = Host(name: "Demo", target: SSHTarget(host: Self.demoHost, user: "demo"))
            hosts.upsert(host)
            hosts.markUsed(host.id)
        } else {
            hosts = HostStore(file: HostStore.defaultFile)
        }
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
        defer { connecting = false }
        do {
            try await session.connect({ try await self.open(host) }, samePanes: sameHost)
            await openDefaultPane()
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
        #if DEBUG
        if demo, host.target.host == Self.demoHost { return DemoBridge() }
        #endif
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
            connectError = "The host key was not trusted, so this iPad's key was not added."
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
    private static let demoHost = "demo.invalid"

    // MARK: Panes

    /// Sessions with a pane to show, grouped by repo like the desktop's session list.
    var sessionGroups: [RepoGroup] {
        session.sessions.filter { $0.defaultPane != nil }.groupedByRepo()
    }

    /// The sessions in the order shown, which Cmd+1…9 and Cmd+Shift+[ ] follow.
    var visibleSessions: [Session] {
        sessionGroups.flatMap(\.sessions)
    }

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

    func open(_ s: Session) async {
        guard let pane = s.defaultPane else { return }
        await watch(pane.id)
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
    private func openDefaultPane() async {
        guard session.watchedPane == nil else { return }
        if let id = currentHostID, let last = hosts.lastPanes[id], let pane = session.sessions.resuming(last) {
            await watch(pane.id)
        } else if let first = visibleSessions.first {
            await open(first)
        }
    }

    private func watch(_ pane: String) async {
        do {
            try await session.watch(pane: pane)
        } catch {
            connectError = PaneSession.describe(error)
            return
        }
        if let id = currentHostID, let name = currentSession?.name {
            hosts.remember(LastPane(session: name, pane: pane), on: id)
        }
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

    func openReader() {
        guard canReadDocs else { return }
        if !readerTabs.contains(readerTab) { readerTab = .docs }
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

    /// Opens the New Task sheet in `repoDir`, else the shown session's repo.
    func startNewTask(in repoDir: String? = nil) {
        guard canStartTasks else { return }
        let dir = repoDir ?? currentSession?.repoDir ?? session.sessions.knownRepos.first?.dir ?? ""
        // One sheet at a time.
        if showSwitcher {
            newTaskAfterSwitcher = TaskDraft(repoDir: dir)
            showSwitcher = false
        } else {
            newTask = TaskDraft(repoDir: dir)
        }
    }

    func switcherClosed() {
        if let draft = newTaskAfterSwitcher {
            newTaskAfterSwitcher = nil
            newTask = draft
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
        if wasShown, let next = visibleSessions.first { await open(next) }
        refocusPane()
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

    func openAlert() async {
        guard let shown = alert else { return }
        alert = nil
        if let s = session.sessions.first(where: { $0.id == shown.session }) { await open(s) }
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
