import Foundation
import MuxaProtocol

/// The demo's Task proposals (`work` level 18, ADR 0060), with the host's checks: Start Task
/// starts one Task per proposal, linked to the Work item; Send on a `new-task` item reaches
/// the secretary and leaves it open; a plan's approve is `conflict` when the menu changed.
extension DemoWork {
    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// Start Task: `approve` without a hand-off, `start-task` beside one. A second answer, or
    /// one on another item with the same proposal, returns the same Task (`made` false).
    mutating func startTask(
        _ id: String, _ action: InboxAction, tasks: inout DemoTasks
    ) throws(Refusal) -> (record: TaskRecord, item: InboxItem, made: Bool) {
        guard let i = taskItems.firstIndex(where: { $0.id == id }), let proposal = taskItems[i].proposedTask else {
            throw Refusal(.badRequest, "\(id) offers no Task")
        }
        let marker = Self.marker(taskItems[i], proposal)
        if let task = startedTasks[marker], let record = tasks.record(task) {
            return (record, answered(i, action, task: record), false)
        }
        guard taskItems[i].state == .open, taskItems[i].answers.contains(action) else {
            throw Refusal(.badRequest, "\(action.rawValue) is not an answer of \(id)")
        }
        let work = proposal.work ?? taskItems[i].result?.work
        let record: TaskRecord
        do throws(DemoTasks.Refusal) {
            record = try tasks.create(workspace: proposal.workspace, title: proposal.title,
                                      goal: "\(proposal.goal)\n\nPlan first.\nmuxa-task-proposal: \(marker)",
                                      project: nil, repo: nil, branch: nil, loop: nil, launched: true)
        } catch {
            throw Refusal(.conflict, "the Task for \(id) cannot start")
        }
        let made = tasks.attach(record.id, worker: "claude", work: work)
        if let work, let w = assistant.work.firstIndex(where: { $0.id == work }) {
            assistant.work[w].tasks.append(made.id)
        }
        startedTasks[marker] = made.id
        return (made, answered(i, action, task: made), true)
    }

    private mutating func answered(_ i: Int, _ action: InboxAction, task: TaskRecord) -> InboxItem {
        if taskItems[i].state == .open {
            taskItems[i].state = .answered
            taskItems[i].answer = action
            taskItems[i].answeredAt = Self.now
            taskItems[i].answers = []
            taskItems[i].task = task.id
            taskItems[i].summary = "Started Task \(task.id): \(task.title)\n\n\(taskItems[i].summary ?? "")"
        }
        return taskItems[i]
    }

    /// Per proposal, as the host marks it: a result's request, version and step, else the item.
    private static func marker(_ item: InboxItem, _ proposal: TaskProposal) -> String {
        if let r = item.result { return "\(r.request) v\(r.version) s\(proposal.step ?? r.step)" }
        return "\(item.id) \(DemoPlans.hash(proposal.title + proposal.goal).prefix(12))"
    }

    /// As a level 17 host keeps it: no proposals on Results, no `new-task` items, and no
    /// `start-task`.
    mutating func forgetProposals() {
        let gone = Set(taskItems.filter { $0.kind == .newTask }.map(\.id))
        taskItems.removeAll { gone.contains($0.id) }
        for p in plans.plans.indices { plans.plans[p].top.removeAll { $0.item.map(gone.contains) ?? false } }
        for i in taskItems.indices where taskItems[i].result?.task != nil {
            taskItems[i].result?.task = nil
            taskItems[i].answers.removeAll { $0 == .startTask }
            if taskItems[i].result?.handoff == nil { taskItems[i].answers.removeAll { $0 == .approve } }
        }
    }

