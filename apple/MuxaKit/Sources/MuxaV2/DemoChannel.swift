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
    public var workspaces: [String]
    /// What the Reader shows in every Task's folder, by kind.
    public var files: [ReaderKind: [DemoFile]]

    public init(terminals: [Terminal], screens: [String: String], inbox: [InboxItem], routines: [Routine], workspaces: [String],
                files: [ReaderKind: [DemoFile]] = [:]) {
        self.terminals = terminals
        self.screens = screens
        self.inbox = inbox
        self.routines = routines
        self.workspaces = workspaces
        self.files = files
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
    private var nextItem = 100
    private var channels: [UInt32: String] = [:]
    private var offsets: [UInt32: UInt64] = [:]
    private var applied: [UInt32: UInt64] = [:]
    private var nextChannel: UInt32 = 1
    private var nextReader: UInt32 = 1
    private var watching = false

    init(content: DemoContent, out: AsyncThrowingStream<Data, any Error>.Continuation) {
        self.content = content
        self.out = out
        inbox = Inbox(revision: 1, items: content.inbox, done: [])
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
            startTask(id, title: m["title"] as? String ?? "Task", places: m["places"] as? [[String: Any]] ?? [])
        case "watch":
            watching = true
            reply(id, [:])
        case "inbox":
            reply(id, ["inbox": json(inbox)])
        case "parse-capture":
            reply(id, ["capture": json(capture(m["text"] as? String ?? "", today: m["today"] as? String ?? ""))])
        case "inbox-add":
            let new = InboxItem(
                id: "d\(nextItem)", kind: ItemKind(rawValue: m["kind"] as? String ?? "") ?? .todo,
                title: m["title"] as? String ?? "", note: m["note"] as? String,
                workspace: m["workspace"] as? String, due: m["due"] as? String, laterUntil: nil,
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
            let until = (m["until"] as? NSNumber)?.int64Value
            change(id) { inbox in
                guard let at = inbox.items.firstIndex(where: { $0.id == item }) else { return false }
                inbox.items[at].laterUntil = until
                return true
            }
        case "inbox-done":
            change(id) { inbox in
                guard let at = inbox.items.firstIndex(where: { $0.id == item }) else { return false }
                inbox.done.insert(inbox.items.remove(at: at), at: 0)
                return true
            }
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
                note: m["note"] as? String, workspace: nil, due: nil, laterUntil: nil,
                created: Int64(Date().timeIntervalSince1970), source: "task")
            nextItem += 1
            change(id) { $0.items.insert(review, at: 0) }
        case "routines":
            reply(id, ["routines": json(content.routines), "problems": []])
        case "run-routine":
            let name = m["name"] as? String
            guard let at = content.routines.firstIndex(where: { $0.name == name }) else {
                return send(["type": "error", "id": id, "code": "bad-request", "message": "no Routine \(name ?? "")"])
            }
            if content.routines[at].runningSince != nil {
                return send(["type": "error", "id": id, "code": "conflict", "message": "\(name ?? "") already runs"])
            }
            content.routines[at].runningSince = Int64(Date().timeIntervalSince1970)
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
        default:
            send(["type": "error", "id": id, "code": "bad-request", "message": "unknown request"])
        }
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

    /// The first branch is the working place; without one, a folder named after the title.
    private func startTask(_ id: UInt64, title: String, places: [[String: Any]]) {
        let firstBranch = places.first { $0["kind"] as? String == "branch" }
        let branch = firstBranch?["branch"] as? String
        let repo = firstBranch?["repo"] as? String
        let dir = branch.map { "\(repo ?? "/demo")-\($0)" } ?? "/demo/tasks/\(title)"
        if let running = content.terminals.first(where: { $0.live && $0.isTaskAgent && $0.taskDir == dir }) {
            return reply(id, ["terminal": json(running), "existing": true])
        }
        var labels = ["muxa.kind": "task", "muxa.role": "agent", "muxa.dir": dir]
        if let branch { labels["muxa.branch"] = branch }
        if let repo { labels["muxa.repo"] = repo }
        let t = Terminal(
            id: "t\(content.terminals.count + 1)", name: title, live: true,
            status: TerminalStatus(state: "working", app: "claude-code"),
            labels: labels, cols: 80, rows: 24)
        content.terminals.append(t)
        content.screens[t.id] = "Starting the agent for \(title)…\r\n"
        reply(id, ["terminal": json(t), "existing": false])
        if watching { send(["type": "changed", "terminal": json(t)]) }
    }

    /// Applies an Inbox change; `false` means the item was not found.
    private func change(_ id: UInt64, _ f: (inout Inbox) -> Bool) {
        guard f(&inbox) else {
            return send(["type": "error", "id": id, "code": "not-found", "message": "no such item"])
        }
        inbox.revision += 1
        reply(id, ["inbox": json(inbox)])
        if watching { send(["type": "inbox", "inbox": json(inbox)]) }
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

    private func json<T: Encodable>(_ value: T) -> Any {
        let e = JSONEncoder()
        e.keyEncodingStrategy = .convertToSnakeCase
        guard let data = try? e.encode(value) else { return NSNull() }
        return (try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])) ?? NSNull()
    }
}
