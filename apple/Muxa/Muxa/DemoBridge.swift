import Foundation
import MuxaProtocol
import MuxaTransport
import UIKit

/// An in-app bridge: a few sessions, typed text echoed back, and a chat for the agent
/// pane %1. Most panes have a fixed size; the content's `followers`, and new tasks,
/// take the app's viewport as a host with `window-size latest` does (ADR 0020).
/// No SSH, no tmux. Try the Demo uses it, and so do the UI tests (launch argument
/// `-demo`, DEBUG only).
final class DemoBridge: BridgeChannel, @unchecked Sendable {
    let lines: AsyncThrowingStream<Data, any Error>
    private let out: AsyncThrowingStream<Data, any Error>.Continuation
    let lock = NSLock()
    private var watch: UInt64 = 0
    private var watchedPane: String?
    /// The app's grid at its default font, once it sent one.
    private var viewport: (cols: UInt16, rows: UInt16)?
    /// Panes whose window follows the device in use.
    var followers: Set<String>
    private var chat: (id: UInt64, seq: UInt64, count: Int)?
    private var openDoc: UInt64?
    private var openDiff: UInt64?
    /// The last paste into %1, sent as a prompt when Enter follows.
    private var prompt: String?
    /// What the demo agent waits on, and the tool call it waits on.
    private var waitingOn: (prompt: ChatPrompt, tool: ChatEntry)?
    /// Texts the Inbox sent to the chat's agent, after its conversation.
    private var itemTexts: [ChatEntry] = []
    /// Grows when a task starts.
    var sessions: [Session]
    /// Grows when a task record is created.
    private var tasks: DemoTasks
    /// Routines, stored Inbox entries (ADR 0026), Captures and Work items (ADR 0046).
    var work: DemoWork
    /// Set until the first watch, when the alert's clock starts.
    private var alertsPending: Bool
    private let content: DemoContent
    static let chatPane = "%1"
    /// A branch whose task fails, to show the error.
    static let failingBranch = "taken"

