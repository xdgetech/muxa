import Foundation
import MuxaSession
import MuxaTransport
import MuxaV2
import Observation

/// The app on protocol v2 (ADR 0066, 0067): one connection to a host's muxa
/// server, resumed after a network change or a return from the background.
/// It keeps the Inbox, the terminals and the Routines current from events.
@MainActor
@Observable
final class NextModel {
    enum Link: Equatable {
        case offline
        case connecting
        case connected
        /// The connection is gone; it is resumed on its own.
        case reconnecting(String)
        case failed(String)
    }

    let hosts: HostStore
    /// The host's settings, read when Settings opens.
    let settings = SettingsModel()
    private(set) var link: Link = .offline
    private(set) var hostID: UUID?
    private(set) var demo = false
    /// Connected to the server on this Mac through its socket, without SSH.
    private(set) var local = false
    /// A host key the user has not trusted yet; `trust(_:)` pins it.
    var untrustedKey: HostKey?
    /// Logging in with the password to add this device's key.
    private(set) var addingKey = false
    /// The password of a key setup that waits for the user to trust the host key.
    private var setUpAfterTrust: String?
    /// The user pressed Disconnect: coming back to the foreground does not connect.
    private var stayDisconnected = false
    private(set) var inbox = Inbox.empty
    private(set) var terminals: [Terminal] = []
    private(set) var routines: [Routine] = []
    private(set) var routineProblems: [RoutineProblem] = []
    /// Workspaces whose Routines are in their file but turned off.
    private(set) var routinesOff: [RoutinesOff] = []
    /// Why the last request failed, shown until the next one.
    var requestError: String?
    /// Followed conversations by chat number.
    private(set) var chats: [UInt32: [ChatEntry]] = [:]
    private(set) var chatPrompts: [UInt32: ChatPrompt] = [:]
    /// Why a chat ended, until it is closed.
    private(set) var chatEnded: [UInt32: String] = [:]
    /// Task Loops by agent terminal.
    private(set) var loops: [String: LoopInfo] = [:]
    /// The Workspaces with their repos and branches, for Focus's sidebar.
    private(set) var workspaceList: [Workspace] = []
    /// Open Reader files by reader number, as the server last sent them.
    private(set) var readerContents: [UInt32: ReaderContent] = [:]
    /// Why a Reader file ended, by reader number, until it is closed.
    private(set) var readerEnded: [UInt32: String] = [:]

    private var client: V2Client?
    private var resume = V2Client.State()
    private var events: Task<Void, Never>?
    private var retry: Task<Void, Never>?
    /// Where each attached channel's output goes (Focus).
    private var sinks: [UInt32: @MainActor (V2Event) -> Void] = [:]
    /// The channel each shown terminal is attached on, for Chat's input.
    private var channelOf: [String: UInt32] = [:]

