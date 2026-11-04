import Foundation
import MuxaProtocol
import MuxaTransport
import Observation

/// One connection to a bridge: hello, the session list, and the watched pane.
/// The UI reads its state; the renderer gets the pane's bytes.
@MainActor
@Observable
public final class PaneSession {
    public enum Phase: Equatable, Sendable {
        case idle
        case connecting
        case connected
        case disconnected(reason: String)
    }

    public enum Failure: Error, Equatable {
        case notConnected
        case bridge(ErrorBody)
        case protocolMismatch(bridge: UInt32)
        case noHello
    }

    public typealias Opener = @Sendable () async throws -> any BridgeChannel

    public private(set) var phase: Phase = .idle
    public private(set) var tmuxVersion: String?
    public private(set) var sessions: [Session] = []
    /// The pane shown, and kept across reconnects.
    public private(set) var watchedPane: String?
    /// Set when the watched pane disappears; cleared by the next watch.
    public private(set) var paneGoneReason: String?
    /// The last failed input or command, for a short notice in the UI.
    public private(set) var lastError: String?
    /// What the bridge supports beyond the base protocol, from `hello`.
    public private(set) var capabilities: [String] = []
    /// The followed agent conversation, kept across reconnects like the watch.
    public private(set) var chat: ChatLog?
    /// Panes whose program no agent adapter knows; they show only the terminal.
    public private(set) var unsupportedChats: Set<String> = []
    /// The document open in the Reader, kept across reconnects.
    public private(set) var doc: DocLog?
    /// The open diff, kept across reconnects.
    public private(set) var diff: DiffLog?
    /// The open image, kept across reconnects.
    public private(set) var image: ImageLog?
    /// The host's task records, as last listed (`task-records`).
    public private(set) var taskRecords: [TaskRecord] = []
    /// The host's workspaces, from its config.
    public private(set) var workspaces: [WorkspaceInfo] = []
    public private(set) var workers: [WorkerInfo] = []
    /// The last listing left some records out.
    public private(set) var tasksTruncated = false
    /// Why the task records could not be listed; the last list stays.
    public private(set) var taskError: String?
    /// The grid the pane area shows at the default font, kept across reconnects.
    public private(set) var viewport: PaneViewport?
    /// The host's Routines, as last listed (`work` level 1, ADR 0026).
    public internal(set) var routines: [RoutineInfo] = []
    /// The stored Inbox entries, newest first, as last listed.
    public internal(set) var inboxEntries: [InboxEntry] = []
    /// The host's Inbox items in the host's order, as last listed (`work` level 3, ADR 0033).
    public internal(set) var inboxItems: [InboxItem] = []
    /// Why the Routines or the stored entries could not be listed; the last lists stay.
    public internal(set) var workError: String?

    @ObservationIgnored public weak var renderer: (any PaneRenderer)?

    @ObservationIgnored private var channel: (any BridgeChannel)?
    @ObservationIgnored private var outbox: AsyncStream<Data>.Continuation?
    @ObservationIgnored private var writer: Task<Void, Never>?
    @ObservationIgnored private var tasks: [Task<Void, Never>] = []
    @ObservationIgnored private var nextId: UInt64 = 0
    @ObservationIgnored private var watchId: UInt64?
    @ObservationIgnored private var chatId: UInt64?
    @ObservationIgnored private var docId: UInt64?
    @ObservationIgnored private var diffId: UInt64?
    @ObservationIgnored private var changeListings: [UInt64: ChangeListing] = [:]
    @ObservationIgnored private var imageId: UInt64?
    @ObservationIgnored private var imageListings: [UInt64: ImageListing] = [:]
    /// `doc-list` messages waiting for their `result`.
    @ObservationIgnored private var listings: [UInt64: DocListing] = [:]
    /// `task-list` messages waiting for their `result`.
    @ObservationIgnored private var taskListings: [UInt64: TaskListing] = [:]
    /// Records from `task-saved`, by request id, until the result arrives.
    @ObservationIgnored var savedTasks: [UInt64: TaskRecord] = [:]
    /// Session names from `task-created`, by request id, until the result arrives.
    @ObservationIgnored private var createdTasks: [UInt64: String] = [:]
    /// `routine-list`, `inbox-list`, `routine-saved` and `inbox-saved`, by request id,
    /// until the result arrives (`PaneSession+Work.swift`).
    @ObservationIgnored var routineListings: [UInt64: [RoutineInfo]] = [:]
    @ObservationIgnored var inboxListings: [UInt64: (entries: [InboxEntry], items: [InboxItem])] = [:]
    @ObservationIgnored var savedRoutines: [UInt64: RoutineInfo] = [:]
    @ObservationIgnored var savedEntries: [UInt64: InboxEntry] = [:]
    @ObservationIgnored var savedItems: [UInt64: InboxItem] = [:]
    /// `session-started`, by request id, until the result arrives (`PaneSession+Ask.swift`).
    @ObservationIgnored var startedSessions: [UInt64: StartedSession] = [:]
    @ObservationIgnored private var pending: [UInt64: CheckedContinuation<Message, any Error>] = [:]
    /// Ids of input requests in flight; their results only matter when they fail.
    @ObservationIgnored private var inputs: Set<UInt64> = []
    @ObservationIgnored private var hello: CheckedContinuation<Void, any Error>?
    /// The viewport this bridge has; nil after a connect.
    @ObservationIgnored private var sentViewport: PaneViewport?
    @ObservationIgnored private var viewportTask: Task<Void, Never>?
    @ObservationIgnored private let pingEvery: Duration

