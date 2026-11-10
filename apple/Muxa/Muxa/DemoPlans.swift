import CryptoKit
import Foundation
import MuxaProtocol

/// The demo's overall assistant (`work` level 10, ADR 0049), kept in memory with the
/// host's checks: one plan made at a time, approve once, and notes written only while
/// the live file is still the one the run read. A plan never changes a Work item.
struct DemoPlans: Sendable {
    /// Newest first, at most 7.
    var plans: [Plan] = []
    var assistant: AssistantInfo?
    /// SHA-256 of each live notes file, by `notesFile`: a workspace's `NOTES.md`, or a
    /// staff member's own.
    var liveNotes: [String: String] = [:]
    /// What approving each proposal made, by item id: a second approve returns it.
    var made: [String: Made] = [:]
    /// When the pending plan's run started.
    var started: Date?

    struct Made: Sendable {
        var capture: Capture?
        var work: WorkItem?
        var request: AssistantRequest?
    }

    /// Seconds a plan run takes before its plan is stored.
    static let planAfter: TimeInterval = 3
    static let kept = 7

    static func id(_ n: Int) -> String { String(format: "p-%08x", 0xe000 + n) }

    static func notesFile(_ workspace: String, member: String? = nil) -> String {
        member.map { "\(workspace)/\($0)" } ?? workspace
    }

