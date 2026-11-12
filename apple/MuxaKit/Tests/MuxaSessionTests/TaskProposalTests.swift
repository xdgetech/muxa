import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with Task proposals (`work` level 18, ADR 0060), as the host answers: Start Task
/// sends `task-saved` before `inbox-saved`, Send on a `new-task` item reaches the secretary
/// and leaves it open, and a plan's approve is `conflict` unless the pane shows the option.
final class ProposalBridge: PlanBridge, @unchecked Sendable {
    /// The plan menu shows "Yes, manually approve edits".
    var showsApproveOption = true
    /// Tasks started, by proposal title: a second answer returns the same Task.
    private(set) var started: [String: TaskRecord] = [:]

    override init() {
        super.init()
        items = [TaskProposalTests.proposing, TaskProposalTests.besideHandoff, TaskProposalTests.secretarys,
                 TaskProposalTests.plan]
    }

    override func send(_ line: Data) async throws {
        guard case .updateInboxEntry(let id, let entry, let action, _, let text, _, _, _) = try Wire.decodeRequest(line),
              let i = items.firstIndex(where: { $0.id == entry }),
              items[i].proposedTask != nil || items[i].isPlan
        else { return try await super.send(line) }
        guard items[i].answers.contains(action) else {
            return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "not an answer"), sessions: nil))
        }
        switch action {
        case .approve where items[i].isPlan:
            guard showsApproveOption else {
                return emit(.result(id: id, error: ErrorBody(code: .conflict, message: "no such option"), sessions: nil))
            }
        case .approve, .startTask:
            guard let proposal = items[i].proposedTask, action == .startTask || items[i].result?.handoff == nil else {
                return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "hand-off"), sessions: nil))
            }
            let record = started[proposal.title] ?? TaskRecord(
                id: String(format: "t-%06x", 0x7f3a2c + started.count), rev: 1, workspace: proposal.workspace,
                title: proposal.title, goal: proposal.goal, status: .open, createdAt: 9, updatedAt: 9,
                work: proposal.work ?? items[i].result?.work)
            started[proposal.title] = record
            items[i].task = record.id
            emit(.taskSaved(task: id, record: record))
        case .reply where items[i].kind == .newTask:
            guard let text, !text.isEmpty else {
                return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "reply needs a text"), sessions: nil))
            }
            emit(.sessionStarted(start: id, session: "$9", name: "ask-assistant"))
            emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
            return emit(.result(id: id, error: nil, sessions: nil))
        default:
            break
        }
        items[i].state = .answered
        items[i].answer = action
        items[i].answers = []
        emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
        emit(.result(id: id, error: nil, sessions: nil))
    }
}

@MainActor
struct TaskProposalTests {
    nonisolated static let task = TaskProposal(
        workspace: "work", repo: "app", title: "Fix the crash", goal: "Guard the empty summary.", step: 1,
        worker: "researcher")

    nonisolated static let proposing = InboxItem(
        id: "i-0000000a", kind: .result, state: .open, question: "researcher proposes a Task: Fix the crash",
        workspace: "work", title: "Crash", createdAt: 8, summary: "Found it.",
        result: ResultReview(request: "q-00000001", version: 1, step: 1, work: "w-00000001", worker: "researcher",
                             task: task),
        answers: [.approve, .reply, .dismiss, .later])

    nonisolated static let besideHandoff = InboxItem(
        id: "i-0000000b", kind: .result, state: .open, question: "researcher asks writer to: Check it.",
        workspace: "work", title: "Crash", createdAt: 8, summary: "Found it.",
        result: ResultReview(request: "q-00000002", version: 1, step: 1, work: "w-00000001", worker: "researcher",
                             handoff: Handoff(worker: "writer", purpose: "Check it."),
                             task: TaskProposal(workspace: "work", title: "Fix it twice", goal: "G", step: 1)),
        answers: [.approve, .startTask, .reply, .dismiss, .later])

    nonisolated static let secretarys = InboxItem(
        id: "i-0000000c", kind: .newTask, state: .open, question: "The secretary proposes a Task in work: Tidy",
        workspace: "work", title: "Tidy", createdAt: 8, summary: "Tidy the docs.",
        newTask: TaskProposal(workspace: "work", title: "Tidy", goal: "Tidy the docs.", work: "w-00000001"),
        answers: [.approve, .reply, .dismiss, .later])

