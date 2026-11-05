import Foundation
import MuxaProtocol

/// The demo bridge's Inbox items (`work` level 3, ADR 0033), with the host's checks:
/// only listed answers, a text for Redo, Change course and Send, and a Later in the future.
/// A Routine's item is its stored entry, so Approve still creates one Task.
extension DemoWork {
    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// What an answer does beyond the item, for the bridge to apply.
    enum ItemEffect {
        case none
        /// The agent of this pane takes the answer and runs again.
        case agentRuns(String)
        /// The Task's status changed.
        case taskChanged
    }

    /// As `inbox-list` sends them: open ones with `ask` first and then newest, then
    /// Later ones, then answered ones. A Later whose time passed is open again.
    var shownItems: [InboxItem] {
        let all = taskItems.map(Self.reopened) + shownEntries.map(item(for:))
        let newest: (InboxItem, InboxItem) -> Bool = { $0.createdAt > $1.createdAt }
        let open = all.filter { $0.state == .open }.sorted { a, b in
            (a.kind == .ask) != (b.kind == .ask) ? a.kind == .ask : newest(a, b)
        }
        let later = all.filter { $0.state == .later }.sorted(by: newest)
        let closed = all.filter { $0.state == .answered || $0.state == .resolved }.sorted(by: newest)
        return open + later + closed.prefix(100)
    }

    /// A Routine's entry as its item.
    func item(for entry: InboxEntry) -> InboxItem {
        let kind = ItemKind(raw: entry.kind)
        let state: ItemState = switch entry.disposition {
        case "open": .open
        case "later": .later
        default: .answered
        }
        let answers: [InboxAction] = switch (state, kind) {
        case (.open, .proposal), (.later, .proposal): [.approve, .reject, .later]
        case (.open, _), (.later, _): [.dismiss, .later]
        default: []
        }
        return InboxItem(
            id: entry.id, kind: kind, state: state, question: entry.title, workspace: entry.workspace, title: entry.title,
            createdAt: entry.createdAt, readAt: entryReads[entry.id], until: entry.until,
            answer: entry.disposition == "approved" ? .approve : (state == .answered ? .dismiss : nil),
            summary: entry.body, task: entry.task, attachments: Attachments(links: entry.links ?? []),
            routine: entry.routine, proposal: entry.proposal, answers: answers)
    }

    /// Answers a Task's item. A Routine's item goes through `update(_:action:until:)`,
    /// except `read`.
    mutating func answer(
        _ id: String, _ action: InboxAction, until: UInt64?, text: String?, option: UInt32?, tasks: inout DemoTasks
    ) throws(Refusal) -> (InboxItem, ItemEffect) {
        if action == .read, let entry = entries.first(where: { $0.id == id }) {
            entryReads[id] = entryReads[id] ?? Self.now
            return (item(for: entry), .none)
        }
        guard let i = taskItems.firstIndex(where: { $0.id == id }) else { throw Refusal(.badRequest, "\(id) is not an item") }
        taskItems[i] = Self.reopened(taskItems[i])
        if action == .read {
            taskItems[i].readAt = taskItems[i].readAt ?? Self.now
            return (taskItems[i], .none)
        }
        let item = taskItems[i]
        guard item.answers.contains(action) else { throw Refusal(.badRequest, "\(action.rawValue) is not an answer of \(id)") }
        if [.redo, .changeCourse, .reply].contains(action), (text ?? "").isEmpty { throw Refusal(.badRequest, "\(action.rawValue) needs a text") }
        if action == .choose, Int(option ?? .max) >= (item.prompt?.options.count ?? 0) {
            throw Refusal(.badRequest, "option is not one of the prompt's options")
        }
        if action == .later {
            guard let until, until > Self.now, until <= Self.now + 366 * 86_400 else {
                throw Refusal(.badRequest, "until must be in the future and at most 366 days ahead")
            }
            taskItems[i].state = .later
            taskItems[i].until = until
            return (taskItems[i], .none)
        }
        let effect = try Self.effect(of: action, on: item, tasks: &tasks)
        taskItems[i].state = .answered
        taskItems[i].answer = action
        taskItems[i].answeredAt = Self.now
        taskItems[i].answers = []
        taskItems[i].until = nil
        return (taskItems[i], effect)
    }

    private static func effect(of action: InboxAction, on item: InboxItem, tasks: inout DemoTasks) throws(Refusal) -> ItemEffect {
        let status: TaskStatus? = switch action {
        case .accept: .done
        case .drop: .dropped
        default: nil
        }
        if let status, let record = tasks.record(item.task) {
            do throws(DemoTasks.Refusal) {
                _ = try tasks.update(record.id, rev: record.rev, status: status)
            } catch {
                throw Refusal(.conflict, "\(record.id) changed")
            }
            return .taskChanged
        }
        switch action {
        case .approve, .reject, .choose, .redo, .changeCourse, .reply, .retry:
            return item.pane.map(ItemEffect.agentRuns) ?? .none
        default:
            return .none
        }
    }