    public init(pingEvery: Duration = .seconds(15)) {
        self.pingEvery = pingEvery
    }

    // MARK: Lifecycle

    /// Opens a channel, waits for `hello`, and lists sessions. On a reconnect it
    /// watches the same pane again, unless `samePanes` is false (another host).
    public func connect(_ open: Opener, samePanes: Bool = true) async throws {
        await teardown(reason: nil)
        if !samePanes {
            watchedPane = nil
            paneGoneReason = nil
            sessions = []
            chat = nil
            unsupportedChats = []
            doc = nil
            diff = nil
            image = nil
            taskRecords = []
            workspaces = []
            workers = []
            tasksTruncated = false
            taskError = nil
            routines = []
            inboxEntries = []
            inboxItems = []
            workError = nil
        }
        phase = .connecting
        do {
            let channel = try await open()
            start(channel)
            try await waitForHello()
            phase = .connected
            // Before the watch, so the pane's window is sized for this device first.
            sendViewport()
            try await refresh()
            await loadTasks()
            await loadWork()
            if let pane = watchedPane { try await watch(pane: pane) }
            if let pane = chat?.pane {
                chat = nil
                await follow(pane: pane)
            }
            if let open = doc {
                doc = nil
                await openDocument(pane: open.pane, path: open.path)
            }
            if let open = diff {
                diff = nil
                await openDiff(pane: open.pane, path: open.path, scope: open.scope)
            }
            if let open = image {
                image = nil
                await openImage(pane: open.pane, path: open.path)
            }
        } catch {
            await teardown(reason: Self.describe(error))
            throw error
        }
    }

    /// Sends `bye` and closes the channel.
    public func disconnect() async {
        if channel != nil { enqueue(.bye(id: takeId())) }
        await teardown(reason: "disconnected")
    }

    // MARK: Requests

    public func refresh() async throws {
        let reply = try await request { .list(id: $0) }
        if case .result(_, _, let sessions?) = reply { self.sessions = sessions }
    }

    /// Shows this pane. Messages for any earlier watch are dropped from now on.
    public func watch(pane: String) async throws {
        watchedPane = pane
        paneGoneReason = nil
        try await request { id in
            self.watchId = id
            return .watch(id: id, pane: pane)
        }
    }

