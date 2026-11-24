import Foundation
import MuxaTransport

/// Protocol v2 served in memory with fixed content, for Try the Demo, the UI
/// tests and client tests. It answers like the muxa server; a terminal echoes
/// what is typed. Capture reads only the kind, a known `#Workspace` and
/// `due today|tomorrow|YYYY-MM-DD`.
public final class DemoChannel: ByteChannel {
    public let bytes: AsyncThrowingStream<Data, any Error>
    private let server: DemoServer

    public init(content: DemoContent = .standard) {
        let (bytes, out) = AsyncThrowingStream<Data, any Error>.makeStream()
        self.bytes = bytes
        server = DemoServer(content: content, out: out)
    }

    public func send(_ data: Data) async throws { await server.receive(data) }
    public func close() async { await server.close() }
}

/// What the demo shows.
public struct DemoContent: Sendable {
    public var terminals: [Terminal]
    /// Each terminal's screen, by id.
    public var screens: [String: String]
    public var inbox: [InboxItem]
    public var routines: [Routine]
    /// The names Capture knows.
    public var workspaces: [String]
    /// What `workspaces` answers.
    public var taskWorkspaces: [Workspace]
    /// What the Reader shows in every Task's folder, by kind.
    public var files: [ReaderKind: [DemoFile]]
    /// Each Task agent's conversation, by terminal id.
    public var chats: [String: ChatUpdate]
    /// Review items whose Approve without force is refused with
    /// `would-lose-work`, and the message, by item id.
    public var losesWork: [String: String]
    /// What `settings` answers first; `settings-update` changes it in memory.
    public var settings: Settings

    public init(terminals: [Terminal], screens: [String: String], inbox: [InboxItem], routines: [Routine], workspaces: [String],
                taskWorkspaces: [Workspace] = [], files: [ReaderKind: [DemoFile]] = [:], chats: [String: ChatUpdate] = [:],
                losesWork: [String: String] = [:], settings: Settings = Settings(revision: "1")) {
        self.terminals = terminals
        self.screens = screens
        self.inbox = inbox
        self.routines = routines
        self.workspaces = workspaces
        self.taskWorkspaces = taskWorkspaces
        self.files = files
        self.chats = chats
        self.losesWork = losesWork
        self.settings = settings
    }
}

/// A Reader file of the demo: its list entry and its content.
public struct DemoFile: Sendable {
    public var entry: ReaderEntry
    public var content: ReaderContent

    public init(entry: ReaderEntry, content: ReaderContent) {
        self.entry = entry
        self.content = content
    }
}