    static let initialSessions = [
        Session(id: "$1", name: "feature-auth", repo: "app", repoDir: "/src/app", branch: "feat/auth",
                worktree: "/src/.worktrees/app-feat-auth", taskId: "t-auth01", panes: [
            Pane(id: "%1", window: "@1", index: 0, role: .agent, cols: 100, rows: 30, command: "claude",
                 path: "/work", dead: false, shellForeground: false,
                 agent: Agent(state: .waiting, since: UInt64(Date().timeIntervalSince1970) - 90)),
            Pane(id: "%2", window: "@1", index: 1, role: .shell, cols: 100, rows: 30, command: "zsh",
                 path: "/work", dead: false, shellForeground: true),
        ]),
        Session(id: "$4", name: "fix-login", repo: "app", repoDir: "/src/app", branch: "fix/login",
                worktree: "/src/.worktrees/app-fix-login", taskId: "t-login1", panes: [
            Pane(id: "%5", window: "@4", index: 0, role: .agent, cols: 100, rows: 30, command: "claude",
                 path: "/work", dead: false, shellForeground: false,
                 agent: Agent(state: .idle, since: UInt64(Date().timeIntervalSince1970) - 600)),
        ]),
        Session(id: "$5", name: "docs-site", repo: "docs", repoDir: "/src/docs", branch: "main", taskId: "t-docs01", panes: [
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
        // Made detached at tmux's default 80x24; its window follows the device (ADR 0020).
        Session(id: "$7", name: "device", panes: [
            Pane(id: "%9", window: "@7", index: 0, role: .shell, cols: 80, rows: 24, command: "zsh",
                 path: "/tmp", dead: false, shellForeground: true),
        ]),
    ]
    /// The panes of `initialSessions` whose window follows the device in use.
    static let initialFollowers: Set<String> = ["%9"]
    /// Shells with nothing run yet: a title bar on the first row and the prompt under it.
    static let freshShells: Set<String> = ["%8"]

    /// With `alerts`, an agent elsewhere starts waiting a few seconds in.
    init(content: DemoContent, alerts: Bool) {
        self.content = content
        sessions = content.sessions + content.work.sessions
        followers = content.followers
        tasks = content.tasks
        work = content.work
        work.offersAskAgent = Self.workLevel >= workAskAgent
        alertsPending = alerts
        (lines, out) = AsyncThrowingStream<Data, any Error>.makeStream()
        emit(.hello(protocol: protocolVersion, bridge: "demo", tmux: "demo", capabilities: Self.capabilities))
    }

    /// Lines of earlier output a demo pane has.
    private static let historyLines = 60

    private static var capabilities: [String] {
        let all = [
            "chat", "docs", "tasks", "changes", "close-task", "images", "branch-changes", "task-records", "task-handoff",
            "task-loop", "tasks-changed", "viewport", "history", "work:9",
        ]
        #if DEBUG
        let arguments = ProcessInfo.processInfo.arguments
        // A bridge from before ask sessions and branches named after the title (ADR 0031).
        if arguments.contains("-work1") { return all.map { $0 == "work:9" ? "work:1" : $0 } }
        // A bridge from before Inbox items (ADR 0033): the app has no Inbox for it.
        if arguments.contains("-work2") { return all.map { $0 == "work:9" ? "work:2" : $0 } }
        // A bridge from before closing ask sessions (ADR 0039).
        if arguments.contains("-work3") { return all.map { $0 == "work:9" ? "work:3" : $0 } }
        // A bridge from before Captures (ADR 0046): `+` is New Task.
        if arguments.contains("-work7") { return all.map { $0 == "work:9" ? "work:7" : $0 } }
        // A bridge from before the overall assistant (ADR 0047): no Today, pins or notes.
        if arguments.contains("-work8") { return all.map { $0 == "work:9" ? "work:8" : $0 } }
        // A bridge from before task records, for the old New Task sheet's test.
        if arguments.contains("-no-task-records") { return all.filter { $0 != "task-records" } }
        #endif
        return all
    }

    func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        switch req {
        case .list(let id):
            emit(.result(id: id, error: nil, sessions: lock.withLock { sessions }))
        case .history(let id, let pane, _):
            // Earlier output above the pane's screen, one line per `\n`, as the bridge sends it.
            let text = (1...Self.historyLines).map { "Earlier output, line \($0)\n" }.joined()
            emit(.history(history: id, pane: pane, cols: UInt16(sized(pane)?.cols ?? 80), data: Data(text.utf8), truncated: false))
            ok(id)
        case .watch(let id, let pane):
            // The alert comes a few seconds after the first pane shows, so a phone's
            // first Inbox does not hide it.
            let startAlert = lock.withLock { () -> Bool in
                watch = id
                watchedPane = pane
                defer { alertsPending = false }
                return alertsPending
            }
            if startAlert {
                DispatchQueue.global().asyncAfter(deadline: .now() + content.alertDelay) { [weak self] in self?.agentWaits("$4") }
            }
            emit(.result(id: id, error: nil, sessions: nil))
            guard let p = sized(pane) else { return }
            emit(.screen(watch: id, pane: pane, cols: p.cols, rows: p.rows, data: content.screen(p)))
        case .viewport(let id, let cols, let rows):
            ok(id)
            let (w, pane) = lock.withLock { () -> (UInt64, String?) in
                viewport = (UInt16(clamping: cols), UInt16(clamping: rows))
                return (watch, watchedPane)
            }
            guard let pane, let p = sized(pane), lock.withLock({ followers.contains(pane) }) else { return }
            emit(.screen(watch: w, pane: pane, cols: p.cols, rows: p.rows, data: content.screen(p)))
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
            emitChat(reset: true, entries: content.conversation + lock.withLock { itemTexts }, status: content.chatStatus)
        case .unchat(let id):
            lock.withLock { chat = nil }
            ok(id)
        case .docs(let id, let pane):
            emit(.docList(DocListing(docs: id, root: content.root, entries: docs(pane).map(\.info))))
            ok(id)
        case .doc(let id, let pane, let path):
            guard let doc = docs(pane).first(where: { $0.info.path == path }) else {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(path) is not listed"), sessions: nil))
                return
            }
            lock.withLock { openDoc = id }
            ok(id)
            emit(.docContent(doc: id, path: path, text: doc.text, mtime: doc.info.mtime))
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
        case .listTasks(let id):
            let tasks = lock.withLock { tasks }
            emit(.taskList(TaskListing(tasks: id, workspaces: tasks.workspaces, entries: tasks.records)))
            ok(id)
        case .createTask(
            let id, let workspace, let title, let goal, let project, let repo, let branch, _, let loop, let worker, let item, let rev):
            guard worker == nil else { unsupported(id); return }
            // Only a level 8 host knows Work items; an older one ignores the fields.
            let forWork = Self.workLevel >= workAssistant ? item.flatMap { w in rev.map { (w, $0) } } : nil
            DispatchQueue.global().asyncAfter(deadline: .now() + 1) { [weak self] in
                self?.createTask(
                    id: id, workspace: workspace, title: title, goal: goal, project: project, repo: repo, branch: branch, loop: loop,
                    work: forWork)
            }
        case .listRoutines, .listInbox, .runRoutine, .updateInboxEntry:
            serveWork(req)
        case .updateTask(let id, let task, let rev, let status, let worktree):
            let outcome = lock.withLock { () -> Result<TaskRecord, DemoTasks.Refusal> in
                do throws(DemoTasks.Refusal) {
                    return .success(try tasks.update(task, rev: rev, status: status, worktree: worktree))
                } catch { return .failure(error) }
            }
            answer(id, outcome)
        case .recoverRestart(let id, _, _, _, _):
            unsupported(id)
        case .applyNotes, .planNow:
            servePlans(req)
        case .capture, .sortCapture, .updateWork, .requestAssistant, .cancelRequest, .getWork:
            await serveCaptures(req)
        case .restartAgent(let id, let task, let force, let worker, let rev):
            guard worker == nil, rev == nil else { unsupported(id); return }
            restartAgent(id: id, task: task, force: force)
        case .startLoop(let id, let task, let loop, let reset):
            changeLoop(id) { (tasks: inout DemoTasks) throws(DemoTasks.Refusal) in try tasks.startLoop(task, spec: loop, reset: reset) }
        case .stopLoop(let id, let task):
            changeLoop(id) { (tasks: inout DemoTasks) throws(DemoTasks.Refusal) in try tasks.stopLoop(task) }
        case .changes(let id, _):
            emit(.changeList(ChangeListing(
                changes: id, root: content.root, entries: content.changes.map(\.info),
                branch: BranchChanges(base: "origin/main", commits: 3, entries: Self.branch.map(\.info)))))
            ok(id)
        case .diff(let id, _, let path, .branch):
            guard let change = Self.branch.first(where: { $0.info.path == path }) else {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(path) is not in the branch"), sessions: nil))
                return
            }
            lock.withLock { openDiff = id }
            ok(id)
            emit(.diffContent(diff: id, path: path, text: change.diff, binary: false))
        case .diff(let id, _, let path, _):
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
        case .images(let id, _):
            emit(.imageList(ImageListing(images: id, root: content.root, entries: Self.images.map(\.info))))
            ok(id)
        case .image(let id, _, let path):
            guard let image = Self.images.first(where: { $0.info.path == path }) else {
                emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(path) is not listed"), sessions: nil))
                return
            }
            ok(id)
            emit(.imageContent(image: id, path: path, data: Self.png(image.color), mtime: image.info.mtime))
        case .unimage(let id):
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
        case .ping(let id), .unwatch(let id), .bytes(let id, _, _):
            ok(id)
        case .startSession(let id, let workspace, let repo):
            startSession(id: id, workspace: workspace, repo: repo)
        case .bye:
            out.finish()
        }
    }

    /// The host's `work` level (ADR 0027 §5).
    static var workLevel: UInt32 { MuxaProtocol.workLevel(capabilities) }

    /// An ask session: the agent in the repo's main checkout, listed as a manual session (ADR 0031).
    private func startSession(id: UInt64, workspace: String, repo: String) {
        guard Self.workLevel >= workAsk else { return unsupported(id, Self.noRequest) }
        let outcome = lock.withLock { () -> Result<Session, DemoTasks.Refusal> in
            let dir: String
            do throws(DemoTasks.Refusal) { dir = try tasks.checkout(repo, in: workspace) } catch { return .failure(error) }
            let base = dir.split(separator: "/").last.map(String.init) ?? "repo"
            let k = sessions.filter { $0.name.hasPrefix("ask-\(base)-") }.count + 1
            let n = 20 + sessions.count
            let pane = Pane(id: "%\(n)", window: "@\(n)", index: 0, role: .agent, cols: 80, rows: 24,
                            command: "claude", path: dir, dead: false, shellForeground: false,
                            agent: Agent(state: .running, since: UInt64(Date().timeIntervalSince1970)))
            followers.insert(pane.id)
            let session = Session(
                id: "$\(n)", name: "ask-\(base)-\(k)", repo: base, repoDir: dir, branch: "main", ask: workspace, panes: [pane])
            sessions.append(session)
            return .success(session)
        }
        switch outcome {
        case .success(let session):
            emit(.sessionStarted(start: id, session: session.id, name: session.name))
            ok(id)
            emit(.sessionsChanged)
        case .failure(.badRequest(let message)):
            emit(.result(id: id, error: ErrorBody(code: .badRequest, message: message), sessions: nil))
        case .failure(.conflict):
            emit(.result(id: id, error: ErrorBody(code: .conflict, message: "conflict"), sessions: nil))
        case .failure(.wouldLoseWork(let reason)):
            emit(.result(id: id, error: ErrorBody(code: .wouldLoseWork, message: reason), sessions: nil))
        }
    }

    /// Ask Agent (ADR 0041): the workspace's one Ask session at its root, started on the
    /// first request. The item stays open.
    private func askAgent(id: UInt64, item: String, text: String?) {
        guard Self.workLevel >= workAskAgent else { return unsupported(id, Self.noRequest) }
        let found = lock.withLock { () -> (InboxItem, Session, Bool)? in
            guard let shown = work.shownItems.first(where: { $0.id == item && $0.state == .open }) else { return nil }
            let name = "ask-\(shown.workspace)"
            if let s = sessions.first(where: { $0.name == name }) { return (shown, s, false) }
            let root = tasks.workspaces.first { $0.name == shown.workspace }?.root ?? "/"
            let n = 20 + sessions.count
            let pane = Pane(id: "%\(n)", window: "@\(n)", index: 0, role: .agent, cols: 80, rows: 24,
                            command: "claude", path: root, dead: false, shellForeground: false,
                            agent: Agent(state: .running, since: UInt64(Date().timeIntervalSince1970)))
            followers.insert(pane.id)
            let session = Session(id: "$\(n)", name: name, ask: shown.workspace, panes: [pane])
            sessions.append(session)
            return (shown, session, true)
        }
        guard let (shown, session, started) = found else {
            return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(item) is not an open item"), sessions: nil))
        }
        guard let text, !text.isEmpty else {
            return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "ask-agent needs a text"), sessions: nil))
        }
        emit(.sessionStarted(start: id, session: session.id, name: session.name))
        emit(.inboxSaved(inbox: id, entry: nil, item: shown))
        ok(id)
        if started { emit(.sessionsChanged) }
    }

    /// What a bridge says to a request it does not know.
    static let noRequest = "The demo bridge does not support this request."

    /// Routines and stored entries, refused as a bridge without `work` level 1 refuses them.
    private func serveWork(_ req: Request) {
        guard Self.workLevel >= workRoutines else { return unsupported(req.id, Self.noRequest) }
        switch req {
        case .listRoutines(let id):
            emit(.routineList(routines: id, entries: lock.withLock { work.routines }))
            ok(id)
        case .listInbox(let id):
            let (entries, items) = lock.withLock { (work.shownEntries, work.shownItems) }
            let (listed, captures, requests) = listedWork()
            let (plans, assistant) = listedPlans()
            // A host below level 9 has no overall assistant, so none of its proposals.
            let shown = Self.workLevel >= workPlans ? items : items.filter { $0.kind != .sort && $0.kind != .delegate }
            emit(.inboxList(inbox: id, entries: entries, items: Self.workLevel >= workItems ? shown : [],
                            work: listed, captures: captures, requests: requests, plans: plans, assistant: assistant))
            ok(id)
        case .runRoutine(let id, let workspace, let routine):
            let outcome = lock.withLock { () -> Result<RoutineInfo, DemoWork.Refusal> in
                do throws(DemoWork.Refusal) {
                    let (saved, session) = try work.run(workspace: workspace, routine: routine, sessionNumber: 20 + sessions.count)
                    sessions.append(session)
                    return .success(saved)
                } catch { return .failure(error) }
            }
            switch outcome {
            case .success(let routine):
                emit(.routineSaved(routine: id, entry: routine))
                ok(id)
                emit(.sessionsChanged)
            case .failure(let refusal):
                emit(.result(id: id, error: refusal.body, sessions: nil))
            }
        case .updateInboxEntry(let id, let entry, .askAgent, _, let text, _, _):
            askAgent(id: id, item: entry, text: text)
        // `until` is ignored: Later moves an item down (ADR 0040).
        case .updateInboxEntry(let id, let entry, .approve, _, _, _, _) where isOverallProposal(entry):
            approveProposal(id: id, item: entry)
        case .updateInboxEntry(let id, let entry, let action, _, let text, let option, let worktree):
            let isEntry = lock.withLock { work.entries.contains { $0.id == entry } }
            if Self.workLevel >= workItems, !isEntry || action == .read {
                answerItem(id: id, item: entry, action: action, text: text, option: option, worktree: worktree)
            } else {
                // A proposal's Reject dismisses it.
                updateEntry(id: id, entry: entry, action: action == .reject ? .dismiss : action)
            }
        default:
            unsupported(req.id, Self.noRequest)
        }
    }

    /// Approve creates one Task, as `create-task` would, and opens no session for a
    /// second approval; dismiss and later touch no Task.
    private func updateEntry(id: UInt64, entry: String, action: InboxAction) {
        let outcome = lock.withLock { () -> Result<(InboxEntry, created: Bool), DemoWork.Refusal> in
            do throws(DemoWork.Refusal) {
                let (saved, goal) = try work.update(entry, action: action)
                guard let goal, saved.task == nil else { return .success((saved, false)) }
                let routine = work.routines.first { $0.workspace == saved.workspace && $0.name == saved.routine }
                let record: TaskRecord
                do throws(DemoTasks.Refusal) {
                    record = try tasks.create(workspace: saved.workspace, title: saved.title, goal: goal,
                                              project: routine?.project, repo: nil, branch: nil, loop: nil, launched: true)
                } catch {
                    throw DemoWork.Refusal(.badRequest, "\(saved.workspace) is not a workspace")
                }
                sessions.append(Self.taskSession(record, number: 20 + sessions.count))
                return .success((work.approved(entry, task: record.id), true))
            } catch { return .failure(error) }
        }
        switch outcome {
        case .success(let (saved, created)):
            let item = Self.workLevel >= workItems ? lock.withLock { work.item(for: saved) } : nil
            emit(.inboxSaved(inbox: id, entry: saved, item: item))
            ok(id)
            if created { emit(.sessionsChanged) }
        case .failure(let refusal):
            emit(.result(id: id, error: refusal.body, sessions: nil))
        }
    }

    /// Answers a Task's item, or marks any item read. The agent that takes the answer
    /// runs again; Accept and Drop set the Task's status.
    private func answerItem(
        id: UInt64, item: String, action: InboxAction, text: String?, option: UInt32?, worktree: WorktreeEnd?
    ) {
        let outcome = lock.withLock { () -> Result<(InboxItem, DemoWork.ItemEffect), DemoWork.Refusal> in
            do throws(DemoWork.Refusal) {
                let answered = try work.answer(item, action, text: text, option: option, worktree: worktree, tasks: &tasks)
                if case .agentRuns(let pane) = answered.1 {
                    for s in sessions.indices {
                        guard let p = sessions[s].panes.firstIndex(where: { $0.id == pane }) else { continue }
                        sessions[s].panes[p].agent = Agent(state: .running, since: UInt64(Date().timeIntervalSince1970))
                    }
                }
                return .success(answered)
            } catch { return .failure(error) }
        }
        switch outcome {
        case .success(let (saved, effect)):
            emit(.inboxSaved(inbox: id, entry: nil, item: saved))
            ok(id)
            if case .agentRuns(Self.chatPane) = effect, let text { sendToChat(text, from: item) }
            switch effect {
            case .agentRuns: emit(.sessionsChanged)
            case .taskChanged: emit(.tasksChanged)
            case .none: break
            }
        case .failure(let refusal):
            emit(.result(id: id, error: refusal.body, sessions: nil))
        }
    }

    /// The text of Send, Redo or Change course reaches the chat's agent as a prompt,
    /// so the Chat view shows what the Inbox sent.
    private func sendToChat(_ text: String, from item: String) {
        let entry = ChatEntry(id: "u:item-\(item)", kind: .user, text: text)
        let open = lock.withLock { () -> Bool in
            itemTexts.append(entry)
            return chat != nil
        }
        if open { emitChat(entries: [entry], status: .running) }
    }

    /// A running agent in its own session for a new task record.
    private static func taskSession(_ record: TaskRecord, number n: Int) -> Session {
        let pane = Pane(id: "%\(n)", window: "@\(n)", index: 0, role: .agent, cols: 80, rows: 24,
                        command: "claude", path: record.dir ?? "/", dead: false, shellForeground: false,
                        agent: Agent(state: .running, since: UInt64(Date().timeIntervalSince1970)))
        return Session(id: "$\(n)", name: record.session ?? record.id, taskId: record.id, panes: [pane])
    }

    func close() async { out.finish() }

    /// The pane, at the viewport's size when its window follows the device.
    private func sized(_ id: String) -> Pane? {
        lock.withLock {
            for s in sessions.indices {
                guard let i = sessions[s].panes.firstIndex(where: { $0.id == id }) else { continue }
                if followers.contains(id), let viewport {
                    sessions[s].panes[i].cols = viewport.cols
                    sessions[s].panes[i].rows = viewport.rows
                }
                return sessions[s].panes[i]
            }
            return nil
        }
    }

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
            let pane = Pane(id: "%\(n)", window: "@\(n)", index: 0, role: .agent, cols: 80, rows: 24,
                            command: "claude", path: repo, dead: false, shellForeground: false)
            followers.insert(pane.id)
            let repoName = repo.split(separator: "/").last.map(String.init)
            sessions.append(Session(id: "$\(n)", name: name, repo: repoName, repoDir: repo, branch: branch, panes: [pane]))
        }
        emit(.taskCreated(task: id, session: name))
        emit(.result(id: id, error: nil, sessions: nil))
        emit(.sessionsChanged)
    }

    /// Records the task, then starts a session for it, unless its branch is taken:
    /// then the record is kept with a note, as a real bridge does.
    private func createTask(
        id: UInt64, workspace: String, title: String, goal: String, project: String?, repo: String?, branch: String?,
        loop: LoopSpec?, work forWork: (id: String, rev: UInt64)? = nil
    ) {
        let launched = branch != Self.failingBranch
        let outcome = lock.withLock { () -> Result<TaskRecord, DemoTasks.Refusal> in
            // One Task per Work item `rev`: asking again returns it (ADR 0046 §3).
            if let forWork, let made = work.assistant.task(for: forWork.id, rev: forWork.rev), let record = tasks.record(made) {
                return .success(record)
            }
            let record: TaskRecord
            do throws(DemoTasks.Refusal) {
                record = try tasks.create(workspace: workspace, title: title, goal: goal, project: project, repo: repo,
                                          branch: branch, loop: loop, launched: launched,
                                          branchFromTitle: Self.workLevel >= workAsk)
            } catch {
                return .failure(error)
            }
            if let name = record.session {
                let n = 20 + sessions.count
                let pane = Pane(id: "%\(n)", window: "@\(n)", index: 0, role: .agent, cols: 80, rows: 24,
                                command: "claude", path: record.dir ?? "/", dead: false, shellForeground: false,
                                agent: Agent(state: .running, since: UInt64(Date().timeIntervalSince1970)))
                followers.insert(pane.id)
                sessions.append(Session(
                    id: "$\(n)", name: name, repo: repo?.split(separator: "/").last.map(String.init), repoDir: repo,
                    branch: branch, worktree: repo == nil ? nil : record.dir, taskId: record.id, panes: [pane]))
            }
            if let forWork { work.assistant.taskStarted(record.id, for: forWork.id, rev: forWork.rev) }
            return .success(record)
        }
        guard case .success(let record) = outcome, !launched else {
            answer(id, outcome)
            if case .success = outcome { emit(.sessionsChanged) }
            return
        }
        emit(.taskSaved(task: id, record: record))
        emit(.result(id: id, error: ErrorBody(code: .taskFailed, message: record.note ?? "the task did not start"), sessions: nil))
    }

    /// `task-saved` and the result, or the refusal.
    private func answer(_ id: UInt64, _ outcome: Result<TaskRecord, DemoTasks.Refusal>) {
        switch outcome {
        case .success(let record):
            emit(.taskSaved(task: id, record: record))
            ok(id)
        case .failure(.badRequest(let message)):
            emit(.result(id: id, error: ErrorBody(code: .badRequest, message: message), sessions: nil))
        case .failure(.conflict):
            emit(.result(id: id, error: ErrorBody(code: .conflict, message: "the task changed since you read it"), sessions: nil))
        case .failure(.wouldLoseWork(let reason)):
            emit(.result(id: id, error: ErrorBody(code: .wouldLoseWork, message: reason), sessions: nil))
        }
    }

    /// Saves a loop change, then sends `task-saved`, `result`, and `tasks-changed` as
    /// the host's poll would send it to every bridge (ADR 0019).
    private func changeLoop(_ id: UInt64, _ change: (inout DemoTasks) throws(DemoTasks.Refusal) -> TaskRecord) {
        let outcome = lock.withLock { () -> Result<TaskRecord, DemoTasks.Refusal> in
            do throws(DemoTasks.Refusal) { return .success(try change(&tasks)) } catch { return .failure(error) }
        }
        answer(id, outcome)
        if case .success = outcome { emit(.tasksChanged) }
    }

    /// Restarts a task's agent in its pane, with the bridge's checks (ADR 0018).
    private func restartAgent(id: UInt64, task: String, force: Bool) {
        enum Outcome {
            case saved(TaskRecord)
            case refused(ErrorBody)
        }
        let outcome = lock.withLock { () -> Outcome in
            guard tasks.record(task) != nil else {
                return .refused(ErrorBody(code: .badRequest, message: "\(task) is not a task"))
            }
            guard let s = sessions.firstIndex(where: { $0.taskId == task }),
                let p = sessions[s].panes.firstIndex(where: { $0.role == .agent && !$0.dead })
            else {
                return .refused(ErrorBody(code: .taskFailed, message: "No session runs this task's agent."))
            }
            if sessions[s].panes[p].agent?.state == .running, !force {
                return .refused(ErrorBody(code: .badRequest, message: "The agent is running. Restart with force to stop it."))
            }
            let record: TaskRecord
            do throws(DemoTasks.Refusal) {
                record = try tasks.restart(task)
            } catch {
                return .refused(ErrorBody(code: .badRequest, message: "\(task) is not a task"))
            }
            sessions[s].panes[p].agent = Agent(state: .running, since: UInt64(Date().timeIntervalSince1970))
            return .saved(record)
        }
        switch outcome {
        case .saved(let record):
            emit(.taskSaved(task: id, record: record))
            ok(id)
            emit(.sessionsChanged)
        case .refused(let error):
            emit(.result(id: id, error: error, sessions: nil))
        }
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
        // An ask session closes too (ADR 0039); `mode` changes nothing for it.
        let isAsk = Self.workLevel >= workCloseAsk && found?.ask != nil
        guard let found, found.worktree != nil || isAsk else {
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

    /// The repo's documents, and TASK.md, HANDOFF.md and, once a check ran, CHECK.md
    /// in a task's session, as the bridge lists them.
    private func docs(_ pane: String) -> [(info: DocInfo, text: String)] {
        let (session, tasks) = lock.withLock { (sessions.first { $0.panes.contains { $0.id == pane } }, tasks) }
        guard let text = tasks.taskFile(for: session?.taskId), let handoff = tasks.handoffFile(for: session?.taskId)
        else { return content.docs }
        var docs = content.docs + [
            (DocInfo(path: DemoTasks.taskFile, kind: .repo, mtime: 1), text),
            (DocInfo(path: DemoTasks.handoffFile, kind: .repo, mtime: 1), handoff),
        ]
        if let check = tasks.checkFile(for: session?.taskId) {
            docs.append((DocInfo(path: DemoTasks.checkFile, kind: .repo, mtime: check.mtime), check.text))
        }
        return docs
    }

    func unsupported(_ id: UInt64, _ message: String = "Workers are not implemented in the demo.") {
        emit(.result(id: id, error: ErrorBody(code: .unsupported, message: message), sessions: nil))
    }

    func ok(_ id: UInt64) { emit(.result(id: id, error: nil, sessions: nil)) }

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

    /// What the task branch's commits changed since it left main.
    static let branch: [(info: ChangeInfo, diff: String)] = [
        (ChangeInfo(path: "src/item.rs", status: .modified, added: 1, removed: 0), """
            diff --git a/src/item.rs b/src/item.rs
            index 1c2d3e4..5f6a7b8 100644
            --- a/src/item.rs
            +++ b/src/item.rs
            @@ -1,3 +1,4 @@
             /// One parsed item.
            +#[derive(Debug, Clone, PartialEq)]
             pub struct Item {
                 pub name: String,

            """),
    ]

    /// Screenshots of a QA run, newest first, and the repo's logo.
    static let images: [(info: ImageInfo, color: UIColor)] = {
        let now = UInt64(Date().timeIntervalSince1970)
        return [
            (ImageInfo(path: "build/screenshots/en/login.png", mtime: now - 60, size: 48_213), .systemBlue),
            (ImageInfo(path: "build/screenshots/en/home.png", mtime: now - 61, size: 51_002), .systemTeal),
            (ImageInfo(path: "build/screenshots/ja/login.png", mtime: now - 62, size: 49_870), .systemIndigo),
            (ImageInfo(path: "docs/images/logo.png", mtime: now - 86_400, size: 9_120), .systemOrange),
        ]
    }()

    static func png(_ color: UIColor) -> Data {
        UIGraphicsImageRenderer(size: CGSize(width: 1024, height: 768)).pngData { context in
            color.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 1024, height: 768))
            UIColor.white.setFill()
            context.fill(CGRect(x: 312, y: 284, width: 400, height: 200))
        }
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

            ## Steps

            \((1...40).map { "\($0). A step of the plan." }.joined(separator: "\n"))

            The end of the plan.
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

    func emit(_ m: Message) {
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
