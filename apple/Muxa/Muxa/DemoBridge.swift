#if DEBUG
import Foundation
import MuxaProtocol
import MuxaTransport

/// An in-app bridge for the simulator (launch argument `-demo`): a few sessions,
/// fixed-size panes, typed text echoed back, and a chat for the agent pane %1.
/// No SSH, no tmux.
final class DemoBridge: BridgeChannel, @unchecked Sendable {
    let lines: AsyncThrowingStream<Data, any Error>
    private let out: AsyncThrowingStream<Data, any Error>.Continuation
    private let lock = NSLock()
    private var watch: UInt64 = 0
    private var chat: (id: UInt64, seq: UInt64, count: Int)?
    private var openDoc: UInt64?
    private var openDiff: UInt64?
    /// The last paste into %1, sent as a prompt when Enter follows.
    private var prompt: String?
    /// What the demo agent waits on, and the tool call it waits on.
    private var waitingOn: (prompt: ChatPrompt, tool: ChatEntry)?
    /// Grows when a task starts.
    private var sessions: [Session]
    private let content = DemoContent.current
    static let chatPane = "%1"
    /// A branch whose task fails, to show the error.
    static let failingBranch = "taken"

    static let initialSessions = [
        Session(id: "$1", name: "feature-auth", repo: "app", repoDir: "/src/app", branch: "feat/auth",
                worktree: "/src/.worktrees/app-feat-auth", panes: [
            Pane(id: "%1", window: "@1", index: 0, role: .agent, cols: 100, rows: 30, command: "claude",
                 path: "/work", dead: false, shellForeground: false,
                 agent: Agent(state: .waiting, since: UInt64(Date().timeIntervalSince1970) - 90)),
            Pane(id: "%2", window: "@1", index: 1, role: .shell, cols: 100, rows: 30, command: "zsh",
                 path: "/work", dead: false, shellForeground: true),
        ]),
        Session(id: "$4", name: "fix-login", repo: "app", repoDir: "/src/app", branch: "fix/login",
                worktree: "/src/.worktrees/app-fix-login", panes: [
            Pane(id: "%5", window: "@4", index: 0, role: .agent, cols: 100, rows: 30, command: "claude",
                 path: "/work", dead: false, shellForeground: false,
                 agent: Agent(state: .idle, since: UInt64(Date().timeIntervalSince1970) - 600)),
        ]),
        Session(id: "$5", name: "docs-site", repo: "docs", repoDir: "/src/docs", branch: "main", panes: [
            Pane(id: "%6", window: "@5", index: 0, role: .agent, cols: 100, rows: 30, command: "claude",
                 path: "/docs", dead: false, shellForeground: false,
                 agent: Agent(state: .running, since: UInt64(Date().timeIntervalSince1970) - 20)),
        ]),
        Session(id: "$2", name: "scratch", panes: [
            Pane(id: "%3", window: "@2", index: 0, role: .unknown, cols: 132, rows: 43, command: "vim",
                 path: "/tmp", dead: false, shellForeground: false),
        ]),
        // Taller than a landscape iPad at a readable size: it needs fit width and scrolling.
        Session(id: "$3", name: "tall", panes: [
            Pane(id: "%4", window: "@3", index: 0, role: .shell, cols: 100, rows: 60, command: "zsh",
                 path: "/tmp", dead: false, shellForeground: true),
        ]),
        // A tall agent with its prompt at the bottom, beside a shell that has just started.
        Session(id: "$6", name: "pair", panes: [
            Pane(id: "%7", window: "@6", index: 0, role: .agent, cols: 100, rows: 60, command: "claude",
                 path: "/tmp", dead: false, shellForeground: false),
            Pane(id: "%8", window: "@6", index: 1, role: .shell, cols: 100, rows: 60, command: "zsh",
                 path: "/tmp", dead: false, shellForeground: true),
        ]),
    ]
    /// Shells with nothing run yet: a title bar on the first row and the prompt under it.
    static let freshShells: Set<String> = ["%8"]