    static func hash(_ text: String) -> String {
        SHA256.hash(data: Data(text.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    /// The UI tests' `-demo-notes-changed`: the live notes changed since every run read them.
    static let notesChanged: Bool = {
        #if DEBUG
        ProcessInfo.processInfo.arguments.contains("-demo-notes-changed")
        #else
        false
        #endif
    }()

    /// Pinned Work items first, in pin order; then dated ones, soonest first; then an
    /// agent's question. A Work item moved down with Later is never a row.
    static func rows(work: [WorkItem], items: [InboxItem], now: UInt64) -> [PlanRow] {
        let open = work.filter { $0.state != .done && $0.state != .withdrawn && $0.laterAt == nil }
        let pinned = open.filter { $0.pinnedAt != nil }.sorted { ($0.pinnedAt ?? 0) < ($1.pinnedAt ?? 0) }
        let dated = open.filter { $0.pinnedAt == nil && $0.due != nil }.sorted { ($0.due?.at ?? 0) < ($1.due?.at ?? 0) }
        var rows = pinned.map { pinnedRow($0, now: now) } + dated.map { dueRow($0, now: now) }
        if let ask = items.first(where: { $0.state == .open && $0.laterAt == nil && $0.kind == .ask }) {
            rows.append(PlanRow(
                item: ask.id, why: "An agent waits on your answer.", next: "Answer it in the Inbox.",
                minutes: Minutes(low: 1, high: 5, estimated: 2),
                basis: [Basis(kind: .dependency, text: "\(ask.title) waits on it", source: ask.task)]))
        }
        return Array(rows.prefix(3))
    }

    private static func pinnedRow(_ item: WorkItem, now: UInt64) -> PlanRow {
        var basis = [Basis(kind: .manual, text: "Pinned by you")]
        if let due = item.due { basis.append(Basis(kind: .due, text: "Due \(inWords(due.at, now: now))", at: due.at)) }
        return PlanRow(work: item.id, why: "Pinned by you.", next: "Pick up where the last session stopped.",
                       minutes: Minutes(low: 60, high: 120, estimated: 90), basis: basis)
    }

    private static func dueRow(_ item: WorkItem, now: UInt64) -> PlanRow {
        let at = item.due?.at ?? now
        return PlanRow(work: item.id, why: "Due \(inWords(at, now: now)).", next: "Read the assistant's result and decide.",
                       minutes: Minutes(low: 30, high: 60, estimated: 45),
                       basis: [Basis(kind: .due, text: "Due \(inWords(at, now: now))", at: at)],
                       missing: ["How long the review takes"])
    }

    private static func inWords(_ at: UInt64, now: UInt64) -> String {
        let hours = at > now ? (at - now) / 3600 : 0
        return hours < 24 ? "in \(hours) hours" : "in \(hours / 24) days"
    }

    /// The rows that entered or moved since the previous plan, in row order, then the ones that left.
    static func changes(_ rows: [PlanRow], since previous: [PlanRow]) -> [PlanChange] {
        let before = previous.map(\.target)
        let now = rows.enumerated().compactMap { i, row -> PlanChange? in
            guard let j = before.firstIndex(of: row.target) else { return PlanChange(id: row.target, change: .entered) }
            return j == i ? nil : PlanChange(id: row.target, change: .moved)
        }
        let left = before.filter { b in !rows.contains { $0.target == b } }.map { PlanChange(id: $0, change: .left) }
        return now + left
    }
}

/// What approving a `sort`, `delegate` or `handoff` item did.
enum DemoProposalOutcome {
    case delegated(WorkItem, AssistantRequest, DemoAssistant.RunChange, InboxItem)
    case sorted(Capture, WorkItem?, InboxItem)
    /// The Capture was sorted meanwhile: the item is resolved, and the answer is this refusal.
    case resolved(InboxItem, DemoWork.Refusal)
}

/// What `apply-notes` wrote.
enum DemoNotesApplied {
    case request(WorkItem, AssistantRequest)
    case plan(Plan)
}

extension DemoWork {
    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// The plans as `inbox-list` sends them: only the newest resulted one carries the
    /// derived `unanswered` and `stale`, which describe now.
    var listedPlans: [Plan] {
        var listed = plans.plans
        guard let i = listed.firstIndex(where: { $0.delivery == .resulted }) else { return listed }
        let open = assistant.shownWork { _ in false }.filter { $0.state != .done && $0.state != .withdrawn }
        let requests = assistant.listedRequests(of: open)
        listed[i].unanswered = requests.filter { r in
            !r.results.contains { $0.version == r.version } && r.delivery != .cancelled && r.delivery != .cancelRequested
        }.map(\.id)
        listed[i].stale = requests.filter { r in
            guard let newest = r.results.first(where: { !$0.stale }), let item = open.first(where: { $0.id == r.work }) else {
                return false
            }
            return newest.at < item.updatedAt
        }.map(\.id)
        return listed
    }

    /// `plan-now`: a pending plan, stored after `planAfter`.
    mutating func planNow(minutes: UInt32?) throws(Refusal) -> Plan {
        guard plans.assistant?.overall != nil else {
            throw Refusal(.unsupported, "no workspace sets [workspace.assistant] overall = true in workspaces.toml on the host")
        }
        if DemoAssistant.daemonOff { throw Refusal(.unsupported, DemoAssistant.noDaemon) }
        if let open = plans.plans.first, open.delivery == .pending {
            throw Refusal(.conflict, "plan \(open.id) is being made")
        }
        if let minutes, !(1...1440).contains(minutes) { throw Refusal(.badRequest, "minutes must be 1 to 1440") }
        let plan = Plan(id: DemoPlans.id(plans.plans.count + 1), rev: 1, at: Self.now, inputsAt: Self.now,
                        delivery: .pending, deliveryAt: Self.now, minutes: minutes, workspaces: ["work"])
        plans.plans = Array(([plan] + plans.plans).prefix(DemoPlans.kept))
        plans.started = Date()
        return plan
    }

    /// Stores the pending plan once its run had time. True when it did.
    mutating func finishPlan(at date: Date = Date()) -> Bool {
        guard let pending = plans.plans.first, pending.delivery == .pending, let started = plans.started,
              date.timeIntervalSince(started) >= DemoPlans.planAfter
        else { return false }
        plans.plans[0] = madePlan(id: pending.id, rev: pending.rev + 1, at: pending.at, minutes: pending.minutes)
        plans.started = nil
        return true
    }

    /// A resulted plan from the Work items and Inbox items as they are now.
    func madePlan(id: String, rev: UInt64, at: UInt64, minutes: UInt32?) -> Plan {
        let now = Self.now
        let shown = assistant.shownWork { _ in false }
        let rows = DemoPlans.rows(work: shown, items: shownItems, now: now)
        let dueSoon = shown.filter { item in
            item.state != .done && item.state != .withdrawn && item.laterAt != nil
                && (item.due.map { $0.at <= now + 86_400 } ?? false)
        }
        .sorted { ($0.due?.at ?? 0) < ($1.due?.at ?? 0) }
        let previous = plans.plans.first { $0.delivery == .resulted && $0.id != id }?.top ?? []
        return Plan(id: id, rev: rev, at: at, inputsAt: now, delivery: .resulted, deliveryAt: now, minutes: minutes,
                    workspaces: ["work"], top: rows, dueSoon: dueSoon.map(\.id),
                    changed: DemoPlans.changes(rows, since: previous))
    }

    /// `apply-notes`: writes only while the live file still hashes to `base`. `step` (level
    /// 12) picks the step's Result, else the version's newest; a member other than the lead
    /// has its own file.
    mutating func applyNotes(source: String, version: UInt64, base: String, step: UInt32? = nil) throws(Refusal)
        -> DemoNotesApplied {
        if source.hasPrefix("p-") {
            guard let plan = plans.plans.first(where: { $0.id == source }), let notes = plan.notes,
                  let overall = plans.assistant?.overall
            else { throw Refusal(.badRequest, "\(source) has no notes") }
            guard plan.rev == version else { throw Refusal(.conflict, "\(source) is at rev \(plan.rev)") }
            try writeNotes(notes, base: base, in: overall)
            return .plan(plan)
        }
        guard let request = assistant.requests.first(where: { $0.id == source }),
              let result = request.results.first(where: { $0.version == version && (step == nil || ($0.step ?? 1) == step) }),
              let notes = result.notes,
              let item = assistant.work.first(where: { $0.id == request.work })
        else { throw Refusal(.badRequest, "\(source) v\(version) has no notes") }
        let member = result.worker.flatMap { name in assistant.staff[request.workspace]?.first { $0.worker == name } }
        if let member, !member.lead {
            try writeNotes(notes, base: base, in: request.workspace, member: member.worker)
        } else {
            try writeNotes(notes, base: base, in: request.workspace)
        }
        return .request(item, request)
    }

    /// The workspace's `NOTES.md`, or a staff member's own notes file.
    private mutating func writeNotes(_ notes: Notes, base: String, in workspace: String, member: String? = nil) throws(Refusal) {
        let file = DemoPlans.notesFile(workspace, member: member)
        guard notes.base == base, plans.liveNotes[file] == base else {
            throw Refusal(.conflict, "the notes of \(file) changed since the run read them")
        }
        plans.liveNotes[file] = DemoPlans.hash(notes.text)
        if let member, let i = assistant.staff[workspace]?.firstIndex(where: { $0.worker == member }) {
            assistant.staff[workspace]?[i].notesAt = Self.now
        } else if let i = plans.assistant?.delegates.firstIndex(where: { $0.workspace == workspace }) {
            plans.assistant?.delegates[i].notesAt = Self.now
        }
    }

    /// Approve on a `sort`, `delegate` or `handoff` item: runs its action once. A second
    /// approve returns what the first made.
    mutating func approveProposal(_ id: String) throws(Refusal) -> DemoProposalOutcome {
        guard let i = taskItems.firstIndex(where: { $0.id == id }) else { throw Refusal(.badRequest, "\(id) is not an item") }
        if taskItems[i].answer == .approve, let made = plans.made[id] {
            if let request = made.request, let work = assistant.work.first(where: { $0.id == request.work }) {
                let current = assistant.requests.first { $0.id == request.id } ?? request
                return .delegated(work, current, DemoAssistant.RunChange(), taskItems[i])
            }
            if let capture = made.capture { return .sorted(capture, made.work, taskItems[i]) }
        }
        guard taskItems[i].state == .open, taskItems[i].answers.contains(.approve) else {
            throw Refusal(.badRequest, "approve is not an answer of \(id)")
        }
        if let h = taskItems[i].handoff {
            let (item, request, change) = try assistant.handOn(h)
            plans.made[id] = DemoPlans.Made(work: item, request: request)
            return .delegated(item, request, change, answered(i))
        }
        if let d = taskItems[i].delegate {
            guard let w = assistant.work.first(where: { $0.id == d.work }) else { throw Refusal(.badRequest, "no Work item \(d.work)") }
            let (item, request, change) = try assistant.ask(d.work, rev: w.rev, scope: d.scope, workspace: d.workspace,
                                                            purpose: d.purpose, worker: d.worker, by: .overall)
            plans.made[id] = DemoPlans.Made(work: item, request: request)
            return .delegated(item, request, change, answered(i))
        }
        guard let s = taskItems[i].sort else { throw Refusal(.badRequest, "\(id) proposes nothing") }
        guard let c = assistant.captures.first(where: { $0.id == s.capture }) else {
            throw Refusal(.badRequest, "no Capture \(s.capture)")
        }
        guard c.state == .received else {
            taskItems[i].state = .resolved
            taskItems[i].summary = "\(c.id) was sorted meanwhile."
            taskItems[i].answers = []
            return .resolved(taskItems[i], Refusal(.conflict, "\(c.id) is sorted"))
        }
        let (capture, item) = try assistant.sort(s.capture, rev: c.rev, action: s.action, into: s.work, title: s.title,
                                                 workspace: s.workspace)
        plans.made[id] = DemoPlans.Made(capture: capture, work: item)
        return .sorted(capture, item, answered(i))
    }

    private mutating func answered(_ i: Int) -> InboxItem {
        taskItems[i].state = .answered
        taskItems[i].answer = .approve
        taskItems[i].answeredAt = Self.now
        taskItems[i].answers = []
        return taskItems[i]
    }

    // MARK: Fixtures

    /// The overall assistant's part of a fixture: its proposals as items, a pinned and
    /// a dated Work item, an item moved down that is due soon, a Result with notes, and
    /// the first plan.
    func planned(
        proposals: [InboxItem], extra: [WorkItem], pinned: String, dueIn: [String: UInt64], notesOn request: String
    ) -> DemoWork {
        let now = Self.now
        var w = self
        w.taskItems = proposals + w.taskItems
        w.assistant.work += extra
        for i in w.assistant.work.indices {
            let id = w.assistant.work[i].id
            if id == pinned { w.assistant.work[i].pinnedAt = now - 3600 }
            if let seconds = dueIn[id] {
                w.assistant.work[i].due = Due(at: now + seconds, timezone: TimeZone.current.identifier)
            }
        }
        let live = DemoPlans.notesNow
        if let r = w.assistant.requests.firstIndex(where: { $0.id == request }), !w.assistant.requests[r].results.isEmpty {
            w.assistant.requests[r].results[0].notes = Notes(text: DemoPlans.proposedNotes, base: DemoPlans.hash(live))
        }
        w.plans = DemoPlans(
            assistant: AssistantInfo(overall: DemoPlans.overall, delegates: [
                DelegateInfo(workspace: "work", share: .summaries, overallRequests: .review, notesAt: now - 86_400),
                DelegateInfo(workspace: "personal", share: .outline, overallRequests: .none),
            ]),
            liveNotes: ["work": DemoPlans.hash(DemoPlans.notesChanged ? live + "- Edited by hand.\n" : live)])
        w.plans.plans = [w.madePlan(id: DemoPlans.id(1), rev: 1, at: now - 1800, minutes: nil)]
        return w
    }

    static func proposal(
        _ id: String, _ question: String, ago: UInt64, sort: SortProposal? = nil, delegate: DelegateProposal? = nil
    ) -> InboxItem {
        InboxItem(id: id, kind: delegate == nil ? .sort : .delegate, state: .open, question: question,
                  workspace: DemoPlans.overall, title: "Overall assistant", createdAt: now - ago,
                  sort: sort, delegate: delegate, answers: [.approve, .reject, .later])
    }

    /// The UI tests' `-demo-plans`: the tests' fixtures with the overall assistant.
    static var testsPlanned: DemoWork {
        tests.planned(
            proposals: [
                proposal("i-0000f001", "Ask work's assistant about passkey browser support", ago: 60, delegate: DelegateProposal(
                    work: "w-0000b002", workspace: "work", scope: .research,
                    purpose: "Which browsers support the conditional UI for passkeys?",
                    reason: "Pinned, and the Safari flag is unclear.")),
                proposal("i-0000f002", "Make a Work item: Fix the flaky CI job", ago: 120, sort: SortProposal(
                    capture: "c-0000c001", action: .work, workspace: "work", title: "Fix the flaky CI job")),
            ],
            extra: [DemoAssistant.item("w-0000b004", "File the expense report", ago: 7200, workspace: "personal",
                                       laterAgo: 500, dueIn: 5 * 3600)],
            pinned: "w-0000b002", dueIn: ["w-0000b001": 2 * 86_400], notesOn: "q-0000d001")
    }

    static var showcasePlanned: DemoWork {
        showcaseBase.planned(
            proposals: [
                proposal("i-5c00f001", "Ask work's assistant which Stripe events refunds need", ago: 300,
                         delegate: DelegateProposal(
                            work: "w-5c00b001", workspace: "work", scope: .research,
                            purpose: "Which Stripe events must the webhook handle for refunds and credit notes?",
                            reason: "Due in two days, and refunds are not covered yet.")),
                proposal("i-5c00f002", "Make a Work item: EU invoice format", ago: 600, sort: SortProposal(
                    capture: "c-5c00c001", action: .work, workspace: "work", title: "EU invoice format for the VAT run")),
            ],
            extra: [DemoAssistant.item("w-5c00b004", "Renew the TLS certificate", ago: 172_800, workspace: "personal",
                                       laterAgo: 3000, dueIn: 6 * 3600)],
            pinned: "w-5c00b002", dueIn: ["w-5c00b001": 2 * 86_400], notesOn: "q-5c00d001")
    }
}

extension DemoPlans {
    /// The overall assistant's workspace: it takes no Tasks, Captures or requests.
    static let overall = "assistant"

    static let notesNow = "# work\n\n- Billing runs on the old provider until the migration.\n"
    static let proposedNotes = "# work\n\n- Billing runs on the old provider until the migration.\n"
        + "- Invoice numbers come from the old provider's sequence: keep it.\n- Finance picks the switch month.\n"
}

extension DemoTasks {
    /// With the overall assistant's workspace, which pickers leave out.
    func planned() -> DemoTasks {
        var t = self
        t.workspaces.append(WorkspaceInfo(name: DemoPlans.overall))
        return t
    }
}