    static let clientName = "muxa-ios \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?")"

    init(hosts: HostStore) {
        self.hosts = hosts
        settings.client = { [weak self] in
            guard let self else { throw V2Error.lost("not connected") }
            return try self.connected()
        }
    }

    /// Agents waiting on the user: a short-lived state, never an Inbox item.
    var needsYou: [Terminal] {
        terminals.filter { $0.live && $0.isTaskAgent && ["blocked", "waiting"].contains($0.status?.state ?? "") }
    }

    var isConnected: Bool { link == .connected }

    // MARK: Connecting

    func connect(to id: UUID) async {
        guard let host = hosts.host(id) else { return }
        stayDisconnected = false
        if hostID != id { forget() }
        hostID = id
        demo = false
        local = false
        UserDefaults.standard.set(false, forKey: Self.localKey)
        hosts.markUsed(id)
        await open {
            try await SSHExec.connect(
                to: host.target, key: try ClientKeyStore.loadOrCreate(), command: host.target.command + " connect")
        }
    }

    /// Try the Demo: the in-app server, no host.
    func startDemo() async {
        stayDisconnected = false
        forget()
        hostID = nil
        demo = true
        local = false
        await open { DemoChannel() }
    }

    #if os(macOS)
    /// The muxa server on this Mac (ADR 0070 decision 3).
    func connectLocal() async {
        stayDisconnected = false
        if !local { forget() }
        hostID = nil
        demo = false
        local = true
        UserDefaults.standard.set(true, forKey: Self.localKey)
        await open { try UnixSocketBytes(path: UnixSocketBytes.serverSocket.path) }
    }

    /// The daemon runs here: its socket exists.
    var serverIsLocal: Bool { FileManager.default.fileExists(atPath: UnixSocketBytes.serverSocket.path) }

    /// The user's last choice was this Mac.
    var prefersLocal: Bool { UserDefaults.standard.bool(forKey: Self.localKey) }
    #endif

    private static let localKey = "next.connectsLocally"

    /// What Home calls the connection.
    var connectionTitle: String {
        if demo { return "Demo" }
        if local { return "This Mac" }
        return currentHost?.title ?? "Host"
    }

    var currentHost: MuxaSession.Host? { hosts.host(hostID) }

    /// Pins the presented host key, then goes on with what it stopped. It takes
    /// the key: on the Mac, closing the sheet clears `untrustedKey` first.
    func trust(_ key: HostKey) async {
        guard let id = hostID, var host = hosts.host(id) else { return }
        host.target.hostKey = key
        hosts.upsert(host)
        untrustedKey = nil
        if let password = setUpAfterTrust {
            setUpAfterTrust = nil
            await setUpKey(on: id, password: password)
        } else {
            await connect(to: id)
        }
    }

    func cancelTrust() {
        untrustedKey = nil
        setUpAfterTrust = nil
        link = .offline
    }

    /// Logs in once with the password to add this device's key, restricted to
    /// the bridge, then connects. The password is not kept.
    func setUpKey(on id: UUID, password: String) async {
        guard let host = hosts.host(id) else { return }
        hostID = id
        addingKey = true
        defer { addingKey = false }
        do {
            let key = try ClientKeyStore.loadOrCreate()
            let line = try KeySetup.authorizedLine(for: key, command: host.target.command, comment: Self.keyComment)
            _ = try await KeySetup.install(line, on: host.target, password: password)
        } catch let TransportError.hostKeyMismatch(presented) {
            setUpAfterTrust = password
            untrustedKey = presented
            return
        } catch TransportError.notAuthorized {
            link = .failed("The host refused the password. Check the user and password.")
            return
        } catch {
            link = .failed(Self.describe(error))
            return
        }
        await connect(to: id)
    }

    /// The `authorized_keys` line for this device, to add by hand.
    func authorizedKeyLine(command: String) -> String? {
        guard let key = try? ClientKeyStore.loadOrCreate() else { return nil }
        return try? KeySetup.authorizedLine(for: key, command: command, comment: Self.keyComment)
    }

    private static let keyComment = "muxa-ios"

    func disconnect() async {
        stayDisconnected = true
        retry?.cancel()
        await drop()
        link = .offline
    }

    /// Leaves for the background: the server keeps the session for a resume.
    func suspend() async {
        retry?.cancel()
        await drop()
        if link == .connected { link = .offline }
    }

    /// Back in the foreground, or the network came back.
    func resumeIfNeeded() async {
        guard client == nil, link != .connecting, !stayDisconnected else { return }
        if demo { return await startDemo() }
        #if os(macOS)
        if local { return await connectLocal() }
        #endif
        if let hostID { await connect(to: hostID) }
    }

    private func open(_ channel: @escaping () async throws -> any ByteChannel) async {
        retry?.cancel()
        await drop()
        link = .connecting
        do {
            let c = try await V2Client.open(try await channel(), name: Self.clientName, state: resume)
            client = c
            listen(c)
            try await c.watch()
            link = .connected
            await reload()
        } catch let TransportError.hostKeyMismatch(presented) {
            untrustedKey = presented
            link = .failed("Check the host key before you trust it.")
        } catch {
            link = .failed(Self.describe(error))
        }
    }

    private func listen(_ c: V2Client) {
        events = Task { [weak self] in
            for await e in c.events {
                guard let self else { return }
                await self.handle(e, from: c)
            }
        }
    }

    private func handle(_ e: V2Event, from c: V2Client) async {
        switch e {
        case .inbox(let i): inbox = i
        case .changed(let t):
            if let at = terminals.firstIndex(where: { $0.id == t.id }) { terminals[at] = t } else { terminals.append(t) }
        case .removed(let id): terminals.removeAll { $0.id == id }
        case .resync: terminals = (try? await c.terminals()) ?? terminals
        case .output(let ch, _), .paint(let ch), .closed(let ch, _): sinks[ch]?(e)
        case .chat(let n, let update):
            guard let entries = chats[n] else { break }
            chats[n] = update.applied(to: entries)
            chatPrompts[n] = update.prompt
        case .chatEnded(let n, let reason):
            chats[n] = nil
            chatPrompts[n] = nil
            chatEnded[n] = reason
        case .loops(let all): loops = Dictionary(all.map { ($0.terminal, $0) }) { _, b in b }
        case .reader(let n, let content):
            if readerContents[n] != nil { readerContents[n] = content }
        case .readerEnded(let n, let reason):
            readerContents[n] = nil
            readerEnded[n] = reason
        case .lost(let why):
            // The server closes chats and Reader files with the connection.
            for n in chats.keys { chatEnded[n] = "The connection was lost. Open the chat again." }
            chats = [:]
            chatPrompts = [:]
            for n in readerContents.keys { readerEnded[n] = "The connection was lost. Open the file again." }
            readerContents = [:]
            resume = await c.resumeState
            client = nil
            link = .reconnecting(why)
            reconnectSoon()
        }
    }

    /// Tries again with growing waits while the connection is down.
    private func reconnectSoon() {
        retry = Task { [weak self] in
            var wait: Duration = .seconds(1)
            while !Task.isCancelled {
                try? await Task.sleep(for: wait)
                guard let self, !Task.isCancelled, self.client == nil else { return }
                await self.resumeIfNeeded()
                if self.isConnected { return }
                wait = min(wait * 2, .seconds(30))
            }
        }
    }

    private func reload() async {
        guard let c = client else { return }
        do {
            inbox = try await c.inbox()
            terminals = try await c.terminals()
            (routines, routineProblems, routinesOff) = try await c.routines()
            loops = Dictionary(try await c.loops().map { ($0.terminal, $0) }) { _, b in b }
            workspaceList = try await c.workspaces()
        } catch {
            requestError = Self.describe(error)
        }
    }

    private func drop() async {
        events?.cancel()
        if let c = client {
            resume = await c.resumeState
            await c.close()
        }
        client = nil
    }

    /// A new host or the demo: nothing of the old session carries over.
    private func forget() {
        resume = V2Client.State()
        sinks = [:]
        inbox = .empty
        terminals = []
        routines = []
        routineProblems = []
        readerContents = [:]
        readerEnded = [:]
        loops = [:]
        workspaceList = []
        chats = [:]
        chatPrompts = [:]
        chatEnded = [:]
        channelOf = [:]
        settings.reset()
    }

    // MARK: Requests

    /// Runs an Inbox request; the reply replaces the Inbox.
    func inbox(_ r: V2Request) async {
        await run { self.inbox = try await $0.inbox(r) }
    }

    /// Closes the Review item's Task. A worktree that holds work stops it:
    /// the server's message comes back, for the user to confirm `force`.
    func approve(_ item: String, force: Bool = false) async -> String? {
        var wouldLoseWork: String?
        await run { c in
            do {
                self.inbox = try await c.inbox(.inboxApprove(item: item, worktree: force ? .force : nil))
            } catch V2Error.refused(let code, let message) where code == "would-lose-work" {
                wouldLoseWork = message
            }
        }
        return wouldLoseWork
    }

    // MARK: Start Task

    func workspaces() async throws -> [Workspace] {
        try await connected().workspaces()
    }

    /// Reads the Workspaces again: a repo's branch may have moved.
    func refreshWorkspaces() async {
        await run { self.workspaceList = try await $0.workspaces() }
    }

    /// Starts the agent in the repo's own checkout (ADR 0077); when one runs
    /// there already, the server returns it.
    func startCheckout(workspace: Workspace, repo: String) async -> Terminal? {
        let title = [repo, workspace.branches[repo]].compactMap { $0 }.joined(separator: " ")
        var started: Terminal?
        await run { _ in
            started = try await self.startTask(TaskRequest(
                workspace: workspace.name, title: title, places: [.checkout(repo: repo)], plan: false))
        }
        return started
    }

    /// Starts a Task and keeps its agent terminal, so Focus can show it at once.
    func startTask(_ request: TaskRequest) async throws -> Terminal {
        let (t, _) = try await connected().startTask(
            workspace: request.workspace, title: request.title, goal: request.goal, places: request.places,
            plan: request.plan, loop: request.loop)
        if let at = terminals.firstIndex(where: { $0.id == t.id }) { terminals[at] = t } else { terminals.append(t) }
        return t
    }

    /// Resume Task: starts the agent in the item's place, then marks the item Done.
    /// The agent's terminal comes back once it started, even when Done failed.
    func resumeTask(_ item: InboxItem) async -> Terminal? {
        guard let place = item.place else { return nil }
        var started: Terminal?
        await run { c in
            started = try await self.startTask(TaskRequest(
                workspace: place.workspace, title: item.resumeTitle, places: [.existing(path: place.path)], plan: false))
            self.inbox = try await c.inbox(.inboxDone(item: item.id))
        }
        return started
    }

    private func connected() throws -> V2Client {
        guard let client else { throw V2Error.lost("not connected") }
        return client
    }

    func parseCapture(_ text: String) async -> Capture? {
        var parsed: Capture?
        await run { parsed = try await $0.parseCapture(text, today: Self.today()) }
        return parsed
    }

    func runRoutine(_ r: Routine) async {
        await run {
            try await $0.runRoutine(workspace: r.workspace, name: r.name)
            (self.routines, self.routineProblems, self.routinesOff) = try await $0.routines()
        }
    }

    func refreshRoutines() async {
        await run { (self.routines, self.routineProblems, self.routinesOff) = try await $0.routines() }
    }

    /// Opens the Routines gate of `workspace` at the current settings
    /// revision, keeping its direct writes as they are.
    func turnOnRoutines(in workspace: String) async {
        if settings.settings == nil { await settings.load() }
        guard let w = settings.workspace(workspace) else {
            requestError = settings.loadError ?? "\(workspace) is not in the settings any more."
            return
        }
        let failure = await settings.save([.routinesGate(workspace: workspace, enabled: true, directWrites: w.directWrites)])
        await refreshRoutines()
        if let failure { requestError = failure }
    }

    private func run(_ body: (V2Client) async throws -> Void) async {
        guard let c = client else {
            requestError = "Not connected."
            return
        }
        do {
            requestError = nil
            try await body(c)
        } catch {
            requestError = Self.describe(error)
        }
    }

    // MARK: Chat

    /// Follows the conversation of a Task's agent; it stays in `chats` until `closeChat`.
    func openChat(_ terminal: String) async -> UInt32? {
        var opened: UInt32?
        await run {
            let (n, update) = try await $0.chatOpen(terminal)
            self.chats[n] = update.applied(to: [])
            self.chatPrompts[n] = update.prompt
            opened = n
        }
        return opened
    }

    func closeChat(_ n: UInt32) async {
        let open = chats.removeValue(forKey: n) != nil
        chatPrompts[n] = nil
        chatEnded[n] = nil
        if open { try? await client?.chatClose(n) }
    }

    /// Types `text` into the terminal through the channel Focus shows it on.
    /// Attaching only for this would resize the agent's terminal.
    func send(_ text: String, to terminal: String) async {
        guard let ch = channelOf[terminal] else {
            requestError = "Show the Task's terminal to answer it."
            return
        }
        requestError = nil
        await input(ch, Array(text.utf8))
    }

    // MARK: Task Loop

    /// Starts or restarts the loop of a Task's agent.
    func startLoop(_ terminal: String, spec: LoopSpec, reset: Bool) async {
        await run { self.loops[terminal] = try await $0.loopStart(terminal, spec: spec, reset: reset) }
    }

    func stopLoop(_ terminal: String) async {
        await run { self.loops[terminal] = try await $0.loopStop(terminal) }
    }

    // MARK: Reader

    /// The files of `kind` in the terminal's folder; documents with HTML ones.
    func readerList(_ terminal: String, kind: ReaderKind) async -> ReaderList? {
        var list: ReaderList?
        await run { list = try await $0.readerList(terminal, kind: kind, html: kind == .docs) }
        return list
    }

    /// Opens a listed file; its content stays in `readerContents` until `closeReader`.
    func openReader(_ terminal: String, kind: ReaderKind, path: String, scope: DiffScope) async -> UInt32? {
        var opened: UInt32?
        await run {
            let (n, content) = try await $0.readerOpen(terminal, kind: kind, path: path, html: kind == .docs, scope: scope)
            self.readerContents[n] = content
            opened = n
        }
        return opened
    }

    func closeReader(_ n: UInt32) async {
        let open = readerContents.removeValue(forKey: n) != nil
        readerEnded[n] = nil
        if open { try? await client?.readerClose(n) }
    }

    // MARK: Terminals (Focus)

    /// Attaches `terminal`; its events go to `sink` until `detach`.
    func attach(_ terminal: String, cols: UInt16, rows: UInt16, sink: @escaping @MainActor (V2Event) -> Void) async -> UInt32? {
        guard let c = client else { return nil }
        do {
            let ch = try await c.attach(terminal, cols: cols, rows: rows, history: 2000)
            sinks[ch] = sink
            channelOf[terminal] = ch
            return ch
        } catch {
            requestError = Self.describe(error)
            return nil
        }
    }

    func detach(_ channel: UInt32) async {
        sinks[channel] = nil
        channelOf = channelOf.filter { $0.value != channel }
        try? await client?.detach(channel)
    }

    func input(_ channel: UInt32, _ bytes: [UInt8]) async {
        try? await client?.input(channel, bytes)
    }

    func resize(_ channel: UInt32, cols: UInt16, rows: UInt16) async {
        await client?.resize(channel, cols: cols, rows: rows)
    }

    // MARK: Helpers

    /// The device's local date, `YYYY-MM-DD`, as Capture reads `due`.
    static func today(_ now: Date = .now) -> String {
        let c = Calendar.current.dateComponents([.year, .month, .day], from: now)
        return String(format: "%04d-%02d-%02d", c.year ?? 0, c.month ?? 0, c.day ?? 0)
    }

    static func describe(_ error: any Error) -> String {
        switch error {
        case V2Error.refused(let code, let message) where code == "version":
            return "The host runs an older muxa: \(message). Update it with make install on the host."
        case V2Error.refused(_, let message): return message
        case V2Error.lost(let why): return "The connection was lost: \(why)"
        case TransportError.notAuthorized: return "The host refused this device's key. Add it in Settings."
        case TransportError.failed(let why): return why
        default: return error.localizedDescription
        }
    }
}