    init() {
        sessions = content.sessions
        (lines, out) = AsyncThrowingStream<Data, any Error>.makeStream()
        // Launch argument `-demo-alerts`: an agent elsewhere starts waiting a few seconds in.
        if ProcessInfo.processInfo.arguments.contains("-demo-alerts") {
            DispatchQueue.global().asyncAfter(deadline: .now() + content.alertDelay) { [weak self] in self?.agentWaits("$4") }
        }
        emit(.hello(protocol: protocolVersion, bridge: "demo", tmux: "demo", capabilities: ["chat", "docs", "tasks", "changes", "close-task"]))
    }

    func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        switch req {
        case .list(let id):
            emit(.result(id: id, error: nil, sessions: lock.withLock { sessions }))
        case .watch(let id, let pane):
            lock.withLock { watch = id }
            emit(.result(id: id, error: nil, sessions: nil))
            guard let p = lock.withLock({ sessions }).flatMap(\.panes).first(where: { $0.id == pane }) else { return }
            emit(.screen(watch: id, pane: pane, cols: p.cols, rows: p.rows, data: content.screen(p)))
        case .text(let id, let pane, let text):
            ok(id)
            echo(pane, Data(text.utf8))
        case .paste(let id, let pane, let text):
            ok(id)
            if pane == Self.chatPane { lock.withLock { prompt = text } }
            echo(pane, Data(text.replacingOccurrences(of: "\n", with: "\r\n").utf8))
        case .chat(let id, let pane):
            guard pane == Self.chatPane else {
                emit(.result(id: id, error: ErrorBody(code: .unsupported, message: "no agent adapter knows this pane"), sessions: nil))
                return
            }
            ok(id)
            lock.withLock { chat = (id, 0, 0) }
            emitChat(reset: true, entries: content.conversation, status: content.chatStatus)
        case .unchat(let id):
            lock.withLock { chat = nil }
            ok(id)
        case .docs(let id, _):
            emit(.docList(DocListing(docs: id, root: content.root, entries: content.docs.map(\.info))))
            ok(id)
        case .doc(let id, _, let path):
            guard let doc = content.docs.first(where: { $0.info.path == path }) else {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(path) is not listed"), sessions: nil))
                return
            }
            lock.withLock { openDoc = id }
            ok(id)
            emit(.docContent(doc: id, path: path, text: doc.text, mtime: 1))
            // The agent keeps writing its plan: an update arrives while it is open.
            if doc.info.kind == .plan, let update = content.planUpdate {
                DispatchQueue.global().asyncAfter(deadline: .now() + 2) { [weak self] in
                    guard let self, self.lock.withLock({ self.openDoc }) == id else { return }
                    self.emit(.docContent(doc: id, path: path, text: doc.text + update, mtime: 2))
                }
            }
        case .newTask(let id, let repo, let branch, _):
            // A launch takes a moment on a real host too.
            DispatchQueue.global().asyncAfter(deadline: .now() + 1) { [weak self] in
                self?.startTask(id: id, repo: repo, branch: branch)
            }
        case .changes(let id, _):
            emit(.changeList(ChangeListing(changes: id, root: content.root, entries: content.changes.map(\.info))))
            ok(id)
        case .diff(let id, _, let path):
            guard let change = content.changes.first(where: { $0.info.path == path }) else {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(path) is not changed"), sessions: nil))
                return
            }
            lock.withLock { openDiff = id }
            ok(id)
            emit(.diffContent(diff: id, path: path, text: change.diff, binary: change.diff == nil))
            // The agent keeps editing: the diff grows while it is open.
            if let diff = change.diff, let update = content.diffUpdate {
                DispatchQueue.global().asyncAfter(deadline: .now() + 2) { [weak self] in
                    guard let self, self.lock.withLock({ self.openDiff }) == id else { return }
                    self.emit(.diffContent(diff: id, path: path, text: diff + update, binary: false))
                }
            }
        case .undiff(let id):
            lock.withLock { openDiff = nil }
            ok(id)
        case .closeTask(let id, let session, let mode):
            closeTask(id: id, session: session, mode: mode)
        case .undoc(let id):
            lock.withLock { openDoc = nil }
            ok(id)
        case .keys(let id, let pane, let keys):
            ok(id)
            if pane == Self.chatPane, keys == ["Enter"], let text = lock.withLock({ prompt.take() }) {
                answer(text)
            }
            if pane == Self.chatPane { pick(keys) }
            for key in keys {
                switch key {
                case "Enter": echo(pane, Data("\r\n$ ".utf8))
                case "BSpace": echo(pane, Data("\u{8} \u{8}".utf8))
                default: echo(pane, Data("\u{1b}[7m<\(key)>\u{1b}[0m".utf8))
                }
            }
        case .bye:
            out.finish()
        default:
            ok(req.id)
        }
    }

    func close() async { out.finish() }

    private func startTask(id: UInt64, repo: String, branch: String) {
        guard branch != Self.failingBranch else {
            emit(.result(id: id, error: ErrorBody(
                code: .taskFailed, message: "could not create a worktree for \(branch): a branch named '\(branch)' already exists"),
                sessions: nil))
            return
        }
        let name = branch.replacingOccurrences(of: "/", with: "-")
        lock.withLock {
            guard !sessions.contains(where: { $0.name == name }) else { return }
            let n = 20 + sessions.count
            let pane = Pane(id: "%\(n)", window: "@\(n)", index: 0, role: .agent, cols: 100, rows: 30,
                            command: "claude", path: repo, dead: false, shellForeground: false)
            let repoName = repo.split(separator: "/").last.map(String.init)
            sessions.append(Session(id: "$\(n)", name: name, repo: repoName, repoDir: repo, branch: branch, panes: [pane]))
        }
        emit(.taskCreated(task: id, session: name))
        emit(.result(id: id, error: nil, sessions: nil))
        emit(.sessionsChanged)
    }

    private func agentWaits(_ session: String) {
        lock.withLock {
            guard let i = sessions.firstIndex(where: { $0.id == session }), !sessions[i].panes.isEmpty else { return }
            sessions[i].panes[0].agent = Agent(state: .waiting, since: UInt64(Date().timeIntervalSince1970))
        }
        emit(.sessionsChanged)
    }

    /// fix-login is not merged, so only keep and force close it.
    private func closeTask(id: UInt64, session: String, mode: CloseMode) {
        let found = lock.withLock { sessions.first { $0.id == session } }
        guard let found, found.worktree != nil else {
            emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(session) is not a task"), sessions: nil))
            return
        }
        if found.name == "fix-login", mode == .close {
            emit(.result(id: id, error: ErrorBody(
                code: .taskFailed,
                message: "fix-login is not merged. Close it and keep the worktree, or remove the worktree anyway."),
                sessions: nil))
            return
        }
        lock.withLock { sessions.removeAll { $0.id == session } }
        emit(.result(id: id, error: nil, sessions: nil))
        emit(.sessionsChanged)
    }

    private func ok(_ id: UInt64) { emit(.result(id: id, error: nil, sessions: nil)) }

    /// The demo agent: echoes a prompt. `/wait` makes it wait on a menu it cannot
    /// name, and `/allow`, `/ask` and `/plan` on a permission, a question and a plan.
    private func answer(_ text: String) {
        let n = lock.withLock { () -> Int in
            chat?.count += 1
            return chat?.count ?? 0
        }
        let user = ChatEntry(id: "u:demo\(n)", kind: .user, text: text)
        if text == "/wait" {
            emitChat(entries: [user], status: .waiting)
        } else if let (prompt, tool) = Self.demoPrompt(text, n: n) {
            lock.withLock { waitingOn = (prompt, tool) }
            emitChat(entries: [user, tool], status: .waiting, prompt: prompt)
        } else {
            let reply = ChatEntry(id: "a:demo\(n)", kind: .assistant, text: "You said: **\(text)**")
            emitChat(entries: [user, reply], status: .idle)
        }
    }

    static func demoPrompt(_ text: String, n: Int) -> (ChatPrompt, ChatEntry)? {
        let entry = "t:demo\(n)"
        switch text {
        case "/allow":
            return (ChatPrompt(entry: entry, kind: .permission, options: [
                .init(label: "Allow", keys: ["1"]), .init(label: "Deny", keys: ["Escape"], cancel: true),
            ]), ChatEntry(id: entry, kind: .tool, tool: ChatTool(name: "Bash", summary: "rm -rf build")))
        case "/ask":
            return (ChatPrompt(entry: entry, kind: .question, question: "Which format should the config use?", options: [
                .init(label: "TOML", keys: ["1"]), .init(label: "JSON", keys: ["2"]),
            ]), ChatEntry(id: entry, kind: .tool, tool: ChatTool(name: "AskUserQuestion", summary: "Which format should the config use?")))
        case "/plan":
            return (ChatPrompt(entry: entry, kind: .plan, options: [
                .init(label: "Keep planning", keys: ["Escape"], cancel: true),
            ]), ChatEntry(id: entry, kind: .tool, tool: ChatTool(name: "ExitPlanMode", summary: "Reader plan")))
        default:
            return nil
        }
    }

    /// Keys that pick an option of the waiting prompt answer it.
    private func pick(_ keys: [String]) {
        guard let (prompt, tool) = lock.withLock({ waitingOn }),
            let option = prompt.options.first(where: { $0.keys == keys })
        else { return }
        lock.withLock { waitingOn = nil }
        var done = tool
        done.tool?.result = option.cancel ? "Declined" : "Chose \(option.label)"
        done.tool?.error = option.cancel
        emitChat(entries: [done], status: .idle)
    }

    private func emitChat(reset: Bool = false, entries: [ChatEntry], status: AgentState, prompt: ChatPrompt? = nil) {
        guard let (id, seq) = lock.withLock({ () -> (UInt64, UInt64)? in
            guard chat != nil else { return nil }
            chat?.seq += 1
            return chat.map { ($0.id, $0.seq) }
        }) else { return }
        let since = UInt64(Date().timeIntervalSince1970)
        emit(.chatEntries(ChatUpdate(
            chat: id, seq: seq, reset: reset, entries: entries, status: Agent(state: status, since: since),
            agent: reset ? "claude-code" : nil, prompt: prompt)))
    }

    static let diffUpdate = "+    // checked by the agent\n"

    static let changes: [(info: ChangeInfo, diff: String?)] = [
        (ChangeInfo(path: "src/parser.rs", status: .modified, added: 2, removed: 1), """
            diff --git a/src/parser.rs b/src/parser.rs
            index 3b18e51..a9c2f04 100644
            --- a/src/parser.rs
            +++ b/src/parser.rs
            @@ -10,7 +10,8 @@ use crate::item::Item;
             /// Parses a list of items.
             pub fn parse(input: &str) -> Option<Vec<Item>> {
            -    if input.is_empty() { return None; }
            +    if input.is_empty() {
            +        return Some(Vec::new());
            +    }
                 let items = split(input)?;
                 Some(items)
             }

            """),
        (ChangeInfo(path: "docs/notes.md", status: .untracked, added: 3, removed: 0), """
            diff --git a/docs/notes.md b/docs/notes.md
            new file mode 100644
            --- /dev/null
            +++ b/docs/notes.md
            @@ -0,0 +1,3 @@
            +# Notes
            +
            +Empty input is a valid, empty list.

            """),
        (ChangeInfo(path: "assets/logo.png", status: .added), nil),
    ]

    static let planUpdate = "\n## Update\n\nUpdated by the agent while you read.\n"

    static let docs: [(info: DocInfo, text: String)] = [
        (DocInfo(path: "plans/demo-plan.md", kind: .plan, title: "Reader plan", mtime: 1), """
            # Reader plan

            ## Context

            Plans and designs are read on the iPad. \(String(repeating: "Long context keeps this page scrolling. ", count: 30))

            ## Phases

            1. List the documents.
            2. Render them with diagrams.
            3. Follow changes.

            ## Risks

            \(String(repeating: "A long section about risks. ", count: 40))
            """),
        (DocInfo(path: "README.md", kind: .repo, mtime: 1), """
            # App

            | Command | Use |
            |---|---|
            | `make check` | every check |
            | `make sandbox` | a test server |

            See the [design](docs/design.md), or the [website](https://example.com).
            """),
        (DocInfo(path: "docs/design.md", kind: .repo, mtime: 1), """
            # Design

            ```mermaid
            flowchart LR
              iPad[iPad app] --> Bridge[muxa bridge] --> Tmux[tmux server]
            ```

            ```swift
            let answer = 42
            ```
            """),
    ]

    static let conversation: [ChatEntry] = [
        ChatEntry(id: "u:1", kind: .user, text: "Fix the failing test in the parser"),
        ChatEntry(id: "a:1", kind: .assistant, text: "I'll run the tests first to see what fails."),
        ChatEntry(id: "t:1", kind: .tool, tool: ChatTool(
            name: "Bash", summary: "cargo test -p parser", result: "test parses_empty ... FAILED", error: true)),
        ChatEntry(id: "a:2", kind: .assistant, text: """
            `parses_empty` expects an empty list, but the parser returns `None`:

            ```rust
            pub fn parse(input: &str) -> Option<Vec<Item>> {
                if input.is_empty() { return None; }
            ```

            I'll return an empty list instead.
            """),
        ChatEntry(id: "t:2", kind: .tool, tool: ChatTool(name: "Edit", summary: "src/parser.rs", result: "Updated 1 line.")),
        ChatEntry(id: "n:1", kind: .notice, text: "/model opus"),
    ]

    private func echo(_ pane: String, _ data: Data) {
        let w = lock.withLock { watch }
        emit(.output(watch: w, pane: pane, data: data))
    }

    private func emit(_ m: Message) {
        if let line = try? Wire.encode(m) { out.yield(line) }
    }

    /// A frame that shows whether the grid is exact: corners, a ruler and the last row.
    static func screen(for p: Pane) -> Data {
        let cols = Int(p.cols), rows = Int(p.rows)
        var s = "\u{1b}c"
        if freshShells.contains(p.id) {
            s += "\u{1b}[1;1H\u{1b}[7m" + "\(p.command) — pane \(p.id) \(cols)x\(rows)".padding(toLength: cols, withPad: " ", startingAt: 0)
            s += "\u{1b}[0m\u{1b}[2;1H$ "
            return Data(s.utf8)
        }
        let ruler = (0..<cols).map { String($0 % 10) }.joined()
        s += "\u{1b}[1;1H\u{1b}[36m" + ruler + "\u{1b}[0m"
        s += "\u{1b}[2;1H\u{1b}[1m\(p.command) — pane \(p.id) \(cols)x\(rows)\u{1b}[0m"
        s += "\u{1b}[4;1H日本語 ✓ é  \u{1b}[31mred\u{1b}[32m green\u{1b}[34m blue\u{1b}[0m"
        s += "\u{1b}[\(rows);1H\u{1b}[7m" + String(repeating: " ", count: cols - 1) + "\u{1b}[0m"
        s += "\u{1b}[\(rows);\(cols)HX"
        // A tall pane keeps its prompt near the bottom, like an agent's composer.
        s += "\u{1b}[\(rows > 50 ? rows - 1 : 6);1H$ "
        return Data(s.utf8)
    }
}

private extension Optional {
    /// The value, leaving nil behind.
    mutating func take() -> Wrapped? {
        defer { self = nil }
        return self
    }
}
#endif