actor DemoServer {
    private let out: AsyncThrowingStream<Data, any Error>.Continuation
    private var buf: [UInt8] = []
    private var content: DemoContent
    private var inbox: Inbox
    /// Uploads by folder and file name, so a second one with the same name gets a number.
    private var uploads: Set<String> = []
    private var nextItem = 100
    private var channels: [UInt32: String] = [:]
    private var offsets: [UInt32: UInt64] = [:]
    private var applied: [UInt32: UInt64] = [:]
    private var nextChannel: UInt32 = 1
    private var nextReader: UInt32 = 1
    private var loops: [String: LoopInfo] = [:]
    private var nextChat: UInt32 = 1
    private var nextTerminal = 1
    private var watching = false

    init(content: DemoContent, out: AsyncThrowingStream<Data, any Error>.Continuation) {
        self.content = content
        self.out = out
        inbox = Inbox(revision: 1, items: content.inbox, done: [])
        // pr-watch has used two of its three items today, as a host counts them.
        let today = Date.now.formatted(.iso8601.year().month().day())
        inbox.bots = [BotCount(workspace: "app", bot: "pr-watch", done: 4, deleted: 1, day: today, added: 2)]
    }

    func close() { out.finish() }

    func receive(_ data: Data) {
        buf += data
        do {
            while let frame = try Frame.decode(&buf) { handle(frame) }
        } catch {
            out.finish(throwing: error)
        }
    }

    private func handle(_ f: Frame) {
        switch (f.channel, f.kind) {
        case (0, .data):
            guard let msg = (try? JSONSerialization.jsonObject(with: Data(f.payload))) as? [String: Any] else { return }
            control(msg)
        case (let n, .data):
            guard let (seq, bytes) = f.splitU64(), let id = channels[n] else { return }
            if seq > applied[n, default: 0] {
                applied[n] = seq
                // The demo's program echoes; Return starts a new line.
                let echo = bytes.flatMap { $0 == 13 ? [13, 10] : [$0] }
                content.screens[id, default: ""] += String(decoding: echo, as: UTF8.self)
                write(Frame.withU64(n, offsets[n, default: 0], echo))
                offsets[n, default: 0] += UInt64(echo.count)
            }
            write(Frame(channel: n, kind: .ack, payload: le(seq)))
        case (let n, .close):
            channels[n] = nil
        default:
            break
        }
    }

    private func control(_ m: [String: Any]) {
        let id = (m["id"] as? NSNumber)?.uint64Value ?? 0
        let item = m["item"] as? String ?? ""
        switch m["type"] as? String {
        case "hello":
            guard (m["version"] as? NSNumber)?.intValue == 2 else {
                return send(["type": "error", "code": "version", "message": "this server speaks protocol 2"])
            }
            send(["type": "welcome", "version": 2, "server": "muxa demo", "backend": "demo", "token": "demo", "resumed": []])
        case "terminals":
            reply(id, ["terminals": json(content.terminals.filter(\.live))])
        case "attach":
            attach(id, m["terminal"] as? String ?? "", cols: (m["cols"] as? NSNumber)?.uint16Value ?? 80)
        case "detach":
            let n = (m["channel"] as? NSNumber)?.uint32Value ?? 0
            channels[n] = nil
            write(Frame(channel: n, kind: .close, payload: []))
            reply(id, [:])
        case "start-task":
            startTask(id, m)
        case "watch":
            watching = true
            reply(id, [:])
        case "inbox":
            reply(id, ["inbox": inbox.wireObject])
        case "parse-capture":
            reply(id, ["capture": json(capture(m["text"] as? String ?? "", today: m["today"] as? String ?? ""))])
        case "inbox-add":
            let new = InboxItem(
                id: "d\(nextItem)", kind: ItemKind(rawValue: m["kind"] as? String ?? "") ?? .todo,
                title: m["title"] as? String ?? "", note: m["note"] as? String,
                workspace: m["workspace"] as? String, due: m["due"] as? String,
                created: Int64(Date().timeIntervalSince1970), source: "capture")
            nextItem += 1
            change(id) { $0.items.insert(new, at: 0) }
        case "inbox-move":
            let index = (m["index"] as? NSNumber)?.intValue ?? 0
            change(id) { inbox in
                guard let at = inbox.items.firstIndex(where: { $0.id == item }) else { return false }
                let moved = inbox.items.remove(at: at)
                inbox.items.insert(moved, at: min(index, inbox.items.count))
                return true
            }
        case "inbox-later":
            change(id) { inbox in
                guard let at = inbox.items.firstIndex(where: { $0.id == item }) else { return false }
                inbox.items.append(inbox.items.remove(at: at))
                return true
            }
        case "inbox-done":
            resolve(id, item)
        case "inbox-reopen":
            change(id) { inbox in
                guard let at = inbox.done.firstIndex(where: { $0.id == item }) else { return false }
                inbox.items.insert(inbox.done.remove(at: at), at: 0)
                return true
            }
        case "inbox-delete":
            change(id) { inbox in
                let before = inbox.items.count + inbox.done.count
                inbox.items.removeAll { $0.id == item }
                inbox.done.removeAll { $0.id == item }
                return inbox.items.count + inbox.done.count < before
            }
        case "task-review":
            let t = content.terminals.first { $0.id == m["terminal"] as? String }
            let review = InboxItem(
                id: "d\(nextItem)", kind: .review, title: "Review \(t?.branch ?? "the Task")",
                note: m["note"] as? String, workspace: nil, due: nil,
                created: Int64(Date().timeIntervalSince1970), source: "task", terminal: t?.id)
            nextItem += 1
            change(id) { $0.items.insert(review, at: 0) }
        case "inbox-approve":
            guard let review = reviewItem(id, item), let terminal = review.terminal else { return }
            if m["worktree"] as? String != WorktreeChoice.force.rawValue, let why = content.losesWork[item] {
                return send(["type": "error", "id": id, "code": "would-lose-work", "message": why])
            }
            closeWithShells(terminal)
            resolve(id, item)
        case "task-shell":
            taskShell(id, m["terminal"] as? String)
        case "close-terminal":
            guard let t = content.terminals.first(where: { $0.id == m["terminal"] as? String }), t.isTaskShell else {
                return send(["type": "error", "id": id, "code": "bad-request", "message": "only a Task's shell closes this way"])
            }
            close(t.id)
            reply(id, [:])
        case "close-task":
            closeTask(id, m)
        case "upload":
            upload(id, m)
        case "inbox-request-changes":
            guard let review = reviewItem(id, item), let terminal = review.terminal else { return }
            let comment = m["comment"] as? String ?? ""
            guard !comment.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                return send(["type": "error", "id": id, "code": "bad-request", "message": "the comment is empty"])
            }
            type(terminal, Array((comment + "\r\n").utf8))
            resolve(id, item)
        case "inbox-retry":
            retry(id, item)
        case "inbox-apply":
            applyProposal(id, item)
        case "inbox-suggestion":
            answerSuggestion(id, item, accept: m["accept"] as? Bool ?? false)
        case "workspaces":
            reply(id, ["workspaces": json(content.taskWorkspaces.map(withTasks))])
        case "settings":
            reply(id, ["settings": plain(content.settings)])
        case "settings-update":
            updateSettings(id, m)
        case "routines":
            reply(id, ["routines": json(content.routines), "problems": [], "off": json(routinesOff)])
        case "run-routine":
            guard runRoutine(id, workspace: m["workspace"] as? String ?? "", name: m["name"] as? String ?? "") else { return }
            reply(id, [:])
        case "reader-list":
            guard let kind = readerKind(id, m) else { return }
            reply(id, ["list": json(ReaderList(root: "app", entries: content.files[kind, default: []].map(\.entry)))])
        case "reader-open":
            guard let kind = readerKind(id, m) else { return }
            let path = m["path"] as? String
            guard let file = content.files[kind, default: []].first(where: { $0.entry.path == path }) else {
                return send(["type": "error", "id": id, "code": "bad-request", "message": "\(path ?? "") is not a listed file"])
            }
            let n = nextReader
            nextReader += 1
            reply(id, ["reader": n, "content": json(file.content)])
        case "reader-close":
            reply(id, [:])
        case "chat-open":
            let terminal = m["terminal"] as? String ?? ""
            guard content.terminals.contains(where: { $0.id == terminal && $0.isTaskAgent }) else {
                return send(["type": "error", "id": id, "code": "bad-request", "message": "\(terminal) is not a Task's agent"])
            }
            let update = content.chats[terminal] ?? ChatUpdate(reset: true, entries: [])
            let n = nextChat
            nextChat += 1
            reply(id, ["chat": n, "update": json(update)])
        case "chat-close":
            reply(id, [:])
        case "loops":
            reply(id, ["loops": json(loops.values.sorted { $0.terminal < $1.terminal })])
        case "loop-start":
            let terminal = m["terminal"] as? String ?? ""
            guard content.terminals.contains(where: { $0.id == terminal && $0.isTaskAgent }) else {
                return send(["type": "error", "id": id, "code": "bad-request", "message": "\(terminal) is not a Task's agent"])
            }
            let previous = (m["reset"] as? Bool) == true ? nil : loops[terminal]
            let lp = newLoop(terminal, m["loop"] as? [String: Any] ?? [:], previous: previous)
            setLoop(lp)
            reply(id, ["loop": json(lp)])
        case "loop-stop":
            let terminal = m["terminal"] as? String ?? ""
            guard var lp = loops[terminal] else {
                return send(["type": "error", "id": id, "code": "bad-request", "message": "\(terminal) has no Task Loop"])
            }
            if lp.isActive {
                lp.state = "stopped"
                lp.reason = "user"
                lp.since = Int64(Date().timeIntervalSince1970)
            }
            setLoop(lp)
            reply(id, ["loop": json(lp)])
        default:
            send(["type": "error", "id": id, "code": "bad-request", "message": "unknown request"])
        }
    }

    /// Settings' Workspaces whose Routines are turned off, as the daemon reports them.
    private var routinesOff: [RoutinesOff] {
        content.settings.workspaces.compactMap { w in
            w.routinesEnabled || w.routines.isEmpty ? nil : RoutinesOff(workspace: w.name, count: UInt32(w.routines.count))
        }
    }

    private func updateSettings(_ id: UInt64, _ m: [String: Any]) {
        guard m["revision"] as? String == content.settings.revision else {
            return send(["type": "error", "id": id, "code": "conflict", "message": "the settings changed; read them again"])
        }
        do {
            let ops = m["ops"] as? [Any] ?? []
            content.settings = try content.settings.demoApplying(ops)
            reply(id, ["settings": plain(content.settings)])
            renameBotItems(ops)
        } catch {
            send(["type": "error", "id": id, "code": "bad-request", "message": (error as? DemoSettingsRefusal)?.message ?? "the ops cannot apply"])
        }
    }

    /// After `bot-rename`, the server moves the Bot's items and counts to the new name (ADR 0089).
    private func renameBotItems(_ ops: [Any]) {
        let renames = ops.compactMap { $0 as? [String: Any] }.filter { $0["op"] as? String == "bot-rename" }
        guard !renames.isEmpty else { return }
        for r in renames {
            guard let w = r["workspace"] as? String, let from = r["from"] as? String, let to = r["to"] as? String else { continue }
            func move(_ items: inout [InboxItem]) {
                for i in items.indices where items[i].bot == from && items[i].workspace == w { items[i].bot = to }
            }
            move(&inbox.items)
            move(&inbox.done)
            if var counts = inbox.bots {
                for i in counts.indices where counts[i].bot == from && counts[i].workspace == w { counts[i].bot = to }
                inbox.bots = counts
            }
        }
        inbox.revision += 1
        if watching { send(["type": "inbox", "inbox": inbox.wireObject]) }
    }

    /// Applies a Bot's proposed settings as `settings-update` would, then
    /// marks the item Done (ADR 0083).
    private func applyProposal(_ id: UInt64, _ item: String) {
        guard let found = openItem(id, item) else { return }
        guard let proposal = found.settingsProposal else {
            return send(["type": "error", "id": id, "code": "bad-request", "message": "this item proposes no settings"])
        }
        guard proposal.revision == content.settings.revision else {
            return send(["type": "error", "id": id, "code": "conflict",
                         "message": "the settings changed since the Bot proposed this; ask it again"])
        }
        do {
            let ops = try proposal.ops.map { try $0.jsonObject() }
            content.settings = try content.settings.demoApplying(ops)
        } catch {
            let message = (error as? DemoSettingsRefusal)?.message ?? "the ops cannot apply"
            return send(["type": "error", "id": id, "code": "bad-request", "message": message])
        }
        resolve(id, item)
    }

    /// Accept applies the Capture assistant's kind, Workspace, due and note,
    /// or splits the item; either answer clears the suggestion (ADR 0084).
    private func answerSuggestion(_ id: UInt64, _ item: String, accept: Bool) {
        guard let found = openItem(id, item) else { return }
        guard let s = found.suggestion else {
            return send(["type": "error", "id": id, "code": "bad-request", "message": "this item has no suggestion"])
        }
        let now = Int64(Date().timeIntervalSince1970)
        let parts = accept ? s.split.map { title -> InboxItem in
            defer { nextItem += 1 }
            return InboxItem(id: "d\(nextItem)", kind: s.kind ?? found.kind, title: title,
                             workspace: s.workspace ?? found.workspace, due: s.due ?? found.due,
                             created: now, source: "capture")
        } : []
        change(id) { inbox in
            guard let at = inbox.items.firstIndex(where: { $0.id == item }) else { return false }
            inbox.items[at].suggestion = nil
            guard accept else { return true }
            if !parts.isEmpty {
                inbox.done.insert(inbox.items.remove(at: at), at: 0)
                inbox.items.insert(contentsOf: parts, at: at)
                return true
            }
            if let kind = s.kind { inbox.items[at].kind = kind }
            if let workspace = s.workspace { inbox.items[at].workspace = workspace }
            if let due = s.due { inbox.items[at].due = due }
            if let note = s.note {
                inbox.items[at].note = [inbox.items[at].note, note].compactMap { $0 }.joined(separator: "\n\n")
            }
            return true
        }
    }

    /// Starts a Routine; `false` after an error was sent.
    private func runRoutine(_ id: UInt64, workspace: String, name: String) -> Bool {
        guard let at = content.routines.firstIndex(where: { $0.workspace == workspace && $0.name == name }) else {
            send(["type": "error", "id": id, "code": "bad-request", "message": "no Routine \(name)"])
            return false
        }
        if content.routines[at].runningSince != nil {
            send(["type": "error", "id": id, "code": "conflict", "message": "\(name) already runs"])
            return false
        }
        content.routines[at].runningSince = Int64(Date().timeIntervalSince1970)
        return true
    }

    /// An open item, or nil after an error was sent.
    private func openItem(_ id: UInt64, _ item: String) -> InboxItem? {
        guard let found = inbox.items.first(where: { $0.id == item }) else {
            send(["type": "error", "id": id, "code": "not-found", "message": "no such item"])
            return nil
        }
        return found
    }

    /// An open Review item of a Task, or nil after an error was sent.
    private func reviewItem(_ id: UInt64, _ item: String) -> InboxItem? {
        guard let found = openItem(id, item) else { return nil }
        guard found.kind == .review, let terminal = found.terminal,
              content.terminals.contains(where: { $0.id == terminal }) else {
            send(["type": "error", "id": id, "code": "bad-request", "message": "this item is not the Review of a running Task"])
            return nil
        }
        return found
    }

    /// Runs again the Routine or the Task agent that made a Failed item.
    private func retry(_ id: UInt64, _ item: String) {
        guard let failed = openItem(id, item) else { return }
        func refuse() {
            send(["type": "error", "id": id, "code": "bad-request", "message": "only a Failed item of a Routine or a Task can run again"])
        }
        guard failed.kind == .failed else { return refuse() }
        if let routine = failed.routine {
            guard runRoutine(id, workspace: routine.workspace, name: routine.name) else { return }
        } else if let terminal = failed.terminal, let at = content.terminals.firstIndex(where: { $0.id == terminal }) {
            content.terminals[at].live = true
            content.terminals[at].status = TerminalStatus(state: "working", app: content.terminals[at].status?.app)
            if watching { send(["type": "changed", "terminal": json(content.terminals[at])]) }
        } else {
            return refuse()
        }
        resolve(id, item)
    }

    /// Moves an open item to done.
    private func resolve(_ id: UInt64, _ item: String) {
        change(id) { inbox in
            guard let at = inbox.items.firstIndex(where: { $0.id == item }) else { return false }
            inbox.done.insert(inbox.items.remove(at: at), at: 0)
            return true
        }
    }

    /// Closes a Task's terminal and its channels.
    private func close(_ terminal: String) {
        content.terminals.removeAll { $0.id == terminal }
        content.screens[terminal] = nil
        for (n, t) in channels where t == terminal {
            channels[n] = nil
            write(Frame(channel: n, kind: .close, payload: []))
        }
        if watching { send(["type": "removed", "terminal": terminal]) }
    }

    /// A shell beside a live Task agent, in its folder; three at most (ADR 0079).
    private func taskShell(_ id: UInt64, _ terminal: String?) {
        guard let agent = content.terminals.first(where: { $0.id == terminal && $0.live && $0.isTaskAgent }),
              let dir = agent.taskDir else {
            return send(["type": "error", "id": id, "code": "bad-request", "message": "a shell starts beside a live Task's agent"])
        }
        let shells = content.terminals.filter { $0.live && $0.isTaskShell && $0.taskDir == dir }
        guard shells.count < 3 else {
            return send(["type": "error", "id": id, "code": "conflict", "message": "a Task has at most 3 shells; close one first"])
        }
        var labels = ["muxa.kind": "task", "muxa.role": "shell", "muxa.dir": dir]
        labels["muxa.workspace"] = agent.labels["muxa.workspace"]
        labels["muxa.title"] = agent.labels["muxa.title"]
        let t = Terminal(id: newTerminalID(), name: agent.name.map { "\($0)-sh" }, live: true, status: nil,
                         labels: labels, cols: 80, rows: 24)
        content.terminals.append(t)
        content.screens[t.id] = "\(dir) $ "
        reply(id, ["terminal": json(t), "existing": false])
        if watching { send(["type": "changed", "terminal": json(t)]) }
    }

    /// Keeps an upload's name, as the host would write it (ADR 0081); nothing is saved.
    private func upload(_ id: UInt64, _ m: [String: Any]) {
        func refuse(_ message: String) { send(["type": "error", "id": id, "code": "bad-request", "message": message]) }
        guard let t = content.terminals.first(where: { $0.id == m["terminal"] as? String }),
              t.labels["muxa.kind"] == "task" else { return refuse("files go to a Task") }
        let name = (m["name"] as? String ?? "").trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, !name.hasPrefix("."), !name.contains("/") else { return refuse("send the file name only") }
        guard let data = Data(base64Encoded: m["data"] as? String ?? "") else { return refuse("data is not base64") }
        guard data.count <= V2Client.maxUpload else { return refuse("a file is at most 10 MB") }
        let dir = t.taskDir ?? ""
        var file = name
        var n = 1
        while uploads.contains("\(dir)/\(file)") {
            n += 1
            let ext = (name as NSString).pathExtension
            let stem = (name as NSString).deletingPathExtension
            file = ext.isEmpty ? "\(name) \(n)" : "\(stem) \(n).\(ext)"
        }
        uploads.insert("\(dir)/\(file)")
        reply(id, ["path": ".muxa/uploads/\(file)"])
    }

    /// Closes a Task without its item, as Approve does, with its shells and open items.
    private func closeTask(_ id: UInt64, _ m: [String: Any]) {
        guard let agent = content.terminals.first(where: { $0.id == m["terminal"] as? String && $0.isTaskAgent }) else {
            return send(["type": "error", "id": id, "code": "bad-request", "message": "only a Task closes this way"])
        }
        let open = inbox.items.filter { $0.terminal == agent.id }.map(\.id)
        if m["worktree"] as? String != WorktreeChoice.force.rawValue,
           let why = open.lazy.compactMap({ self.content.losesWork[$0] }).first {
            return send(["type": "error", "id": id, "code": "would-lose-work", "message": why])
        }
        closeWithShells(agent.id)
        change(id) { inbox in
            for item in open {
                guard let at = inbox.items.firstIndex(where: { $0.id == item }) else { continue }
                inbox.done.insert(inbox.items.remove(at: at), at: 0)
            }
            return true
        }
    }

    /// Closes a Task's agent, then the shells in its folder, as the server does.
    private func closeWithShells(_ agent: String) {
        let dir = content.terminals.first { $0.id == agent }?.taskDir
        close(agent)
        for shell in content.terminals where shell.isTaskShell && dir != nil && shell.taskDir == dir { close(shell.id) }
    }

    /// Input as if the user typed it: the screen and every attached channel show it.
    private func type(_ terminal: String, _ bytes: [UInt8]) {
        content.screens[terminal, default: ""] += String(decoding: bytes, as: UTF8.self)
        for (n, t) in channels where t == terminal {
            write(Frame.withU64(n, offsets[n, default: 0], bytes))
            offsets[n, default: 0] += UInt64(bytes.count)
        }
    }

    /// An armed loop; `previous` keeps its failed checks.
    private func newLoop(_ terminal: String, _ spec: [String: Any], previous: LoopInfo?) -> LoopInfo {
        let now = Int64(Date().timeIntervalSince1970)
        return LoopInfo(
            terminal: terminal, check: spec["check"] as? String ?? "make test",
            maxAttempts: (spec["max_attempts"] as? NSNumber)?.uint32Value ?? 3,
            timeout: (spec["timeout"] as? NSNumber)?.uint64Value ?? 600, state: "armed",
            attempts: previous?.attempts ?? 0, armedAt: now, since: now)
    }

    private func setLoop(_ lp: LoopInfo) {
        loops[lp.terminal] = lp
        if watching { send(["type": "loops", "loops": json(loops.values.sorted { $0.terminal < $1.terminal })]) }
    }

    /// The kind of a Reader request on a terminal with a folder, or nil after an error.
    private func readerKind(_ id: UInt64, _ m: [String: Any]) -> ReaderKind? {
        let t = content.terminals.first { $0.id == m["terminal"] as? String }
        guard t?.taskDir != nil else {
            send(["type": "error", "id": id, "code": "bad-request",
                  "message": "this terminal has no folder: only a Task's agent or a Routine run has a Reader"])
            return nil
        }
        return ReaderKind(rawValue: m["kind"] as? String ?? "")
    }

    private func attach(_ id: UInt64, _ terminal: String, cols: UInt16) {
        guard content.terminals.contains(where: { $0.id == terminal }) else {
            return send(["type": "error", "id": id, "code": "not-found", "message": "no terminal \(terminal)"])
        }
        let n = nextChannel
        nextChannel += 1
        channels[n] = terminal
        reply(id, ["channel": n])
        send(["type": "paint", "channel": n])
        let screen = Array(content.screens[terminal, default: ""].utf8)
        write(Frame.withU64(n, 0, screen))
        offsets[n] = UInt64(screen.count)
    }

    /// An existing place or the first branch is the working place; without one, a folder
    /// named after the title. A `loop` arms the new agent's Task Loop, as the server does.
    private func startTask(_ id: UInt64, _ m: [String: Any]) {
        func refuse(_ message: String) { send(["type": "error", "id": id, "code": "bad-request", "message": message]) }
        let title = (m["title"] as? String ?? "").trimmingCharacters(in: .whitespaces)
        guard !title.isEmpty else { return refuse("a Task needs a title") }
        let name = m["workspace"] as? String ?? ""
        guard let workspace = content.taskWorkspaces.first(where: { $0.name == name }) else {
            return refuse("no Workspace \(name)")
        }
        let loopSpec = m["loop"] as? [String: Any]
        if let loopSpec, let refusal = loopRefusal(loopSpec, workspace) { return refuse(refusal) }
        let bot = m["bot"] as? String
        if let bot, !BotSpec.isValidName(bot) { return refuse("a Bot's name is 1 to 64 lowercase letters, digits, - or _") }
        let places = m["places"] as? [[String: Any]] ?? []
        let checkout = places.first { $0["kind"] as? String == "checkout" }?["repo"] as? String
        let existing = (places.first { $0["kind"] as? String == "existing" }?["path"] as? String)
            ?? checkout.map { "\(workspace.root)/\($0)" }
        if existing != nil, places.contains(where: { ["folder", "branch"].contains($0["kind"] as? String) }) {
            return refuse("an existing place or a checkout is the working place: it comes without a folder or a branch")
        }
        let firstBranch = places.first { $0["kind"] as? String == "branch" }
        let branch = firstBranch?["branch"] as? String
        let repo = firstBranch?["repo"] as? String ?? checkout
        let dir = existing ?? branch.map { "\(repo ?? "/demo")-\($0)" } ?? "/demo/tasks/\(title)"
        if let running = content.terminals.first(where: { $0.live && $0.isTaskAgent && $0.taskDir == dir }) {
            return reply(id, ["terminal": json(running), "existing": true])
        }
        var labels = ["muxa.kind": "task", "muxa.role": "agent", "muxa.dir": dir, "muxa.workspace": workspace.name,
                      "muxa.title": title]
        if let branch { labels["muxa.branch"] = branch }
        if let repo { labels["muxa.repo"] = repo }
        if let checkout { labels["muxa.checkout"] = checkout }
        if let bot { labels["muxa.bot"] = bot }
        // Every agent gets all the Workspace's connections and servers (ADR 0075);
        // `connections` and `mcp` in the request are ignored.
        let connections = workspace.connections.map(\.name)
        if !connections.isEmpty { labels["muxa.connections"] = jsonText(connections) }
        let servers = workspace.mcp.map(\.name)
        if !servers.isEmpty { labels["muxa.mcp"] = jsonText(servers) }
        let t = Terminal(
            id: newTerminalID(), name: title, live: true,
            status: TerminalStatus(state: "working", app: "claude-code"),
            labels: labels, cols: 80, rows: 24)
        content.terminals.append(t)
        content.screens[t.id] = "Starting the agent for \(title)…\r\n"
        if let loopSpec { setLoop(newLoop(t.id, loopSpec, previous: nil)) }
        reply(id, ["terminal": json(t), "existing": false])
        if watching { send(["type": "changed", "terminal": json(t)]) }
    }

    /// `ws` with its live Task agents under every connection and MCP server,
    /// as the server answers `workspaces` (ADR 0075).
    private func withTasks(_ ws: Workspace) -> Workspace {
        let agents = content.terminals
            .filter { $0.live && $0.isTaskAgent && $0.labels["muxa.workspace"] == ws.name }
            .map(\.id)
        var out = ws
        for i in out.connections.indices { out.connections[i].usedBy.tasks = agents }
        for i in out.mcp.indices { out.mcp[i].usedBy.tasks = agents }
        return out
    }

    private func jsonText(_ names: [String]) -> String {
        (try? JSONEncoder().encode(names)).map { String(decoding: $0, as: UTF8.self) } ?? "[]"
    }

    /// Why the Workspace refuses this loop, or nil.
    private func loopRefusal(_ spec: [String: Any], _ workspace: Workspace) -> String? {
        guard let checks = workspace.loop?.checks else { return "\(workspace.name) runs no Task Loops" }
        let check = spec["check"] as? String
        switch checks {
        case .any:
            return check?.isEmpty == false ? nil : "name the check: \(workspace.name) allows any"
        case .list(let list):
            if let check { return list.contains(check) ? nil : "\(workspace.name) does not allow the check \(check)" }
            return list.count == 1 ? nil : "name the check: \(workspace.name) allows several"
        }
    }

    /// A terminal id not used yet, also after a Task closed.
    private func newTerminalID() -> String {
        while content.terminals.contains(where: { $0.id == "t\(nextTerminal)" }) { nextTerminal += 1 }
        defer { nextTerminal += 1 }
        return "t\(nextTerminal)"
    }

    /// Applies an Inbox change; `false` means the item was not found.
    private func change(_ id: UInt64, _ f: (inout Inbox) -> Bool) {
        guard f(&inbox) else {
            return send(["type": "error", "id": id, "code": "not-found", "message": "no such item"])
        }
        inbox.revision += 1
        reply(id, ["inbox": inbox.wireObject])
        if watching { send(["type": "inbox", "inbox": inbox.wireObject]) }
    }

    private func change(_ id: UInt64, _ f: (inout Inbox) -> Void) {
        change(id) { (inbox: inout Inbox) -> Bool in
            f(&inbox)
            return true
        }
    }

    private func capture(_ text: String, today: String) -> Capture {
        var words = text.split(separator: " ").map(String.init)
        var kind = ItemKind.todo
        if let first = words.first?.lowercased(), let k = ["reply": ItemKind.reply, "read": .read, "decide": .decide][first] {
            kind = k
            words.removeFirst()
        }
        var due: String?
        if let at = words.firstIndex(where: { $0.lowercased() == "due" }), at + 1 < words.count {
            due = day(words[at + 1].lowercased(), today: today)
            if due != nil { words.removeSubrange(at...at + 1) }
        }
        var workspace: String?
        words.removeAll { w in
            guard workspace == nil, w.hasPrefix("#"),
                  let ws = content.workspaces.first(where: { $0.lowercased() == w.dropFirst().lowercased() })
            else { return false }
            workspace = ws
            return true
        }
        return Capture(kind: kind, title: words.joined(separator: " "), workspace: workspace, due: due)
    }

    private func day(_ word: String, today: String) -> String? {
        let f = DateFormatter()
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = TimeZone(identifier: "UTC")
        f.dateFormat = "yyyy-MM-dd"
        switch word {
        case "today": return today
        case "tomorrow":
            return f.date(from: today).flatMap { Calendar(identifier: .gregorian).date(byAdding: .day, value: 1, to: $0) }.map(f.string)
        default: return f.date(from: word) != nil ? word : nil
        }
    }

    private func reply(_ id: UInt64, _ fields: [String: Any]) {
        send(fields.merging(["type": "reply", "id": id]) { a, _ in a })
    }

    private func send(_ msg: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: msg) else { return }
        write(Frame.control(data))
    }

    private func write(_ frame: Frame) {
        out.yield(Data(frame.encode()))
    }

    /// The settings types spell out their wire keys.
    private func plain<T: Encodable>(_ value: T) -> Any {
        guard let data = try? Coding.plainEncoder.encode(value) else { return NSNull() }
        return (try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])) ?? NSNull()
    }

    private func json<T: Encodable>(_ value: T) -> Any {
        let e = JSONEncoder()
        e.keyEncodingStrategy = .convertToSnakeCase
        guard let data = try? e.encode(value) else { return NSNull() }
        return (try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])) ?? NSNull()
    }
}
