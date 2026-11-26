import Foundation

/// A request on channel 0. The client adds `id`.
public enum V2Request: Sendable {
    case terminals
    case attach(terminal: String, cols: UInt16, rows: UInt16, history: UInt32 = 0)
    case detach(channel: UInt32)
    /// No places: one folder (ADR 0068).
    /// `connections` and `mcp` are deprecated and ignored: every agent gets all the
    /// Workspace's connections and MCP servers (ADR 0075). `bot` makes it a talk
    /// with that Bot of the Workspace, or one to set up (ADR 0083).
    case startTask(
        workspace: String, title: String, goal: String? = nil, places: [PlaceSpec] = [], connections: [String] = [],
        mcp: [String] = [], plan: Bool = false, loop: LoopSpec? = nil, bot: String? = nil)
    case watch
    case inbox
    /// `today` is the device's local date, `YYYY-MM-DD`.
    case parseCapture(text: String, today: String)
    case inboxAdd(Capture, note: String? = nil)
    case inboxMove(item: String, index: Int)
    /// Moves the item to the bottom (ADR 0096).
    case inboxLater(item: String)
    case inboxDone(item: String)
    case inboxReopen(item: String)
    case inboxDelete(item: String)
    /// Deletes every open and done item older than `days`, 1 to 3650 (ADR 0095).
    case inboxDeleteOlder(days: Int)
    /// Closes the Review item's Task. `worktree` nil means `remove`.
    case inboxApprove(item: String, worktree: WorktreeChoice? = nil)
    /// Types `comment` into the Review item's Task agent and resolves the item.
    case inboxRequestChanges(item: String, comment: String)
    /// Runs again what made a Failed item and resolves it.
    case inboxRetry(item: String)
    /// Applies the item's settings proposal with its revision and marks it Done (ADR 0083).
    case inboxApply(item: String)
    /// Accepts or dismisses the item's suggestion (ADR 0084).
    case inboxSuggestion(item: String, accept: Bool)
    case taskReview(terminal: String, note: String? = nil)
    /// A shell beside the Task's agent `terminal`, in its working place (ADR 0079).
    case taskShell(terminal: String)
    /// Ends a Task's shell; no other terminal.
    case closeTerminal(terminal: String)
    /// A file into the Task's working place, under `.muxa/uploads/` (ADR 0081).
    case upload(terminal: String, name: String, data: Data)
    /// Closes the Task of the agent `terminal` as Approve does, and its open items.
    case closeTask(terminal: String, worktree: WorktreeChoice? = nil)
    /// Stops the Task's agent and shells and keeps the Task (ADR 0104).
    case pauseTask(terminal: String)
    /// Runs the paused Task `terminal` again, in its places.
    case resumeTask(terminal: String)
    case pausedTasks
    case routines
    case runRoutine(workspace: String, name: String)
    case readerList(terminal: String, kind: ReaderKind, html: Bool = false)
    case readerOpen(terminal: String, kind: ReaderKind, path: String, html: Bool = false, scope: DiffScope = .uncommitted)
    case readerClose(reader: UInt32)
    /// The listed documents and images under `folder`, as one zip (ADR 0101).
    case readerFolder(terminal: String, folder: String)
    case loops
    case loopStart(terminal: String, spec: LoopSpec, reset: Bool = false)
    case loopStop(terminal: String)
    case chatOpen(terminal: String)
    case chatClose(chat: UInt32)
    case workspaces
    /// Each subscription and its agents' last report of the plan limits (ADR 0103).
    case subscriptions
    /// Opens the agent's own login for `subscription` in a host terminal, or
    /// the one already open (ADR 0103 §7).
    case loginStart(subscription: String)
    /// The tokens agents used on `day` (`YYYY-MM-DD`, nil for today), by
    /// Workspace and by Task (ADR 0103 §6).
    case tokenUsage(day: String?)
    /// The disk use of Task places and leftovers (ADR 0102).
    case disk
    /// A dry run unless `dryRun` is false; `force` needs `paths`.
    case diskClean(paths: [String] = [], force: Bool = false, dryRun: Bool = true)
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
        case let .startTask(workspace, title, goal, places, connections, mcp, plan, loop, bot):
            set("start-task", ["workspace": workspace, "title": title, "goal": goal,
                               "places": places.map(\.json), "connections": connections, "mcp": mcp,
                               "plan": plan, "loop": loop?.json, "bot": bot])
        case .watch: set("watch", [:])
        case .inbox: set("inbox", [:])
        case let .parseCapture(text, today): set("parse-capture", ["text": text, "today": today])
        case let .inboxAdd(c, note):
            set("inbox-add", ["kind": c.kind.rawValue, "title": c.title, "workspace": c.workspace,
                              "due": c.due, "note": note])
        case let .inboxMove(item, index): set("inbox-move", ["item": item, "index": index])
        case let .inboxLater(item): set("inbox-later", ["item": item])
        case let .inboxDone(item): set("inbox-done", ["item": item])
        case let .inboxReopen(item): set("inbox-reopen", ["item": item])
        case let .inboxDelete(item): set("inbox-delete", ["item": item])
        case let .inboxDeleteOlder(days): set("inbox-delete-older", ["days": days])
        case let .inboxApprove(item, worktree):
            set("inbox-approve", ["item": item, "worktree": worktree?.rawValue])
        case let .inboxRequestChanges(item, comment):
            set("inbox-request-changes", ["item": item, "comment": comment])
        case let .inboxRetry(item): set("inbox-retry", ["item": item])
        case let .inboxApply(item): set("inbox-apply", ["item": item])
        case let .inboxSuggestion(item, accept): set("inbox-suggestion", ["item": item, "accept": accept])
        case let .taskReview(terminal, note): set("task-review", ["terminal": terminal, "note": note])
        case let .taskShell(terminal): set("task-shell", ["terminal": terminal])
        case let .closeTerminal(terminal): set("close-terminal", ["terminal": terminal])
        case let .upload(terminal, name, data):
            set("upload", ["terminal": terminal, "name": name, "data": data.base64EncodedString()])
        case let .closeTask(terminal, worktree):
            set("close-task", ["terminal": terminal, "worktree": worktree?.rawValue])
        case let .pauseTask(terminal): set("pause-task", ["terminal": terminal])
        case let .resumeTask(terminal): set("resume-task", ["terminal": terminal])
        case .pausedTasks: set("paused-tasks", [:])
        case .routines: set("routines", [:])
        case let .runRoutine(workspace, name): set("run-routine", ["workspace": workspace, "name": name])
        case let .readerList(terminal, kind, html):
            set("reader-list", ["terminal": terminal, "kind": kind.rawValue, "html": html])
        case let .readerOpen(terminal, kind, path, html, scope):
            set("reader-open", ["terminal": terminal, "kind": kind.rawValue, "path": path, "html": html,
                                "scope": scope.rawValue])
        case let .readerClose(reader): set("reader-close", ["reader": reader])
        case let .readerFolder(terminal, folder): set("reader-folder", ["terminal": terminal, "folder": folder])
        case .loops: set("loops", [:])
        case let .loopStart(terminal, spec, reset):
            set("loop-start", ["terminal": terminal, "loop": spec.json, "reset": reset])
        case let .loopStop(terminal): set("loop-stop", ["terminal": terminal])
        case let .chatOpen(terminal): set("chat-open", ["terminal": terminal])
        case let .chatClose(chat): set("chat-close", ["chat": chat])
        case .workspaces: set("workspaces", [:])
        case .subscriptions: set("subscriptions", [:])
        case let .loginStart(subscription): set("login-start", ["subscription": subscription])
        case let .tokenUsage(day): set("token-usage", ["day": day])
        case .disk: set("disk", [:])
        case let .diskClean(paths, force, dryRun):
            set("disk-clean", ["paths": paths, "force": force, "dry_run": dryRun])
        case .settings: set("settings", [:])
        case let .settingsUpdate(revision, ops):
            set("settings-update", ["revision": revision, "ops": try ops.map { try $0.jsonObject() }])
        }
        return try JSONSerialization.data(withJSONObject: m, options: [.sortedKeys])
    }
}
