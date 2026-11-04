import Foundation

/// Sent by the client. Every request has an `id`, echoed in its `result`.
public enum Request: Equatable, Sendable {
    case ping(id: UInt64)
    case list(id: UInt64)
    /// Show this pane. `screen` and `output` for it carry this request's id as `watch`.
    case watch(id: UInt64, pane: String)
    case unwatch(id: UInt64)
    /// The grid the pane area shows at the default font (capability `viewport`,
    /// ADR 0020). The bridge sizes the watched pane's window for it.
    case viewport(id: UInt64, cols: Int, rows: Int)
    /// Named keys, e.g. `C-c`, `Up`, `S-Enter`.
    case keys(id: UInt64, pane: String, keys: [String])
    /// Typed literally.
    case text(id: UInt64, pane: String, text: String)
    /// Raw key bytes, for a client that encodes keys itself.
    case bytes(id: UInt64, pane: String, data: Data)
    /// Pasted, bracketed when the program asked for it.
    case paste(id: UInt64, pane: String, text: String)
    /// Follow the conversation of the agent in this pane. Needs the `chat`
    /// capability. `chat-entries` for it carry this request's id as `chat`.
    case chat(id: UInt64, pane: String)
    case unchat(id: UInt64)
    /// This pane's Markdown documents (capability `docs`); a `doc-list` comes first.
    case docs(id: UInt64, pane: String)
    /// Open a listed document and follow its changes; `doc-content` carries this id as `doc`.
    case doc(id: UInt64, pane: String, path: String)
    case undoc(id: UInt64)
    /// The files that differ from HEAD in the pane's repo (capability `changes`);
    /// a `change-list` comes first.
    case changes(id: UInt64, pane: String)
    /// Show and follow one listed file's diff; `diff-content` carries this id as `diff`.
    /// `scope` is the list the path is from; nil means the uncommitted one.
    case diff(id: UInt64, pane: String, path: String, scope: DiffScope?)
    case undiff(id: UInt64)
    /// The image files in the pane's repo, ignored ones included (capability
    /// `images`); an `image-list` comes first.
    case images(id: UInt64, pane: String)
    /// Open a listed image and follow its changes; `image-content` carries this id as `image`.
    case image(id: UInt64, pane: String, path: String)
    case unimage(id: UInt64)
    /// Start a task on `branch` of the repo at `repo` (capability `tasks`); a
    /// `task-created` comes first. `base` is where a new branch starts.
    case newTask(id: UInt64, repo: String, branch: String, base: String?)
    /// Close a task's session (capability `close-task`). The worktree goes only as
    /// `mode` allows, never with uncommitted changes; the branch stays.
    case closeTask(id: UInt64, session: String, mode: CloseMode)
    /// The task records and the workspaces (capability `task-records`); a
    /// `task-list` comes first.
    case listTasks(id: UInt64)
    /// Record a new task and start an agent for it; a `task-saved` comes first.
    /// Without `repo` the task gets a directory in its workspace.
    /// With `loop`, the host also starts a Task Loop (capability `task-loop`).
    case createTask(
        id: UInt64, workspace: String, title: String, goal: String, project: String?, repo: String?,
        branch: String?, base: String?, loop: LoopSpec? = nil, worker: String? = nil)
    /// Set a task record's status. `rev` is the one the client saw; a stale one
    /// fails with `conflict`. A `task-saved` comes first.
    case updateTask(id: UInt64, task: String, rev: UInt64, status: TaskStatus)
    /// Restart a task's agent in the same pane, after a handoff (capability
    /// `task-handoff`, ADR 0018). Refused while the agent runs, unless `force`.
    /// A `task-saved` comes first.
    /// `worker` is a name or `default`; nil keeps the current worker.
    case restartAgent(id: UInt64, task: String, force: Bool, worker: String? = nil, rev: UInt64? = nil)
    /// Start a task's Task Loop again (capability `task-loop`, ADR 0019); `reset`
    /// counts the attempts from zero. A `task-saved` comes first.
    case recoverRestart(id: UInt64, task: String, rev: UInt64, operation: String, action: RestartRecoveryAction)
    case startLoop(id: UInt64, task: String, loop: LoopSpec, reset: Bool)
    /// Stop a task's Task Loop (capability `task-loop`). A `task-saved` comes first.
    case stopLoop(id: UInt64, task: String)
    /// The host's Routines (capability `work` level 1, ADR 0026).
    case listRoutines(id: UInt64)
    /// Start a run of an enabled Routine now. Needs the host daemon.
    case runRoutine(id: UInt64, workspace: String, routine: String)
    /// The stored Inbox entries (capability `work` level 1).
    case listInbox(id: UInt64)
    /// Answer a stored entry or an Inbox item (its `i-` id). `until` goes with
    /// `later`, `text` with `redo`, `change-course` and `reply`, `option` with `choose`.
    case updateInboxEntry(id: UInt64, entry: String, action: InboxAction, until: UInt64?, text: String? = nil, option: UInt32? = nil)
    /// An agent session in a repo's main checkout, to ask about its code: no
    /// Task, no worktree (capability `work` level 2, ADR 0031). `repo` is as
    /// `createTask`'s. A `session-started` comes first.
    case startSession(id: UInt64, workspace: String, repo: String)
    /// The client is leaving; the bridge exits.
    case bye(id: UInt64)