    private static func reopened(_ item: InboxItem) -> InboxItem {
        guard item.state == .later, (item.until ?? 0) <= now else { return item }
        var open = item
        open.state = .open
        open.until = nil
        return open
    }

    // MARK: Fixtures

    private static func taskItem(
        _ id: String, _ kind: ItemKind, _ question: String, task: String, title: String, ago: UInt64,
        session: String? = nil, pane: String? = nil, read: Bool = false, summary: String? = nil,
        prompt: ChatPrompt? = nil, attachments: Attachments = Attachments(), answers: [InboxAction]
    ) -> InboxItem {
        InboxItem(id: id, kind: kind, state: .open, question: question, workspace: "work", title: title,
                  createdAt: now - ago, readAt: read ? now - ago + 30 : nil, summary: summary, task: task,
                  session: session, pane: pane, prompt: prompt, attachments: attachments, answers: answers)
    }

    private static let permission = ChatPrompt(entry: "toolu_demo", kind: .permission, options: [
        ChatPrompt.Option(label: "Yes", keys: ["Enter"]),
        ChatPrompt.Option(label: "No", keys: ["Escape"], cancel: true),
    ])

    /// The UI tests' items: an ask with a permission, a question, a review with
    /// evidence and a failure. The proposal and the briefing are the entries.
    static let testItems: [InboxItem] = [
        taskItem("i-0000a001", .ask, "Allow Bash: npm run deploy", task: "t-auth01", title: "Add sign-in with passkeys",
                 ago: 90, session: "$1", pane: "%1", prompt: permission, answers: [.approve, .reject, .reply, .later]),
        taskItem("i-0000a004", .ask, "Which test runner should the suite use?", task: "t-perf01",
                 title: "Speed up the test suite", ago: 300,
                 prompt: ChatPrompt(entry: "toolu_demo2", kind: .question, question: "Which test runner?", options: [
                     ChatPrompt.Option(label: "Vitest", keys: ["1"]),
                     ChatPrompt.Option(label: "Jest", keys: ["2"]),
                     ChatPrompt.Option(label: "Cancel", keys: ["Escape"], cancel: true),
                 ]),
                 answers: [.choose, .later]),
        taskItem("i-0000a002", .review, "Ready for review: Fix the login redirect", task: "t-login1",
                 title: "Fix the login redirect", ago: 600, session: "$4", pane: "%5",
                 summary: "## Login redirect\n\nUsers now return to the page they came from after sign-in.\n\n"
                     + "- The flow is in docs/design.md.\n- The tests pass.\n\n"
                     + "**Ready to merge:** [PR #42](https://github.com/example/app/pull/42).",
                 attachments: Attachments(
                     changes: ChangeCount(files: 3, added: 6, removed: 1), docs: ["docs/design.md"],
                     images: ["build/screenshots/en/login.png"],
                     links: [SourceLink(url: "https://github.com/example/app/pull/42", title: "PR #42")]),
                 answers: [.accept, .reply, .redo, .changeCourse, .keepOpen, .later]),
        taskItem("i-0000a003", .failure, "The agent stopped with an error", task: "t-price1",
                 title: "Draft the pricing page", ago: 1800, read: true,
                 summary: "npm install failed: the lockfile is out of date.",
                 answers: [.retry, .reply, .changeCourse, .drop, .dismiss, .later]),
    ]

    /// Try the Demo's items: an agent asking, and two finished turns.
    static let showcaseItems: [InboxItem] = [
        taskItem("i-5c000001", .ask, "Allow Bash: stripe listen --forward-to localhost:4242", task: "t-a1b2c3",
                 title: "Handle Stripe invoice.paid webhook", ago: 75, session: "$1", pane: "%1", prompt: permission,
                 answers: [.approve, .reject, .reply, .later]),
        taskItem("i-5c000002", .review, "Ready for review: Dark mode for the dashboard", task: "t-0a1b2c",
                 title: "Dark mode for the dashboard", ago: 720, session: "$3", pane: "%5",
                 summary: "The dashboard follows the system setting now. The charts use the new palette, and the "
                     + "contrast checks pass. The setting is described in `README.md`; see "
                     + "[PR #318](https://github.com/example/dashboard/pull/318).",
                 attachments: Attachments(
                     changes: ChangeCount(files: 6, added: 212, removed: 48), docs: ["README.md"],
                     links: [SourceLink(url: "https://github.com/example/dashboard/pull/318", title: "PR #318")]),
                 answers: [.accept, .reply, .redo, .changeCourse, .keepOpen, .later]),
        taskItem("i-5c000003", .review, "Ready for review: Faster search index", task: "t-3c4d5e",
                 title: "Faster search index", ago: 1800, session: "$4", pane: "%6", read: true,
                 summary: "The index builds in 41 s instead of 96 s. Search results are unchanged.",
                 attachments: Attachments(changes: ChangeCount(files: 2, added: 37, removed: 12)),
                 answers: [.accept, .reply, .redo, .changeCourse, .keepOpen, .later]),
    ]
}