    /// The researcher's Result `item` (with its hand-off) also proposes a Task, and so does
    /// `plain`; the secretary proposes `newTask` for `work`, which Today lists; `plan` is the
    /// plan menu of the Task in `session`.
    func proposing(
        item: String, plain: String, newTask: String, plan: String, work: String, session: (id: String, pane: String, task: String)
    ) -> DemoWork {
        var w = self
        if let i = w.taskItems.firstIndex(where: { $0.id == item }) {
            w.taskItems[i].result?.task = TaskProposal(
                workspace: "work", repo: "app", title: "Add the provider B client",
                goal: "Add a client for provider B's e-invoicing API behind the billing interface, with tests.",
                step: 2, worker: DemoStaff.researcher)
            w.taskItems[i].answers = [.approve, .startTask, .reply, .dismiss, .later]
        }
        if let i = w.taskItems.firstIndex(where: { $0.id == plain }) {
            w.taskItems[i].result?.task = TaskProposal(
                workspace: "work", title: "Keep the old invoice numbers",
                goal: "Make the migration keep the old provider's invoice number sequence.", step: 1)
            w.taskItems[i].question = "The assistant proposes a Task: Keep the old invoice numbers"
            w.taskItems[i].answers = [.approve, .reply, .dismiss, .later]
        }
        let goal = "The Inbox crashes when a result has no summary. Find the cause and fix it, with a test."
        w.taskItems.insert(InboxItem(
            id: newTask, kind: .newTask, state: .open,
            question: "The secretary proposes a Task in work: Fix the Inbox crash on an empty result",
            workspace: "work", title: "Fix the Inbox crash on an empty result", createdAt: Self.now - 240, summary: goal,
            newTask: TaskProposal(workspace: "work", title: "Fix the Inbox crash on an empty result", goal: goal, work: work),
            answers: [.approve, .reply, .dismiss, .later]), at: 0)
        w.taskItems.insert(InboxItem(
            id: plan, kind: .ask, state: .open, question: "Approve the plan: keep the redirect after sign-in",
            workspace: "work", title: "Fix the login redirect", createdAt: Self.now - 30, task: session.task,
            session: session.id, pane: session.pane,
            prompt: ChatPrompt(entry: "e-plan01", kind: .plan, question: "Would you like to proceed?", options: [
                ChatPrompt.Option(label: "Yes, and use auto mode", keys: ["1"]),
                ChatPrompt.Option(label: "Yes, manually approve edits", keys: ["2"]),
                ChatPrompt.Option(label: "No, keep planning", keys: ["Escape"], cancel: true),
            ]),
            answers: [.approve, .reject, .reply]), at: 0)
        if !w.plans.plans.isEmpty {
            w.plans.plans[0].top.append(PlanRow(
                item: newTask, why: "The secretary proposes a Task for the Inbox crash.",
                next: "Read the goal, then start the Task or tell the secretary what to change.",
                minutes: Minutes(low: 2, high: 5, estimated: 3)))
        }
        return w
    }

    /// The UI tests' `-demo-proposals`: `-demo-staff` with proposals. i-0000f004 (the
    /// researcher's Result, with its hand-off to the writer) and i-0000f007 propose Tasks;
    /// the secretary's i-0000f009 is Today's last row; i-0000f00a is the plan of t-login1.
    static var testsProposing: DemoWork {
        testsStaffed.proposing(item: "i-0000f004", plain: "i-0000f007", newTask: "i-0000f009", plan: "i-0000f00a",
                               work: "w-0000b005", session: ("$4", "%5", "t-login1"))
    }
}

extension DemoBridge {
    /// The plan menu changed since the item: approve is refused (UI tests' `-plan-menu-changed`).
    private static var planMenuChanged: Bool {
        #if DEBUG
        ProcessInfo.processInfo.arguments.contains("-plan-menu-changed")
        #else
        false
        #endif
    }

    /// At level 18: `start-task`, `approve` on an item whose Start Task it is, and `reply` on a
    /// `new-task` item; and `approve` on a plan.
    func answersProposal(_ id: String, _ action: InboxAction) -> Bool {
        guard Self.workLevel >= workTaskProposals else { return false }
        return lock.withLock {
            guard let item = work.taskItems.first(where: { $0.id == id }) else { return false }
            return action == item.startsTask || (action == .reply && item.kind == .newTask)
                || (action == .approve && item.isPlan)
        }
    }

    func answerProposal(id: UInt64, item: String, action: InboxAction, text: String?) {
        let found = lock.withLock { work.taskItems.first { $0.id == item } }
        guard let found else {
            return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "\(item) is not an item"), sessions: nil))
        }
        if found.isPlan {
            guard !Self.planMenuChanged else {
                return emit(.result(id: id, error: ErrorBody(
                    code: .conflict, message: "the pane shows no line \"Yes, manually approve edits\""), sessions: nil))
            }
            return answerItem(id: id, item: item, action: action, text: text, option: nil, worktree: nil)
        }
        if action == .reply { return sendToSecretary(id: id, item: found, text: text) }
        let outcome = lock.withLock { () -> Result<(TaskRecord, InboxItem, Bool), DemoWork.Refusal> in
            do throws(DemoWork.Refusal) {
                let (record, saved, made) = try work.startTask(item, action, tasks: &tasks)
                if made { sessions.append(Self.taskSession(record, number: 20 + sessions.count)) }
                return .success((record, saved, made))
            } catch { return .failure(error) }
        }
        switch outcome {
        case .success(let (record, saved, made)):
            emit(.taskSaved(task: id, record: record))
            emit(.inboxSaved(inbox: id, entry: nil, item: saved))
            ok(id)
            if made {
                emit(.sessionsChanged)
                emit(.tasksChanged)
            }
        case .failure(let refusal):
            emit(.result(id: id, error: refusal.body, sessions: nil))
        }
    }

    /// Send on a `new-task` item: the secretary's session gets the proposal and the text; the
    /// item stays open.
    private func sendToSecretary(id: UInt64, item: InboxItem, text: String?) {
        guard let text, !text.isEmpty else {
            return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "reply needs a text"), sessions: nil))
        }
        let (session, started) = lock.withLock { rootSession(of: DemoPlans.overall) }
        emit(.sessionStarted(start: id, session: session.id, name: session.name))
        emit(.inboxSaved(inbox: id, entry: nil, item: item))
        ok(id)
        if started { emit(.sessionsChanged) }
    }
}
