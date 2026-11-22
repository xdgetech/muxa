import Foundation

/// A request on channel 0. The client adds `id`.
public enum V2Request: Sendable {
    case terminals
    case attach(terminal: String, cols: UInt16, rows: UInt16, history: UInt32 = 0)
    case detach(channel: UInt32)
    /// No places: one folder (ADR 0068).
    /// `connections` and `mcp`: the Workspace's, besides the default ones (ADR 0069).
    case startTask(
        workspace: String, title: String, goal: String? = nil, places: [PlaceSpec] = [], connections: [String] = [],
        mcp: [String] = [], plan: Bool = false, loop: LoopSpec? = nil)
    case watch
    case inbox
    /// `today` is the device's local date, `YYYY-MM-DD`.
    case parseCapture(text: String, today: String)
    case inboxAdd(Capture, note: String? = nil)
    case inboxMove(item: String, index: Int)
    /// `until` is Unix seconds.
    case inboxLater(item: String, until: Int64)
    case inboxDone(item: String)
    case inboxReopen(item: String)
    case inboxDelete(item: String)
    /// Closes the Review item's Task. `worktree` nil means `remove`.
    case inboxApprove(item: String, worktree: WorktreeChoice? = nil)
    /// Types `comment` into the Review item's Task agent and resolves the item.
    case inboxRequestChanges(item: String, comment: String)
    /// Runs again what made a Failed item and resolves it.
    case inboxRetry(item: String)
    case taskReview(terminal: String, note: String? = nil)
    case routines
    case runRoutine(workspace: String, name: String)
    case readerList(terminal: String, kind: ReaderKind, html: Bool = false)
    case readerOpen(terminal: String, kind: ReaderKind, path: String, html: Bool = false, scope: DiffScope = .uncommitted)
    case readerClose(reader: UInt32)
    case loops
    case loopStart(terminal: String, spec: LoopSpec, reset: Bool = false)
    case loopStop(terminal: String)
    case chatOpen(terminal: String)
    case chatClose(chat: UInt32)
    case workspaces
    /// The host's editable settings (ADR 0074).
    case settings
    /// `revision` must be the current one; the ops apply all or none.
    case settingsUpdate(revision: String, ops: [SettingsOp])

    func json(id: UInt64) throws -> Data {
        var m: [String: Any] = ["id": id]
        func set(_ type: String, _ fields: [String: Any?]) {
            m["type"] = type
            for (k, v) in fields { if let v { m[k] = v } }
        }
        switch self {
        case .terminals: set("terminals", [:])
        case let .attach(terminal, cols, rows, history):
            set("attach", ["terminal": terminal, "cols": cols, "rows": rows, "history": history])
        case let .detach(channel): set("detach", ["channel": channel])
        case let .startTask(workspace, title, goal, places, connections, mcp, plan, loop):
            set("start-task", ["workspace": workspace, "title": title, "goal": goal,
                               "places": places.map(\.json), "connections": connections, "mcp": mcp,
                               "plan": plan, "loop": loop?.json])
        case .watch: set("watch", [:])
        case .inbox: set("inbox", [:])
        case let .parseCapture(text, today): set("parse-capture", ["text": text, "today": today])
        case let .inboxAdd(c, note):
            set("inbox-add", ["kind": c.kind.rawValue, "title": c.title, "workspace": c.workspace,
                              "due": c.due, "note": note])
        case let .inboxMove(item, index): set("inbox-move", ["item": item, "index": index])
        case let .inboxLater(item, until): set("inbox-later", ["item": item, "until": until])
        case let .inboxDone(item): set("inbox-done", ["item": item])
        case let .inboxReopen(item): set("inbox-reopen", ["item": item])
        case let .inboxDelete(item): set("inbox-delete", ["item": item])
        case let .inboxApprove(item, worktree):
            set("inbox-approve", ["item": item, "worktree": worktree?.rawValue])
        case let .inboxRequestChanges(item, comment):
            set("inbox-request-changes", ["item": item, "comment": comment])
        case let .inboxRetry(item): set("inbox-retry", ["item": item])
        case let .taskReview(terminal, note): set("task-review", ["terminal": terminal, "note": note])
        case .routines: set("routines", [:])
        case let .runRoutine(workspace, name): set("run-routine", ["workspace": workspace, "name": name])
        case let .readerList(terminal, kind, html):
            set("reader-list", ["terminal": terminal, "kind": kind.rawValue, "html": html])
        case let .readerOpen(terminal, kind, path, html, scope):
            set("reader-open", ["terminal": terminal, "kind": kind.rawValue, "path": path, "html": html,
                                "scope": scope.rawValue])
        case let .readerClose(reader): set("reader-close", ["reader": reader])
        case .loops: set("loops", [:])
        case let .loopStart(terminal, spec, reset):
            set("loop-start", ["terminal": terminal, "loop": spec.json, "reset": reset])
        case let .loopStop(terminal): set("loop-stop", ["terminal": terminal])
        case let .chatOpen(terminal): set("chat-open", ["terminal": terminal])
        case let .chatClose(chat): set("chat-close", ["chat": chat])
        case .workspaces: set("workspaces", [:])
        case .settings: set("settings", [:])
        case let .settingsUpdate(revision, ops):
            set("settings-update", ["revision": revision, "ops": try ops.map { try $0.jsonObject() }])
        }
        return try JSONSerialization.data(withJSONObject: m, options: [.sortedKeys])
    }
}