    nonisolated static let plan = InboxItem(
        id: "i-0000000d", kind: .ask, state: .open, question: "Claude Code has a plan", workspace: "work",
        title: "Fix the crash", createdAt: 8, task: "t-7f3a2c", session: "$1", pane: "%1",
        prompt: ChatPrompt(entry: "e1", kind: .plan, options: [
            ChatPrompt.Option(label: "Yes, manually approve edits", keys: ["2"]),
            ChatPrompt.Option(label: "No, keep planning", keys: ["Escape"], cancel: true),
        ]),
        answers: [.approve, .reject, .reply])

    func connected(_ bridge: ProposalBridge, level: String = "work:18") async throws -> PaneSession {
        try await PlanTests().connected(bridge, level: level)
    }

    @Test func startTaskIsTheMainAnswerAndNamed() {
        #expect(Self.proposing.startsTask == .approve)
        #expect(InboxAction.approve.title(for: Self.proposing) == "Start Task")
        #expect(ItemAnswers(Self.proposing).main == .approve)

        let beside = ItemAnswers(Self.besideHandoff)
        #expect(Self.besideHandoff.startsTask == .startTask)
        #expect(beside.decisions == [.approve, .startTask] && beside.main == .startTask)
        #expect(InboxAction.approve.title(for: Self.besideHandoff) == "Hand off to writer")
        #expect(InboxAction.startTask.title(for: Self.besideHandoff) == "Start Task")

        #expect(Self.secretarys.proposedTask?.work == "w-00000001" && Self.secretarys.startsTask == .approve)
        #expect(InboxAction.approve.title(for: Self.secretarys) == "Start Task")
        #expect(Self.secretarys.todayVerb == "Start Task")
    }

    @Test func aPlanItemApprovesThePlanOrKeepsPlanning() {
        #expect(Self.plan.isPlan && Self.plan.proposedTask == nil)
        #expect(InboxAction.approve.title(for: Self.plan) == "Approve plan")
        #expect(InboxAction.reject.title(for: Self.plan) == "Keep planning")
        #expect(InboxAction.reply.title(for: Self.plan) == "Keep planning & Send")
        #expect(Self.plan.todaySend?.label == "Keep planning & send to the Task's agent…")
    }

    @Test func startTaskStartsOneTaskLinkedToTheWorkItem() async throws {
        let bridge = ProposalBridge()
        let session = try await connected(bridge)
        #expect(session.proposesTasks)
        let saved = try await session.answer(Self.proposing, .approve)
        #expect(saved.state == .answered && saved.task == "t-7f3a2c")
        #expect(bridge.started["Fix the crash"]?.work == "w-00000001")
        #expect(session.savedTasks.isEmpty && session.savedItems.isEmpty)
    }

    @Test func startTaskBesideAHandoffSendsStartTask() async throws {
        let bridge = ProposalBridge()
        let session = try await connected(bridge)
        let saved = try await session.answer(Self.besideHandoff, .startTask)
        #expect(saved.answer == .startTask && saved.task != nil)
        #expect(bridge.started["Fix it twice"] != nil)
    }

    @Test func sendOnANewTaskItemKeepsItOpen() async throws {
        let session = try await connected(ProposalBridge())
        let saved = try await session.answer(Self.secretarys, .reply, text: "Use the docs repo.")
        #expect(saved.state == .open)
        #expect(session.startedSessions.isEmpty)
    }

    /// The host presses the option only when the pane shows it; else the plan is approved
    /// in the terminal, and nothing reached the pane.
    @Test func aPlanApprovedWhereThePaneShowsNoOptionSaysToUseTheTerminal() async throws {
        let bridge = ProposalBridge()
        bridge.showsApproveOption = false
        let session = try await connected(bridge)
        let conflict = PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: PaneSession.planInTerminal))
        await #expect(throws: conflict) { try await session.answer(Self.plan, .approve) }
        #expect(session.inboxItems.first { $0.id == Self.plan.id }?.state == .open)
        bridge.showsApproveOption = true
        #expect(try await session.answer(Self.plan, .approve).answer == .approve)
    }
}
