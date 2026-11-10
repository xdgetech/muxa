import Foundation
import MuxaProtocol
import MuxaSession

/// Policies, Today's order, Not today, Forget and hypotheses (ADR 0050) on a host with
/// `work` level 11. A level 10 host keeps Today as it was.
extension AppModel {
    var usesPolicies: Bool { usesPlans && session.canUsePolicies }

    /// Each returns the host's message when it refused, else nil.
    func addPolicy(_ draft: PolicyDraft) async -> String? {
        await attempt {
            try await session.addPolicy(text: draft.sentText, scope: draft.scope, from: draft.from, until: draft.until)
        }
    }

    /// Sends only the fields the user changed.
    func editPolicy(_ policy: Policy, _ draft: PolicyDraft) async -> String? {
        let text = draft.sentText == policy.text ? nil : draft.sentText
        let scope = draft.scope == policy.scope ? nil : draft.scope
        let from = draft.from == policy.from ? nil : draft.from
        let until = draft.until == policy.until ? nil : draft.until
        guard text != nil || scope != nil || from != nil || until != nil else { return nil }
        return await attempt { try await session.editPolicy(policy, text: text, scope: scope, from: from, until: until) }
    }

    func deletePolicy(_ policy: Policy) async -> String? {
        await attempt { try await session.deletePolicy(policy) }
    }

    func forget(_ record: String) async -> String? {
        await attempt { try await session.forget(record) }
    }

    /// Moves Today's row at `source` to `destination`, as a list's move gives them. A
    /// refusal shows once, as Later's does.
    func moveToday(_ plan: Plan, from source: IndexSet, to destination: Int) {
        var order = session.todayRows(of: plan).compactMap(\.rowId)
        order.move(fromOffsets: source, toOffset: destination)
        Task { laterFailure = await attempt { try await session.reorderToday(order) } }
    }

    /// Moves a row one place up (`-1`) or down (`1`).
    func moveToday(_ plan: Plan, row: String, by step: Int) {
        let order = session.todayRows(of: plan).compactMap(\.rowId)
        guard let i = order.firstIndex(of: row), order.indices.contains(i + step) else { return }
        moveToday(plan, from: IndexSet(integer: i), to: step > 0 ? i + step + 1 : i + step)
    }

    func notToday(_ row: String, reason: String) {
        Task { laterFailure = await attempt { try await session.dismissFromToday(row, reason: reason) } }
    }

    /// Confirm on a hypothesis: the host keeps its text as a policy.
    func confirm(_ item: InboxItem) async -> String? {
        await attempt { try await session.confirm(item) }
    }

    /// Reject with the user's optional reason; a level 10 host gets none.
    func reject(_ item: InboxItem, reason: String) async -> String? {
        await attempt { try await session.reject(item, reason: reason) }
    }

    /// A Reject that takes a reason: on the overall assistant's proposals and hypotheses,
    /// and on a write (ADR 0052 §2).
    func rejectTakesReason(_ item: InboxItem) -> Bool {
        usesPolicies && [.sort, .delegate, .hypothesis, .write].contains(item.kind)
    }

    private func attempt(_ body: () async throws -> some Any) async -> String? {
        do {
            _ = try await body()
            return nil
        } catch {
            return PaneSession.describe(error)
        }
    }

    // MARK: Words

    /// "All workspaces" or the workspace's name.
    func scopeShown(_ scope: String) -> String { scope == "all" ? "All workspaces" : scope }

    /// The title of a correction's row, or its id when nothing lists it.
    func rowTitle(_ id: String) -> String {
        if let work = workItem(id) { return work.title }
        if let item = item(id) { return item.question }
        return id
    }

    /// What the host saw, in the user's terms.
    func shown(_ correction: Correction) -> String {
        let row = correction.row.map { "“\(rowTitle($0))”" } ?? "Today"
        let what = switch correction.kind {
        case .reordered: "You reordered Today"
        case .dismissed: "Not today: \(row)"
        case .rejected: "You rejected \(row)"
        case .pinned: "You pinned \(row)"
        case .unpinned: "You unpinned \(row)"
        case .deferred: "Later: \(row)"
        case .other(let raw): "\(raw): \(row)"
        }
        return correction.reason.map { "\(what). “\($0)”" } ?? what
    }

    /// A cited record's text from the host's lists; "forgotten" when it has none.
    func shown(_ cited: CitedRecord) -> String {
        switch cited {
        case .policy(let policy): policy.text
        case .hypothesis(let hypothesis): hypothesis.text
        case .correction(let correction): shown(correction)
        case .forgotten: "forgotten"
        }
    }
}

extension CitedRecord {
    /// A hypothesis or a correction; a policy is deleted, never forgotten.
    var isForgettable: Bool {
        switch self {
        case .hypothesis, .correction: true
        case .policy, .forgotten: false
        }
    }
}

extension Policy {
    /// "Until Sun, Nov 15", "From … until …" or "No end".
    var period: String {
        let style = Date.FormatStyle(date: .abbreviated, time: .omitted)
        let date = { (at: UInt64) in Date(timeIntervalSince1970: TimeInterval(at)).formatted(style) }
        switch (from, until) {
        case (let from?, let until?): return "From \(date(from)) until \(date(until))"
        case (let from?, nil): return "From \(date(from))"
        case (nil, let until?): return "Until \(date(until))"
        case (nil, nil): return "No end"
        }
    }
}

extension Correction {
    /// "Based on 3 corrections": a hypothesis's evidence.
    static func basedOn(_ evidence: [String]) -> String {
        evidence.count == 1 ? "Based on 1 correction" : "Based on \(evidence.count) corrections"
    }
}

extension Confidence {
    var shown: String {
        switch self {
        case .low: "Low confidence"
        case .medium: "Medium confidence"
        case .other(let raw): raw
        }
    }
}

/// The Policy sheet's fields. "This week" ends on Sunday at 23:59:59 here, as
/// `muxa-cli policy add --until week` does.
struct PolicyDraft: Equatable {
    enum End: Hashable { case none, thisWeek, until }

    var text = ""
    var scope = "all"
    var startsLater = false
    var start = Date()
    var end = End.none
    var endDate = Date().addingTimeInterval(7 * 86_400)

    init() {}

    init(_ policy: Policy) {
        text = policy.text
        scope = policy.scope
        if let from = policy.from {
            startsLater = true
            start = Date(timeIntervalSince1970: TimeInterval(from))
        }
        if let until = policy.until {
            end = .until
            endDate = Date(timeIntervalSince1970: TimeInterval(until))
        }
    }

    /// One paragraph: line breaks become spaces.
    var sentText: String {
        text.components(separatedBy: .newlines).joined(separator: " ").trimmingCharacters(in: .whitespaces)
    }

    var from: UInt64? { startsLater ? UInt64(start.timeIntervalSince1970) : nil }

    var until: UInt64? {
        switch end {
        case .none: nil
        case .thisWeek: UInt64(PaneSession.endOfWeek(Date()).timeIntervalSince1970)
        case .until: UInt64(endDate.timeIntervalSince1970)
        }
    }

    /// Why Save is off, or nil. The host cannot clear a start or an end yet.
    func problem(editing policy: Policy?) -> String? {
        if let problem = PaneSession.policyTextProblem(sentText) { return problem }
        if let policy, policy.until != nil, end == .none {
            return "The host cannot remove an end yet. Delete the policy and add it again."
        }
        if let policy, policy.from != nil, !startsLater {
            return "The host cannot remove a start yet. Delete the policy and add it again."
        }
        if let until, let from, until <= from { return "The end must come after the start." }
        return nil
    }
}