    /// The pane area changed (rotation, split view, fold, keyboard). The bridge
    /// gets the first viewport at once and later ones once they settle for 300 ms,
    /// so a drag or an animation resizes the window once (ADR 0020).
    public func setViewport(_ new: PaneViewport) {
        guard new != viewport else { return }
        viewport = new
        viewportTask?.cancel()
        guard sentViewport != nil else {
            sendViewport()
            return
        }
        viewportTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(300))
            guard !Task.isCancelled else { return }
            self?.sendViewport()
        }
    }

    private func sendViewport() {
        guard let viewport, viewport != sentViewport, channel != nil, capabilities.contains("viewport") else {
            return
        }
        sentViewport = viewport
        let id = takeId()
        inputs.insert(id)
        enqueue(.viewport(id: id, cols: viewport.cols, rows: viewport.rows))
    }

    public var canChat: Bool { capabilities.contains("chat") }

    /// Follows the agent conversation in `pane`. A pane no adapter knows is
    /// remembered in `unsupportedChats`; other failures end up in `chat.ended`.
    public func follow(pane: String) async {
        guard canChat, !unsupportedChats.contains(pane) else { return }
        if chat?.pane == pane, chat?.ended == nil { return }
        chat = ChatLog(pane: pane)
        do {
            try await request { id in
                self.chatId = id
                return .chat(id: id, pane: pane)
            }
        } catch Failure.bridge(let body) where body.code == .unsupported {
            unsupportedChats.insert(pane)
            if chat?.pane == pane { chat = nil }
        } catch {
            if chat?.pane == pane { chat?.ended = Self.describe(error) }
        }
    }

    public func unfollow() async {
        guard chat != nil else { return }
        chat = nil
        chatId = nil
        try? await request { .unchat(id: $0) }
    }

    public var canReadDocs: Bool { capabilities.contains("docs") }

    /// The pane's documents: its agent's first, then its repo's.
    public func documents(pane: String) async throws -> DocListing {
        var requestId: UInt64 = 0
        try await request { id in
            requestId = id
            return .docs(id: id, pane: pane)
        }
        // The list arrives just before the result.
        return listings.removeValue(forKey: requestId) ?? DocListing(docs: requestId, entries: [])
    }

    /// Opens a listed document and follows its changes. A failure ends up in `doc.ended`.
    public func openDocument(pane: String, path: String) async {
        guard canReadDocs else { return }
        if doc?.pane == pane, doc?.path == path, doc?.ended == nil { return }
        doc = DocLog(pane: pane, path: path)
        do {
            try await request { id in
                self.docId = id
                return .doc(id: id, pane: pane, path: path)
            }
        } catch {
            if doc?.path == path { doc?.ended = Self.describe(error) }
        }
    }

    public func closeDocument() async {
        guard doc != nil else { return }
        doc = nil
        docId = nil
        try? await request { .undoc(id: $0) }
    }

    public var canShowChanges: Bool { capabilities.contains("changes") }

    /// The files of the pane's repo that differ from HEAD.
    public func changes(pane: String) async throws -> ChangeListing {
        var requestId: UInt64 = 0
        try await request { id in
            requestId = id
            return .changes(id: id, pane: pane)
        }
        // The list arrives just before the result.
        return changeListings.removeValue(forKey: requestId) ?? ChangeListing(changes: requestId, entries: [])
    }

    /// Opens a listed file's diff and follows it. A failure ends up in `diff.ended`.
    /// A branch diff needs the `branch-changes` capability.
    public func openDiff(pane: String, path: String, scope: DiffScope = .uncommitted) async {
        guard canShowChanges else { return }
        if diff?.pane == pane, diff?.path == path, diff?.scope == scope, diff?.ended == nil { return }
        diff = DiffLog(pane: pane, path: path, scope: scope)
        do {
            try await request { id in
                self.diffId = id
                // Absent for uncommitted, so older bridges understand it.
                return .diff(id: id, pane: pane, path: path, scope: scope == .branch ? .branch : nil)
            }
        } catch {
            if diff?.path == path { diff?.ended = Self.describe(error) }
        }
    }

    public func closeDiff() async {
        guard diff != nil else { return }
        diff = nil
        diffId = nil
        try? await request { .undiff(id: $0) }
    }

    public var canShowImages: Bool { capabilities.contains("images") }

    /// The image files of the pane's repo, newest first.
    public func images(pane: String) async throws -> ImageListing {
        var requestId: UInt64 = 0
        try await request { id in
            requestId = id
            return .images(id: id, pane: pane)
        }
        // The list arrives just before the result.
        return imageListings.removeValue(forKey: requestId) ?? ImageListing(images: requestId, entries: [])
    }

    /// Opens a listed image and follows it. A failure ends up in `image.ended`.
    public func openImage(pane: String, path: String) async {
        guard canShowImages else { return }
        if image?.pane == pane, image?.path == path, image?.ended == nil { return }
        image = ImageLog(pane: pane, path: path)
        do {
            try await request { id in
                self.imageId = id
                return .image(id: id, pane: pane, path: path)
            }
        } catch {
            if image?.path == path { image?.ended = Self.describe(error) }
        }
    }

    public func closeImage() async {
        guard image != nil else { return }
        image = nil
        imageId = nil
        try? await request { .unimage(id: $0) }
    }

    public var canCloseTasks: Bool { capabilities.contains("close-task") }

    /// Closes a task's session, then lists again. Throws the bridge's reason
    /// when nothing was closed.
    public func closeTask(session: String, mode: CloseMode) async throws {
        try await request { .closeTask(id: $0, session: session, mode: mode) }
        try await refresh()
    }

    public var canStartTasks: Bool { capabilities.contains("tasks") }

    /// Starts a task on `branch` of the repo at `repo`, and returns its session's
    /// name once the session list has it.
    public func newTask(repo: String, branch: String, base: String?) async throws -> String {
        var requestId: UInt64 = 0
        try await request { id in
            requestId = id
            return .newTask(id: id, repo: repo, branch: branch, base: base)
        }
        // The name arrives just before the result.
        guard let name = createdTasks.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "the bridge did not name the task's session"))
        }
        try await refresh()
        return name
    }

    public var canRecoverRestarts: Bool { capabilities.contains("task-restart-recovery") }

    public var canUseWorkers: Bool { capabilities.contains("workers") }

    public var canKeepTasks: Bool { capabilities.contains("task-records") }

    /// Lists the task records and workspaces again. A failure lands in `taskError`.
    public func loadTasks() async {
        guard canKeepTasks else { return }
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .listTasks(id: id)
            }
        } catch {
            taskError = Self.describe(error)
            return
        }
        // The list arrives just before the result.
        guard let listing = taskListings.removeValue(forKey: requestId) else { return }
        taskRecords = listing.entries
        workspaces = listing.workspaces
        workers = listing.workers
        tasksTruncated = listing.truncated
        taskError = nil
    }

    /// Records a task and starts its agent. A failed launch still keeps the record,
    /// with a `note` saying why; it is returned then too. Throws when nothing was kept.
    public func createTask(_ new: NewTaskRecord) async throws -> TaskRecord {
        if new.loop != nil { try requireLoops() }
        if let worker = new.worker {
            try requireWorkers()
            try requireWorkerChoice(worker, workspace: new.workspace)
        }
        var requestId: UInt64 = 0
        var failure: (any Error)?
        do {
            try await request { id in
                requestId = id
                return .createTask(
                    id: id, workspace: new.workspace, title: new.title, goal: new.goal, project: new.project,
                    repo: new.repo, branch: new.branch, base: new.base, loop: new.loop, worker: new.worker)
            }
        } catch {
            failure = error
        }
        // The record arrives just before the result.
        let record = savedTasks.removeValue(forKey: requestId)
        try? await refresh()
        await loadTasks()
        if let record { return record }
        throw failure ?? Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the task."))
    }

    /// Shown when another device changed the task first.
    public static let taskConflict = "This task changed on another device. The list now shows its latest state. Try again."

    /// Sets a task's status. When the record changed since `record.rev`, the list is
    /// loaded again and the error says so; nothing changed then.
    public func updateTask(_ record: TaskRecord, status: TaskStatus) async throws -> TaskRecord {
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .updateTask(id: id, task: record.id, rev: record.rev, status: status)
            }
        } catch Failure.bridge(let body) where body.code == .conflict {
            await loadTasks()
            throw Failure.bridge(ErrorBody(code: .conflict, message: Self.taskConflict))
        }
        let saved = savedTasks.removeValue(forKey: requestId)
        await loadTasks()
        return saved ?? record
    }

    private func requireWorkerChoice(_ name: String, workspace: String) throws {
        if let problem = workerProblem(name: name, workspace: workspace, workers: workers) {
            throw Failure.bridge(ErrorBody(code: .badRequest, message: problem))
        }
    }

    private func requireWorkers() throws {
        guard canUseWorkers else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: "This host does not support workers."))
        }
    }

    public var canRestartAgents: Bool { capabilities.contains("task-handoff") }

    /// Shown when the bridge cannot restart agents.
    public static let restartUnsupported = "This host's bridge can't restart agents. Update muxa on the host."

    /// Restarts a task's agent in its pane, after a handoff (ADR 0018). Without
    /// `force`, the bridge refuses a running agent. Returns the record with its new run.
    /// `worker` is a name or `default`; nil keeps the current worker (ADR 0027).
    public func restartAgent(task: String, force: Bool, worker: String? = nil, rev: UInt64? = nil) async throws -> TaskRecord {
        if taskRecords.first(where: { $0.id == task })?.restartOperation?.blocksRestart == true {
            throw Failure.bridge(ErrorBody(code: .conflict, message: "Resolve the interrupted restart before restarting again."))
        }
        if let worker {
            try requireWorkers()
            guard let record = taskRecords.first(where: { $0.id == task }) else {
                throw Failure.bridge(ErrorBody(code: .badRequest, message: "Reload the task before restarting."))
            }
            try requireWorkerChoice(worker, workspace: record.workspace)
        }
        // The bridge checks a revision only when one is sent.
        let revision = canUseWorkers ? (rev ?? taskRecords.first { $0.id == task }?.rev) : nil
        guard canRestartAgents else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.restartUnsupported))
        }
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .restartAgent(id: id, task: task, force: force, worker: worker, rev: revision)
            }
        } catch {
            savedTasks[requestId] = nil
            if case Failure.bridge(let body) = error, body.code == .conflict { await loadTasks() }
            throw error
        }
        // The record arrives just before the result, and already replaced the listed one.
        let saved = savedTasks.removeValue(forKey: requestId)
        // The agent pane's state changes with the restart.
        try? await refresh()
        guard let saved else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the task."))
        }
        return saved
    }

    /// Explicit action bound to the operation/revision the user reviewed. Never retries automatically.
    public func recoverRestart(_ record: TaskRecord, operation: String, action: RestartRecoveryAction) async throws -> TaskRecord {
        guard canRecoverRestarts else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: "This host does not support restart recovery."))
        }
        guard let summary = record.restartOperation, summary.id == operation, summary.recoveryActions.contains(action),
              let current = taskRecords.first(where: { $0.id == record.id })?.restartOperation,
              current.id == operation, current.recoveryActions.contains(action) else {
            throw Failure.bridge(ErrorBody(code: .conflict, message: "Reload the task and review its recovery options again."))
        }
        do {
            return try await saving { .recoverRestart(id: $0, task: record.id, rev: record.rev, operation: operation, action: action) }
        } catch {
            await loadTasks()
            throw error
        }
    }

    /// The host's bridge reached the muxa daemon, so Tasks can have a Task Loop (ADR 0019).
    public var canLoop: Bool { capabilities.contains("task-loop") }

    /// Starts a task's Task Loop with `loop`, also a stopped or passed one. `reset`
    /// counts attempts from zero. Throws the host's reason, e.g. a check the workspace
    /// does not allow; a bridge without `task-loop` is never asked.
    public func startLoop(_ record: TaskRecord, loop: LoopSpec, reset: Bool = false) async throws -> TaskRecord {
        try requireLoops()
        return try await saving { .startLoop(id: $0, task: record.id, loop: loop, reset: reset) }
    }

    /// Stops a task's Task Loop, with reason `user`.
    public func stopLoop(_ record: TaskRecord) async throws -> TaskRecord {
        try requireLoops()
        return try await saving { .stopLoop(id: $0, task: record.id) }
    }

    private func requireLoops() throws {
        guard canLoop else { throw Failure.bridge(ErrorBody(code: .unsupported, message: loopNeedsDaemon)) }
    }

    /// Sends a request that answers with `task-saved`. The record is kept as it arrives,
    /// also one sent before a failure.
    private func saving(_ make: (UInt64) -> Request) async throws -> TaskRecord {
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return make(id)
            }
        } catch {
            savedTasks[requestId] = nil
            throw error
        }
        guard let saved = savedTasks.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the task."))
        }
        return saved
    }

    /// A newer record replaces the one listed; an older one is ignored.
    private func keep(_ record: TaskRecord) {
        if let i = taskRecords.firstIndex(where: { $0.id == record.id }) {
            if taskRecords[i].rev <= record.rev { taskRecords[i] = record }
        } else {
            taskRecords.append(record)
        }
    }

    /// Asks for the watched pane's whole screen again, e.g. after the view lost it.
    public func resync() async {
        guard let pane = watchedPane else { return }
        try? await watch(pane: pane)
    }

    public func send(keys: [String]) { fire { .keys(id: $0, pane: $1, keys: keys) } }
    public func send(text: String) { fire { .text(id: $0, pane: $1, text: text) } }
    public func send(bytes: Data) { fire { .bytes(id: $0, pane: $1, data: bytes) } }
    public func paste(_ text: String) { fire { .paste(id: $0, pane: $1, text: text) } }

    /// Input for the watched pane, queued synchronously so it keeps call order.
    /// Its result is not awaited; a failure lands in `lastError`.
    private func fire(_ make: (UInt64, String) -> Request) {
        guard let pane = watchedPane, channel != nil else { return }
        let req = make(takeId(), pane)
        inputs.insert(req.id)
        enqueue(req)
    }

    @discardableResult
    func request(_ make: (UInt64) -> Request) async throws -> Message {
        guard channel != nil else { throw Failure.notConnected }
        let req = make(takeId())
        // Registered before the line is queued, so the result cannot arrive first.
        return try await withCheckedThrowingContinuation { cont in
            pending[req.id] = cont
            enqueue(req)
        }
    }

    // MARK: Plumbing

    private func takeId() -> UInt64 {
        nextId += 1
        return nextId
    }

    private func enqueue(_ req: Request) {
        guard let line = try? Wire.encode(req) else { return }
        outbox?.yield(line)
    }

    private func start(_ channel: any BridgeChannel) {
        self.channel = channel
        let (lines, outbox) = AsyncStream<Data>.makeStream()
        self.outbox = outbox
        let pingEvery = self.pingEvery
        // One writer keeps requests in the order they were made.
        writer = Task {
            for await line in lines {
                do { try await channel.send(line) } catch { break }
            }
        }
        tasks = [
            Task { [weak self] in
                do {
                    for try await line in channel.lines {
                        self?.receive(line)
                    }
                    await self?.channelClosed()
                } catch {
                    await self?.teardown(reason: Self.describe(error))
                }
            },
            Task { [weak self] in
                while !Task.isCancelled {
                    try? await Task.sleep(for: pingEvery)
                    guard let self, self.channel != nil else { return }
                    self.enqueue(.ping(id: self.takeId()))
                }
            },
        ]
    }

    /// Closed before hello means the bridge command exited, most often because it is not installed.
    private func channelClosed() async {
        guard let hello else {
            await teardown(reason: "Connection closed.")
            return
        }
        self.hello = nil
        hello.resume(throwing: Failure.noHello)
        await teardown(reason: Self.describe(Failure.noHello))
    }

    private func waitForHello() async throws {
        let timeout = Task { [weak self] in
            try? await Task.sleep(for: .seconds(10))
            guard !Task.isCancelled, let self else { return }
            self.hello?.resume(throwing: Failure.noHello)
            self.hello = nil
        }
        defer { timeout.cancel() }
        // Set before the reader can run: both are on the main actor.
        try await withCheckedThrowingContinuation { hello = $0 }
    }

    private func receive(_ line: Data) {
        guard let message = try? Wire.decodeMessage(line) else { return }
        switch message {
        case .hello(let version, _, let tmux, let capabilities):
            if version == protocolVersion {
                tmuxVersion = tmux
                self.capabilities = capabilities
                hello?.resume()
            } else {
                hello?.resume(throwing: Failure.protocolMismatch(bridge: version))
            }
            hello = nil
        case .result(let id?, let error, _) where inputs.contains(id):
            inputs.remove(id)
            if let error { lastError = error.message }
        case .result(let id?, let error, _):
            guard let cont = pending.removeValue(forKey: id) else { return }
            if let error { cont.resume(throwing: Failure.bridge(error)) } else { cont.resume(returning: message) }
        case .result(nil, let error, _):
            lastError = error?.message
        case .screen(let watch, _, let cols, let rows, let data) where watch == watchId:
            renderer?.reset(cols: Int(cols), rows: Int(rows), data: data)
        case .output(let watch, _, let data) where watch == watchId:
            renderer?.feed(data)
        case .chatEntries(let update) where update.chat == chatId:
            chat?.apply(update)
        case .chatEnded(let id, let reason) where id == chatId:
            chat?.ended = reason
        case .docList(let listing):
            listings[listing.docs] = listing
        case .changeList(let listing):
            changeListings[listing.changes] = listing
        case .diffContent(let id, _, let text, let binary) where id == diffId:
            diff?.text = text
            diff?.binary = binary
            diff?.ended = nil
        case .diffEnded(let id, let reason) where id == diffId:
            diff?.ended = reason
        case .imageList(let listing):
            imageListings[listing.images] = listing
        case .imageContent(let id, _, let data, let mtime) where id == imageId:
            image?.data = data
            image?.mtime = mtime
            image?.ended = nil
        case .imageEnded(let id, let reason) where id == imageId:
            image?.ended = reason
        case .taskCreated(let task, let session):
            createdTasks[task] = session
        case .docContent(let id, _, let text, let mtime) where id == docId:
            doc?.text = text
            doc?.mtime = mtime
            doc?.ended = nil
        case .docEnded(let id, let reason) where id == docId:
            doc?.ended = reason
        case .paneGone(let watch, _, let reason) where watch == watchId:
            paneGoneReason = reason ?? "the pane closed"
        case .taskList(let listing):
            taskListings[listing.tasks] = listing
        case .taskSaved(let task, let record):
            savedTasks[task] = record
            keep(record)
        case .sessionsChanged:
            // A session that starts or ends can change which task records have an agent.
            Task {
                try? await refresh()
                await loadTasks()
                await loadWork()
            }
        case .tasksChanged:
            // A Routine run is a task record too, so its Routine may have changed.
            Task {
                await loadTasks()
                await loadWork()
            }
        case .routineList(let id, let entries):
            routineListings[id] = entries
        case .routineSaved(let id, let entry):
            savedRoutines[id] = entry
            keep(entry)
        case .inboxList(let id, let entries, let items):
            inboxListings[id] = (entries, items)
        case .inboxSaved(let id, let entry, let item):
            // A Routine's item comes with its entry; a Task's item has none.
            if let entry {
                savedEntries[id] = entry
                keep(entry)
            }
            if let item {
                savedItems[id] = item
                keep(item)
            }
        case .sessionStarted(let start, let session, let name):
            startedSessions[start] = StartedSession(id: session, name: name)
        case .bye(let reason):
            Task { await teardown(reason: reason) }
        case .screen, .output, .paneGone, .chatEntries, .chatEnded, .docContent, .docEnded, .unknown,
            .diffContent, .diffEnded, .imageContent, .imageEnded:
            break
        }
    }

    private func teardown(reason: String?) async {
        guard let channel else {
            // Keep the first reason: a failed connect tears down twice.
            if let reason, phase == .connecting { phase = .disconnected(reason: reason) }
            return
        }
        // Everything shared is reset before the first await: a new connect can start
        // while this one drains, and must not lose its own state to it.
        self.channel = nil
        phase = .disconnected(reason: reason ?? "disconnected")
        tasks.forEach { $0.cancel() }
        tasks = []
        let writer = self.writer
        self.writer = nil
        // Let queued lines (a final `bye`) go out, but do not wait on a dead link.
        outbox?.finish()
        outbox = nil
        watchId = nil
        chatId = nil
        docId = nil
        diffId = nil
        imageId = nil
        listings = [:]
        changeListings = [:]
        imageListings = [:]
        taskListings = [:]
        savedTasks = [:]
        createdTasks = [:]
        routineListings = [:]
        inboxListings = [:]
        savedRoutines = [:]
        savedEntries = [:]
        savedItems = [:]
        startedSessions = [:]
        inputs = []
        sentViewport = nil
        viewportTask?.cancel()
        viewportTask = nil
        let failed = pending
        pending = [:]
        failed.values.forEach { $0.resume(throwing: Failure.notConnected) }
        hello?.resume(throwing: Failure.notConnected)
        hello = nil
        if let writer {
            let drained = Task { await writer.value }
            let timeout = Task {
                try? await Task.sleep(for: .seconds(1))
                writer.cancel()
            }
            await drained.value
            timeout.cancel()
        }
        await channel.close()
    }

    /// A message for the user, saying what to do next where there is something to do.
    public nonisolated static func describe(_ error: any Error) -> String {
        switch error {
        case Failure.bridge(let body): body.message
        case Failure.protocolMismatch(let v): "The bridge speaks protocol \(v); this app speaks \(protocolVersion). Update one of them."
        case Failure.noHello: "The bridge did not start. Check that muxa-bridge is installed on the host and that the bridge command points to it."
        case Failure.notConnected: "Not connected."
        case TransportError.hostKeyMismatch: "The host key changed or is not trusted yet."
        case TransportError.notAuthorized: "The host does not accept this iPad's key yet. Set it up with your password."
        case TransportError.failed(let message): message
        default: String(describing: error)
        }
    }
}
