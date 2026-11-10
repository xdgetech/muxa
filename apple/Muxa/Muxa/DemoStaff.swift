import Foundation
import MuxaProtocol

/// The demo's staff and hand-offs (`work` level 12, ADR 0051), with the host's checks: a
/// named member must be on the workspace's staff, and approve starts the next step once.
extension DemoAssistant {
    /// The member who runs step 1: `worker` when it is on the staff, else the lead. Nil
    /// for a workspace without staff, which takes no `worker`.
    func member(_ worker: String?, in workspace: String) throws(Refusal) -> String? {
        let staff = self.staff[workspace] ?? []
        guard let lead = staff.first(where: \.lead) ?? staff.first else {
            if let worker { throw Refusal(.badRequest, "workspace \"\(workspace)\" has no staff to ask \(worker)") }
            return nil
        }
        guard let worker else { return lead.worker }
        guard staff.contains(where: { $0.worker == worker }) else {
            throw Refusal(.badRequest, "\(worker) is not on the staff of \(workspace): "
                + staff.map(\.worker).joined(separator: ", "))
        }
        return worker
    }

    /// Approve on a `handoff` item: step `step + 1` of the request's current version.
    mutating func handOn(_ h: HandoffProposal) throws(Refusal) -> (WorkItem, AssistantRequest, RunChange) {
        guard let r = requests.firstIndex(where: { $0.id == h.request }),
              let w = work.firstIndex(where: { $0.id == requests[r].work })
        else { throw Refusal(.badRequest, "\(h.request) is not a request") }
        guard requests[r].version == h.version, let last = requests[r].steps.last, last.step == h.step,
              last.handoff?.worker == h.worker, last.delivery == .resulted
        else { throw Refusal(.conflict, "\(h.request) moved on since step \(h.step) asked for the hand-off") }
        let run = startRun(r)
        requests[r].steps.append(RequestStep(
            step: h.step + 1, worker: h.worker, by: .handoff, delivery: .pending,
            deliveryAt: UInt64(run.started.timeIntervalSince1970), run: run.task))
        return (work[w], requests[r], RunChange(started: run))
    }

    /// As a level 11 host keeps it: no staff, no steps, and Results that name no member.
    mutating func forgetStaff() {
        staff = [:]
        for r in requests.indices {
            requests[r].steps = []
            for k in requests[r].results.indices {
                requests[r].results[k].worker = nil
                requests[r].results[k].step = nil
            }
        }
    }
}

extension DemoWork {
    /// The assistants as `inbox-list` sends them, each delegate with its staff.
    var listedAssistant: AssistantInfo? {
        guard var info = plans.assistant else { return nil }
        for i in info.delegates.indices {
            info.delegates[i].staff = assistant.staff[info.delegates[i].workspace] ?? []
        }
        return info
    }

    /// As a level 11 host keeps it: no `handoff` items, and proposals that name no member.
    mutating func forgetStaff() {
        assistant.forgetStaff()
        taskItems.removeAll { $0.kind == .handoff }
        for i in taskItems.indices {
            guard let d = taskItems[i].delegate, let worker = d.worker else { continue }
            taskItems[i].delegate?.worker = nil
            taskItems[i].question = taskItems[i].question.replacingOccurrences(
                of: "\(worker) in \(d.workspace)", with: "\(d.workspace)'s assistant")
        }
    }
}

// MARK: Fixtures

/// The staff of the demo's `work` workspace, and a request whose chain waits on a hand-off.
enum DemoStaff {
    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    static let lead = "chief-of-staff"
    static let researcher = "researcher"
    static let writer = "writer"

    static let work = [
        StaffMember(worker: lead, description: "Takes each request first and hands it to the right member.", lead: true),
        StaffMember(worker: researcher, description: "Reads the docs, the repos and the web, and compares options with sources."),
        StaffMember(worker: writer, description: "Drafts memos and replies in the team's voice."),
    ]

    /// A staff of one: Ask offers no picker for it.
    static let personal = [
        StaffMember(worker: "home-helper", description: "Keeps the household's dates and bills.", lead: true),
    ]

    static let researcherNotes = "# researcher\n\n- EU invoice formats: compare by country, not by provider.\n"