    public var id: UInt64 {
        switch self {
        case .ping(let id), .list(let id), .unwatch(let id), .unchat(let id), .undoc(let id), .bye(let id), .docs(let id, _), .doc(let id, _, _),
            .watch(let id, _), .chat(let id, _), .keys(let id, _, _), .text(let id, _, _),
            .bytes(let id, _, _), .paste(let id, _, _), .newTask(let id, _, _, _),
            .changes(let id, _), .diff(let id, _, _, _), .undiff(let id), .closeTask(let id, _, _),
            .images(let id, _), .image(let id, _, _), .unimage(let id), .listTasks(let id),
            .createTask(let id, _, _, _, _, _, _, _, _, _), .updateTask(let id, _, _, _),
            .restartAgent(let id, _, _, _, _), .recoverRestart(let id, _, _, _, _), .startLoop(let id, _, _, _), .stopLoop(let id, _),
            .viewport(let id, _, _), .listRoutines(let id), .runRoutine(let id, _, _), .listInbox(let id),
            .updateInboxEntry(let id, _, _, _, _, _), .startSession(let id, _, _):
            id
        }
    }
}

extension Request: Codable {
    fileprivate enum Key: String, CodingKey {
        case type, id, pane, keys, text, data, path, repo, branch, base, session, mode, scope
        case workspace, title, goal, project, task, rev, status, force, loop, reset, worker
        case cols, rows, operation, action
        case routine, entry, until, option
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        switch self {
        case .ping(let id): try c.head("ping", id)
        case .list(let id): try c.head("list", id)
        case .unwatch(let id): try c.head("unwatch", id)
        case .viewport(let id, let cols, let rows):
            try c.head("viewport", id)
            try c.encode(cols, forKey: .cols)
            try c.encode(rows, forKey: .rows)
        case .bye(let id): try c.head("bye", id)
        case .unchat(let id): try c.head("unchat", id)
        case .undoc(let id): try c.head("undoc", id)
        case .undiff(let id): try c.head("undiff", id)
        case .unimage(let id): try c.head("unimage", id)
        case .images(let id, let pane):
            try c.head("images", id)
            try c.encode(pane, forKey: .pane)
        case .image(let id, let pane, let path):
            try c.head("image", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(path, forKey: .path)
        case .closeTask(let id, let session, let mode):
            try c.head("close-task", id)
            try c.encode(session, forKey: .session)
            try c.encode(mode, forKey: .mode)
        case .listTasks(let id): try c.head("list-tasks", id)
        case .createTask(
            let id, let workspace, let title, let goal, let project, let repo, let branch, let base, let loop, let worker):
            try c.head("create-task", id)
            try c.encode(workspace, forKey: .workspace)
            try c.encode(title, forKey: .title)
            try c.encode(goal, forKey: .goal)
            try c.encodeIfPresent(project, forKey: .project)
            try c.encodeIfPresent(repo, forKey: .repo)
            try c.encodeIfPresent(branch, forKey: .branch)
            try c.encodeIfPresent(base, forKey: .base)
            try c.encodeIfPresent(loop, forKey: .loop)
            try c.encodeIfPresent(worker, forKey: .worker)
        case .updateTask(let id, let task, let rev, let status):
            try c.head("update-task", id)
            try c.encode(task, forKey: .task)
            try c.encode(rev, forKey: .rev)
            try c.encode(status, forKey: .status)
        case .restartAgent(let id, let task, let force, let worker, let rev):
            try c.head("restart-agent", id)
            try c.encode(task, forKey: .task)
            if force { try c.encode(true, forKey: .force) }
            try c.encodeIfPresent(worker, forKey: .worker)
            try c.encodeIfPresent(rev, forKey: .rev)
        case .recoverRestart(let id, let task, let rev, let operation, let action):
            try c.head("recover-restart", id)
            try c.encode(task, forKey: .task)
            try c.encode(rev, forKey: .rev)
            try c.encode(operation, forKey: .operation)
            try c.encode(action, forKey: .action)
        case .startLoop(let id, let task, let loop, let reset):
            try c.head("start-loop", id)
            try c.encode(task, forKey: .task)
            try c.encode(loop, forKey: .loop)
            if reset { try c.encode(true, forKey: .reset) }
        case .stopLoop(let id, let task):
            try c.head("stop-loop", id)
            try c.encode(task, forKey: .task)
        case .listRoutines(let id): try c.head("list-routines", id)
        case .runRoutine(let id, let workspace, let routine):
            try c.head("run-routine", id)
            try c.encode(workspace, forKey: .workspace)
            try c.encode(routine, forKey: .routine)
        case .listInbox(let id): try c.head("list-inbox", id)
        case .startSession(let id, let workspace, let repo):
            try c.head("start-session", id)
            try c.encode(workspace, forKey: .workspace)
            try c.encode(repo, forKey: .repo)
        case .updateInboxEntry(let id, let entry, let action, let until, let text, let option):
            try c.head("update-inbox", id)
            try c.encode(entry, forKey: .entry)
            try c.encode(action, forKey: .action)
            try c.encodeIfPresent(until, forKey: .until)
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(option, forKey: .option)
        case .changes(let id, let pane):
            try c.head("changes", id)
            try c.encode(pane, forKey: .pane)
        case .diff(let id, let pane, let path, let scope):
            try c.head("diff", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(path, forKey: .path)
            try c.encodeIfPresent(scope, forKey: .scope)
        case .docs(let id, let pane):
            try c.head("docs", id)
            try c.encode(pane, forKey: .pane)
        case .doc(let id, let pane, let path):
            try c.head("doc", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(path, forKey: .path)
        case .newTask(let id, let repo, let branch, let base):
            try c.head("new-task", id)
            try c.encode(repo, forKey: .repo)
            try c.encode(branch, forKey: .branch)
            try c.encodeIfPresent(base, forKey: .base)
        case .watch(let id, let pane):
            try c.head("watch", id)
            try c.encode(pane, forKey: .pane)
        case .chat(let id, let pane):
            try c.head("chat", id)
            try c.encode(pane, forKey: .pane)
        case .keys(let id, let pane, let keys):
            try c.head("keys", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(keys, forKey: .keys)
        case .text(let id, let pane, let text):
            try c.head("text", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(text, forKey: .text)
        case .bytes(let id, let pane, let data):
            try c.head("bytes", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(data.base64EncodedString(), forKey: .data)
        case .paste(let id, let pane, let text):
            try c.head("paste", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(text, forKey: .text)
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        let type = try c.decode(String.self, forKey: .type)
        let id = try c.decode(UInt64.self, forKey: .id)
        func pane() throws -> String { try c.decode(String.self, forKey: .pane) }
        switch type {
        case "ping": self = .ping(id: id)
        case "list": self = .list(id: id)
        case "unwatch": self = .unwatch(id: id)
        case "viewport":
            self = .viewport(
                id: id, cols: try c.decode(Int.self, forKey: .cols), rows: try c.decode(Int.self, forKey: .rows))
        case "bye": self = .bye(id: id)
        case "watch": self = .watch(id: id, pane: try pane())
        case "chat": self = .chat(id: id, pane: try pane())
        case "unchat": self = .unchat(id: id)
        case "undoc": self = .undoc(id: id)
        case "undiff": self = .undiff(id: id)
        case "unimage": self = .unimage(id: id)
        case "images": self = .images(id: id, pane: try pane())
        case "image": self = .image(id: id, pane: try pane(), path: try c.decode(String.self, forKey: .path))
        case "close-task":
            self = .closeTask(
                id: id, session: try c.decode(String.self, forKey: .session),
                mode: try c.decode(CloseMode.self, forKey: .mode))
        case "list-tasks": self = .listTasks(id: id)
        case "create-task":
            self = .createTask(
                id: id, workspace: try c.decode(String.self, forKey: .workspace),
                title: try c.decode(String.self, forKey: .title),
                goal: try c.decode(String.self, forKey: .goal),
                project: try c.decodeIfPresent(String.self, forKey: .project),
                repo: try c.decodeIfPresent(String.self, forKey: .repo),
                branch: try c.decodeIfPresent(String.self, forKey: .branch),
                base: try c.decodeIfPresent(String.self, forKey: .base),
                loop: try c.decodeIfPresent(LoopSpec.self, forKey: .loop),
                worker: try c.decodeIfPresent(String.self, forKey: .worker))
        case "update-task":
            self = .updateTask(
                id: id, task: try c.decode(String.self, forKey: .task),
                rev: try c.decode(UInt64.self, forKey: .rev),
                status: try c.decode(TaskStatus.self, forKey: .status))
        case "restart-agent":
            self = .restartAgent(
                id: id, task: try c.decode(String.self, forKey: .task),
                force: try c.decodeIfPresent(Bool.self, forKey: .force) ?? false,
                worker: try c.decodeIfPresent(String.self, forKey: .worker),
                rev: try c.decodeIfPresent(UInt64.self, forKey: .rev))
        case "recover-restart":
            self = .recoverRestart(id: id, task: try c.decode(String.self, forKey: .task),
                rev: try c.decode(UInt64.self, forKey: .rev), operation: try c.decode(String.self, forKey: .operation),
                action: try c.decode(RestartRecoveryAction.self, forKey: .action))
        case "start-loop":
            self = .startLoop(
                id: id, task: try c.decode(String.self, forKey: .task),
                loop: try c.decode(LoopSpec.self, forKey: .loop),
                reset: try c.decodeIfPresent(Bool.self, forKey: .reset) ?? false)
        case "stop-loop": self = .stopLoop(id: id, task: try c.decode(String.self, forKey: .task))
        case "list-routines": self = .listRoutines(id: id)
        case "run-routine":
            self = .runRoutine(
                id: id, workspace: try c.decode(String.self, forKey: .workspace),
                routine: try c.decode(String.self, forKey: .routine))
        case "list-inbox": self = .listInbox(id: id)
        case "start-session":
            self = .startSession(
                id: id, workspace: try c.decode(String.self, forKey: .workspace),
                repo: try c.decode(String.self, forKey: .repo))
        case "update-inbox":
            self = .updateInboxEntry(
                id: id, entry: try c.decode(String.self, forKey: .entry),
                action: try c.decode(InboxAction.self, forKey: .action),
                until: try c.decodeIfPresent(UInt64.self, forKey: .until),
                text: try c.decodeIfPresent(String.self, forKey: .text),
                option: try c.decodeIfPresent(UInt32.self, forKey: .option))
        case "changes": self = .changes(id: id, pane: try pane())
        case "diff":
            self = .diff(
                id: id, pane: try pane(), path: try c.decode(String.self, forKey: .path),
                scope: try c.decodeIfPresent(DiffScope.self, forKey: .scope))
        case "docs": self = .docs(id: id, pane: try pane())
        case "doc": self = .doc(id: id, pane: try pane(), path: try c.decode(String.self, forKey: .path))
        case "new-task":
            self = .newTask(
                id: id, repo: try c.decode(String.self, forKey: .repo),
                branch: try c.decode(String.self, forKey: .branch),
                base: try c.decodeIfPresent(String.self, forKey: .base))
        case "keys": self = .keys(id: id, pane: try pane(), keys: try c.decode([String].self, forKey: .keys))
        case "text": self = .text(id: id, pane: try pane(), text: try c.decode(String.self, forKey: .text))
        case "paste": self = .paste(id: id, pane: try pane(), text: try c.decode(String.self, forKey: .text))
        case "bytes": self = .bytes(id: id, pane: try pane(), data: try c.decodeBase64(forKey: .data))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: c, debugDescription: "unknown request type \(type)")
        }
    }
}

extension KeyedEncodingContainer<Request.Key> {
    fileprivate mutating func head(_ type: String, _ id: UInt64) throws {
        try encode(type, forKey: .type)
        try encode(id, forKey: .id)
    }
}

/// What happens to a task's worktree when it closes.
public enum CloseMode: String, Codable, Sendable {
    /// Remove it when the branch is merged; otherwise refuse.
    case close
    /// Keep it.
    case keep
    /// Remove it even when the branch is not merged.
    case force
}
