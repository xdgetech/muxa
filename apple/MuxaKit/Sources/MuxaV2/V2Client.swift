import Foundation
import MuxaTransport

/// A protocol v2 connection (`protocol/PROTOCOL-v2.md`). It keeps what a
/// resume needs: the token, and per channel the next output offset and the
/// input not yet acknowledged. After `.lost`, open a new client with `state`.
public actor V2Client {
    public struct State: Sendable {
        public var token: String?
        var channels: [UInt32: Channel] = [:]
        public init() {}
        /// The channels a resume asks for.
        public var openChannels: [UInt32] { channels.keys.sorted() }
    }

    struct Channel: Sendable {
        var terminal: String
        var cols: UInt16
        var rows: UInt16
        /// The next output offset expected.
        var offset: UInt64 = 0
        var lastSeq: UInt64 = 0
        var acked: UInt64 = 0
        var pending: [(seq: UInt64, bytes: [UInt8])] = []
    }

    public nonisolated let events: AsyncStream<V2Event>
    private let eventsOut: AsyncStream<V2Event>.Continuation
    private let channel: any ByteChannel
    /// Frames to write, in order; one writer task drains it.
    private let outbox: AsyncStream<[UInt8]>.Continuation
    private let frames: AsyncStream<[UInt8]>
    private var state: State
    private var nextID: UInt64 = 0
    private var waiting: [UInt64: (CheckedContinuation<Data, any Error>, attach: Channel?)] = [:]
    private var hello: CheckedContinuation<Data, any Error>?
    private var tasks: [Task<Void, Never>] = []
    private var gone: String?
    /// The channels the server resumed, from `welcome`.
    public private(set) var resumed: [UInt32] = []
    public private(set) var backend = ""

    /// Says hello, with a resume when `state` has a token.
    public static func open(_ channel: any ByteChannel, name: String, state: State = State()) async throws -> V2Client {
        let client = V2Client(channel: channel, state: state)
        try await client.start(name: name)
        return client
    }

    private init(channel: any ByteChannel, state: State) {
        self.channel = channel
        self.state = state
        (events, eventsOut) = AsyncStream.makeStream(bufferingPolicy: .unbounded)
        (frames, outbox) = AsyncStream<[UInt8]>.makeStream(bufferingPolicy: .unbounded)
    }

    private func start(name: String) async throws {
        tasks.append(Task { [channel, frames] in
            do {
                for await bytes in frames { try await channel.send(Data(bytes)) }
            } catch {
                self.lost("cannot write: \(error)")
            }
        })
        let bytes = channel.bytes
        tasks.append(Task {
            var buf: [UInt8] = []
            do {
                for try await chunk in bytes {
                    buf += chunk
                    while let frame = try Frame.decode(&buf) { self.handle(frame) }
                }
                self.lost("the connection closed")
            } catch {
                self.lost("\(error)")
            }
        })
        var resume: Any = NSNull()
        if let token = state.token {
            resume = [
                "token": token,
                "channels": state.channels.map { n, ch in
                    ["channel": n, "terminal": ch.terminal, "offset": ch.offset,
                     "input_seq": ch.acked, "cols": ch.cols, "rows": ch.rows]
                },
            ]
        }
        let msg: [String: Any] = ["type": "hello", "version": 2, "client": name, "resume": resume]
        send(Frame.control(try JSONSerialization.data(withJSONObject: msg)))
        _ = try await withCheckedThrowingContinuation { hello = $0 }
    }

    /// What a new client needs to resume this one.
    public var resumeState: State { state }

    public func close() async {
        tasks.forEach { $0.cancel() }
        outbox.finish()
        eventsOut.finish()
        await channel.close()
    }

    // MARK: Requests

    func request(_ r: V2Request, attach: Channel? = nil) async throws -> Data {
        if let gone { throw V2Error.lost(gone) }
        nextID += 1
        let id = nextID
        let body = try r.json(id: id)
        return try await withCheckedThrowingContinuation { cont in
            waiting[id] = (cont, attach)
            send(Frame.control(body))
        }
    }

    public func terminals() async throws -> [Terminal] {
        try field("terminals", of: await request(.terminals))
    }

    /// Attaches and returns the channel. A `paint` and the screen follow.
    public func attach(_ terminal: String, cols: UInt16, rows: UInt16, history: UInt32 = 0) async throws -> UInt32 {
        let ch = Channel(terminal: terminal, cols: cols, rows: rows)
        return try field("channel", of: await request(.attach(terminal: terminal, cols: cols, rows: rows, history: history), attach: ch))
    }

    public func detach(_ channel: UInt32) async throws {
        _ = try await request(.detach(channel: channel))
        state.channels[channel] = nil
    }

    /// The Task's agent terminal, and whether it was already running.
    public func startTask(
        workspace: String, title: String, goal: String? = nil, places: [PlaceSpec] = [], connections: [String] = [],
        mcp: [String] = [], plan: Bool = false, loop: LoopSpec? = nil, bot: String? = nil
    ) async throws -> (Terminal, existing: Bool) {
        let reply = try await request(
            .startTask(
                workspace: workspace, title: title, goal: goal, places: places, connections: connections, mcp: mcp,
                plan: plan, loop: loop, bot: bot))
        return (try field("terminal", of: reply), try field("existing", of: reply))
    }

    /// A new shell beside the Task's agent `terminal`; `refused(code: "conflict")`
    /// when the Task has its 3 shells.
    public func taskShell(_ terminal: String) async throws -> Terminal {
        try field("terminal", of: await request(.taskShell(terminal: terminal)))
    }

    /// The largest file `upload` takes, as the server checks it.
    public static let maxUpload = 10 * 1024 * 1024

    /// Copies a file into the Task's working place; the path there, relative to it.
    public func upload(_ data: Data, name: String, to terminal: String) async throws -> String {
        try field("path", of: await request(.upload(terminal: terminal, name: name, data: data)))
    }

    /// Ends a Task's shell and removes its terminal.
    public func closeTerminal(_ terminal: String) async throws {
        _ = try await request(.closeTerminal(terminal: terminal))
    }

    /// Asks for `changed`, `removed`, `resync` and `inbox` events.
    public func watch() async throws { _ = try await request(.watch) }

    public func parseCapture(_ text: String, today: String) async throws -> Capture {
        try field("capture", of: await request(.parseCapture(text: text, today: today)))
    }

    /// Any Inbox request; the reply is the whole Inbox. `inboxApprove` and
    /// `closeTask` are `refused(code: "would-lose-work")` while the Task's worktree holds work.
    public func inbox(_ r: V2Request = .inbox) async throws -> Inbox {
        try Inbox.field(of: await request(r))
    }

    /// The host's Workspaces, for starting a Task.
    public func workspaces() async throws -> [Workspace] {
        try field("workspaces", of: await request(.workspaces))
    }

    /// The host's Task places, other worktrees and build folders (ADR 0102).
    public func disk() async throws -> [DiskRow] {
        try field("rows", of: await request(.disk))
    }

    /// Removes rows of `disk`; a dry run unless `dryRun` is false.
    public func diskClean(paths: [String] = [], force: Bool = false, dryRun: Bool = true) async throws -> DiskCleaned {
        try Coding.decoder.decode(DiskCleaned.self, from: await request(.diskClean(paths: paths, force: force, dryRun: dryRun)))
    }

    /// The host's resources now (ADR 0105).
    public func host() async throws -> HostReport {
        try field("host", of: await request(.host))
    }

    /// Asks for `host` events: one at once, then every 10 s.
    public func hostWatch() async throws { _ = try await request(.hostWatch) }

    public func hostUnwatch() async throws { _ = try await request(.hostUnwatch) }

    /// The host's editable settings.
    public func settings() async throws -> Settings {
        try field("settings", of: await request(.settings), decoder: Coding.plainDecoder)
    }

    /// Applies `ops` in order, all or none, and returns the settings after them.
    /// Refused with `conflict` when `revision` is not the current one, `read-only`
    /// when the host locked its settings, and `bad-request` naming the op that failed.
    public func settingsUpdate(revision: String, ops: [SettingsOp]) async throws -> Settings {
        try field("settings", of: await request(.settingsUpdate(revision: revision, ops: ops)),
                  decoder: Coding.plainDecoder)
    }

    public func routines() async throws -> (routines: [Routine], problems: [RoutineProblem], off: [RoutinesOff]) {
        let reply = try await request(.routines)
        let off: [RoutinesOff] = try optionalField("off", of: reply) ?? []
        return (try field("routines", of: reply), try field("problems", of: reply), off)
    }

    /// Runs a Routine now; `refused(code: "conflict")` while it already runs.
    public func runRoutine(workspace: String, name: String) async throws {
        _ = try await request(.runRoutine(workspace: workspace, name: name))
    }

    // MARK: Reader

    /// The files of `kind` in the terminal's folder; only a Task's agent or a
    /// Routine run has one.
    public func readerList(_ terminal: String, kind: ReaderKind, html: Bool = false) async throws -> ReaderList {
        try field("list", of: await request(.readerList(terminal: terminal, kind: kind, html: html)))
    }

    /// Opens a listed file. `reader` and `readerEnded` events follow it until
    /// `readerClose`; a resume ends it.
    public func readerOpen(
        _ terminal: String, kind: ReaderKind, path: String, html: Bool = false, scope: DiffScope = .uncommitted
    ) async throws -> (reader: UInt32, content: ReaderContent) {
        let reply = try await request(.readerOpen(terminal: terminal, kind: kind, path: path, html: html, scope: scope))
        return (try field("reader", of: reply), try field("content", of: reply))
    }

    public func readerClose(_ reader: UInt32) async throws {
        _ = try await request(.readerClose(reader: reader))
    }

    /// The documents and images the Reader lists under `folder`, subfolders
    /// too, as a zip under a folder of that name (ADR 0101). Empty: all.
    public func readerFolder(_ terminal: String, folder: String) async throws -> Data {
        let zip: String = try field("zip", of: await request(.readerFolder(terminal: terminal, folder: folder)))
        guard let data = Data(base64Encoded: zip) else { throw V2Error.badMessage("zip: not base64") }
        return data
    }

    // MARK: Chat

    /// Follows the conversation of a Task's agent: `chat` and `chatEnded`
    /// events follow until `chatClose`; a resume ends it.
    public func chatOpen(_ terminal: String) async throws -> (chat: UInt32, update: ChatUpdate) {
        let reply = try await request(.chatOpen(terminal: terminal))
        return (try field("chat", of: reply), try field("update", of: reply))
    }

    public func chatClose(_ chat: UInt32) async throws {
        _ = try await request(.chatClose(chat: chat))
    }

    // MARK: Task Loop

    public func loops() async throws -> [LoopInfo] {
        try field("loops", of: await request(.loops))
    }

    /// Starts or restarts the loop of a Task's agent; `reset` counts the failed checks from 0.
    public func loopStart(_ terminal: String, spec: LoopSpec = LoopSpec(), reset: Bool = false) async throws -> LoopInfo {
        try field("loop", of: await request(.loopStart(terminal: terminal, spec: spec, reset: reset)))
    }

    public func loopStop(_ terminal: String) async throws -> LoopInfo {
        try field("loop", of: await request(.loopStop(terminal: terminal)))
    }

    // MARK: Terminal channels

    /// Sends input; it is kept until the server acknowledges it.
    public func input(_ channel: UInt32, _ bytes: [UInt8]) throws {
        guard var ch = state.channels[channel] else { throw V2Error.badMessage("no channel \(channel)") }
        ch.lastSeq += 1
        ch.pending.append((ch.lastSeq, bytes))
        state.channels[channel] = ch
        send(Frame.withU64(channel, ch.lastSeq, bytes))
    }

    public func resize(_ channel: UInt32, cols: UInt16, rows: UInt16) {
        state.channels[channel]?.cols = cols
        state.channels[channel]?.rows = rows
        send(Frame.resize(channel, cols: cols, rows: rows))
    }

    // MARK: Incoming

    private func handle(_ frame: Frame) {
        switch (frame.channel, frame.kind) {
        case (0, .data): control(Data(frame.payload))
        case (let n, .data): output(n, frame)
        case (let n, .ack):
            guard let seq = frame.splitU64()?.0 else { return }
            state.channels[n]?.pending.removeAll { $0.seq <= seq }
            if let acked = state.channels[n]?.acked { state.channels[n]?.acked = max(acked, seq) }
        case (let n, .close):
            state.channels[n] = nil
            let exit = try? Coding.decoder.decode(Exit.self, from: Data(frame.payload))
            eventsOut.yield(.closed(channel: n, exit: exit))
        default: break
        }
    }

    private func output(_ n: UInt32, _ frame: Frame) {
        guard let (offset, bytes) = frame.splitU64(), var ch = state.channels[n] else { return }
        // A resume may send again bytes this client already has.
        let skip = Int(min(UInt64(bytes.count), ch.offset > offset ? ch.offset - offset : 0))
        ch.offset = max(ch.offset, offset + UInt64(bytes.count))
        state.channels[n] = ch
        send(Frame.credit(n, UInt32(clamping: bytes.count)))
        if skip < bytes.count { eventsOut.yield(.output(channel: n, bytes: Array(bytes[skip...]))) }
    }

    private func control(_ data: Data) {
        guard let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let type = msg["type"] as? String else { return }
        let id = (msg["id"] as? NSNumber)?.uint64Value
        switch type {
        case "welcome":
            state.token = msg["token"] as? String
            backend = msg["backend"] as? String ?? ""
            resumed(msg["resumed"] as? [[String: Any]] ?? [])
            hello?.resume(returning: data)
            hello = nil
        case "error" where id == nil && hello != nil:
            hello?.resume(throwing: refusal(msg))
            hello = nil
        case "reply", "error":
            guard let id, let (cont, attach) = waiting.removeValue(forKey: id) else { return }
            if type == "error" { return cont.resume(throwing: refusal(msg)) }
            if let attach, let n = (msg["channel"] as? NSNumber)?.uint32Value { state.channels[n] = attach }
            cont.resume(returning: data)
        case "paint":
            guard let n = (msg["channel"] as? NSNumber)?.uint32Value else { return }
            state.channels[n]?.offset = 0
            eventsOut.yield(.paint(channel: n))
        case "changed":
            if let t: Terminal = try? field("terminal", of: data) { eventsOut.yield(.changed(t)) }
        case "removed":
            if let t = msg["terminal"] as? String { eventsOut.yield(.removed(terminal: t)) }
        case "resync":
            eventsOut.yield(.resync)
        case "inbox":
            if let i = try? Inbox.field(of: data) { eventsOut.yield(.inbox(i)) }
        case "chat":
            guard let n = (msg["chat"] as? NSNumber)?.uint32Value,
                  let update: ChatUpdate = try? field("update", of: data) else { return }
            eventsOut.yield(.chat(chat: n, update: update))
        case "chat-ended":
            guard let n = (msg["chat"] as? NSNumber)?.uint32Value else { return }
            eventsOut.yield(.chatEnded(chat: n, reason: msg["reason"] as? String ?? ""))
        case "loops":
            if let l: [LoopInfo] = try? field("loops", of: data) { eventsOut.yield(.loops(l)) }
        case "host":
            if let h: HostReport = try? field("host", of: data) { eventsOut.yield(.host(h)) }
        case "reader":
            guard let n = (msg["reader"] as? NSNumber)?.uint32Value,
                  let content: ReaderContent = try? field("content", of: data) else { return }
            eventsOut.yield(.reader(reader: n, content: content))
        case "reader-ended":
            guard let n = (msg["reader"] as? NSNumber)?.uint32Value else { return }
            eventsOut.yield(.readerEnded(reader: n, reason: msg["reason"] as? String ?? ""))
        default:
            break
        }
    }

    /// Resends input the server did not apply; ends channels it did not resume.
    private func resumed(_ listed: [[String: Any]]) {
        var applied: [UInt32: UInt64?] = [:]
        for r in listed {
            guard let n = (r["channel"] as? NSNumber)?.uint32Value else { continue }
            applied[n] = (r["input_seq"] as? NSNumber)?.uint64Value
        }
        resumed = applied.keys.sorted()
        for n in state.channels.keys.sorted() {
            guard let seq = applied[n] else {
                state.channels[n] = nil
                eventsOut.yield(.closed(channel: n, exit: nil))
                continue
            }
            guard var ch = state.channels[n] else { continue }
            if let seq {
                ch.pending.removeAll { $0.seq <= seq }
                ch.acked = max(ch.acked, seq)
                for p in ch.pending { send(Frame.withU64(n, p.seq, p.bytes)) }
            } else {
                // The server cannot tell what it applied: never type twice.
                ch.pending = []
            }
            state.channels[n] = ch
        }
    }

    private func lost(_ why: String) {
        guard gone == nil else { return }
        gone = why
        hello?.resume(throwing: V2Error.lost(why))
        hello = nil
        for (cont, _) in waiting.values { cont.resume(throwing: V2Error.lost(why)) }
        waiting = [:]
        eventsOut.yield(.lost(why))
        eventsOut.finish()
        outbox.finish()
    }

    private func send(_ frame: Frame) {
        outbox.yield(frame.encode())
    }
}

private func refusal(_ msg: [String: Any]) -> V2Error {
    .refused(code: msg["code"] as? String ?? "?", message: msg["message"] as? String ?? "")
}

/// `key` of a reply, decoded; nil when the reply has no `key` (an older server).
func optionalField<T: Decodable>(_ key: String, of data: Data) throws -> T? {
    guard let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any], msg[key] != nil else { return nil }
    return try field(key, of: data) as T
}

/// `key` of a reply, decoded.
func field<T: Decodable>(_ key: String, of data: Data, decoder: JSONDecoder = Coding.decoder) throws -> T {
    guard let msg = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any], let value = msg[key] else {
        throw V2Error.badMessage("no \(key) in the reply")
    }
    let json = try JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
    do {
        return try decoder.decode(T.self, from: json)
    } catch {
        throw V2Error.badMessage("\(key): \(error)")
    }
}