    /// Version 1 asked the lead, who handed it to the researcher, who asks for the writer.
    /// Both steps propose notes: the lead's `NOTES.md`, the researcher's own file.
    static func chainRequest(_ id: String, work: String, run: Int, workspaceNotes: String) -> AssistantRequest {
        let asked = now - 3600
        let first = AssistResult(
            version: 1, at: asked + 600,
            summary: "Two providers fit. The researcher should compare their EU formats and prices.",
            openQuestions: ["Must the switch happen before the VAT run?"],
            notes: Notes(text: DemoPlans.proposedNotes, base: DemoPlans.hash(workspaceNotes)),
            worker: lead, step: 1)
        let second = AssistResult(
            version: 1, at: asked + 1800,
            summary: "Provider B covers every EU format we need and costs less per invoice.",
            findings: [
                Finding(text: "Provider B sends e-invoices in all 27 EU formats; A covers 19.", sources: [
                    ResultSource(url: "https://example.com/eu-invoicing", title: "EU e-invoicing formats", readAt: asked + 1500),
                ]),
            ],
            next: ["Draft the decision memo for finance."],
            notes: Notes(text: researcherNotes, base: DemoPlans.hash("")),
            worker: researcher, step: 2)
        return AssistantRequest(
            id: id, work: work, version: 1, workspace: "work", scope: .compare, delivery: .resulted,
            deliveryAt: second.at, createdAt: asked, updatedAt: second.at,
            purpose: "Which provider should send our EU invoices?", results: [second, first],
            steps: [
                RequestStep(step: 1, worker: lead, by: .lead, delivery: .resulted, deliveryAt: first.at,
                            run: String(format: "t-q%05d", run),
                            handoff: Handoff(worker: researcher, purpose: "Compare the two providers' EU formats and prices.")),
                RequestStep(step: 2, worker: researcher, by: .handoff, delivery: .resulted, deliveryAt: second.at,
                            run: String(format: "t-q%05d", run + 1),
                            handoff: Handoff(worker: writer, purpose: "Draft the decision memo for finance.")),
            ])
    }

    static func handoffItem(_ id: String, request: AssistantRequest, title: String) -> InboxItem {
        InboxItem(
            id: id, kind: .handoff, state: .open, question: "\(researcher) asks \(writer) to draft the decision memo for finance.",
            workspace: "work", title: title, createdAt: request.updatedAt,
            handoff: HandoffProposal(request: request.id, version: 1, step: 2, work: request.work, from: researcher,
                                     worker: writer, purpose: "Draft the decision memo for finance."),
            answers: [.approve, .reject, .later])
    }
}

extension DemoWork {
    /// A staff of three in `work` and of one in `personal`; a Work item whose request ran
    /// two steps and waits on a hand-off to the writer; and the plan's delegate, which
    /// names the researcher and is Today's last row.
    func staffed(work id: String, title: String, request: String, item: String, delegate: String) -> DemoWork {
        var w = self
        let chain = DemoStaff.chainRequest(request, work: id, run: 0x9000, workspaceNotes: DemoPlans.notesNow)
        w.assistant.staff = ["work": DemoStaff.work, "personal": DemoStaff.personal]
        w.assistant.work.append(DemoAssistant.item(id, title, ago: 4000, workspace: "work", requests: [request]))
        w.assistant.requests.append(chain)
        w.taskItems.insert(DemoStaff.handoffItem(item, request: chain, title: title), at: 0)
        w.plans.liveNotes[DemoPlans.notesFile("work", member: DemoStaff.researcher)] = DemoPlans.hash("")
        w.plans.liveNotes[DemoPlans.notesFile("work", member: DemoStaff.writer)] = DemoPlans.hash("")
        if let i = w.taskItems.firstIndex(where: { $0.id == delegate }) {
            w.taskItems[i].delegate?.worker = DemoStaff.researcher
            w.taskItems[i].question = w.taskItems[i].question.replacingOccurrences(
                of: "work's assistant", with: "\(DemoStaff.researcher) in work")
            if !w.plans.plans.isEmpty {
                let row = PlanRow(item: delegate, why: "The overall assistant proposes a request for you.",
                                  next: "Approve or reject it in the Inbox.", minutes: Minutes(low: 1, high: 5, estimated: 2))
                if w.plans.plans[0].top.count < 3 {
                    w.plans.plans[0].top.append(row)
                } else {
                    w.plans.plans[0].top[2] = row
                }
            }
        }
        return w
    }

    /// The UI tests' `-demo-staff`: `-demo-policies` with staff. w-0000b005's request
    /// q-0000d002 ran chief-of-staff → researcher and waits on i-0000f004 for the writer;
    /// the delegate i-0000f001 names the researcher.
    static var testsStaffed: DemoWork {
        testsRemembered.staffed(work: "w-0000b005", title: "Pick the EU invoicing provider", request: "q-0000d002",
                                item: "i-0000f004", delegate: "i-0000f001")
    }

    static var showcaseStaffed: DemoWork {
        showcaseRemembered.staffed(work: "w-5c00b005", title: "Pick the EU invoicing provider", request: "q-5c00d002",
                                   item: "i-5c00f004", delegate: "i-5c00f001")
    }
}
