import Foundation
import MuxaProtocol
import MuxaSession

/// The overall assistant's plan, delegate notes, pins and due dates (ADR 0049) on a
/// host with `work` level 10. A level 9 host keeps the Inbox as it was.
extension AppModel {
    /// The Inbox has Today above Decisions, and Work items can be pinned and dated.
    var usesPlans: Bool { usesWork && session.canPlan }

    /// Today shows: a plan exists, or the host has an overall assistant to ask for one.
    var showsToday: Bool {
        usesPlans && (!session.plans.isEmpty || session.assistantInfo?.overall != nil)
    }

    /// A plan newer than the one the user last saw from Today.
    var hasNewPlan: Bool {
        guard let seenPlan, seenPlan.host == currentHostID, let current = session.currentPlan?.id else { return false }
        return seenPlan.id != current
    }

    /// The first time Today shows on a host, its plan counts as seen.
    func notePlanShown() {
        guard seenPlan?.host != currentHostID else { return }
        seenPlan = (currentHostID, session.currentPlan?.id)
    }

    func markPlanSeen() {
        seenPlan = (currentHostID, session.currentPlan?.id)
    }

    /// Opens a Today row's Work item or Inbox item. Only a tap changes the selection.
    func open(_ row: PlanRow) {
        markPlanSeen()
        if let work = row.work {
            select(.work(work))
        } else if let item = row.item {
            select(.item(item))
        }
    }

    /// Asks the overall assistant for a plan now. Returns the host's message when it refused.
    func planNow(minutes: UInt32? = nil) async -> String? {
        do {
            try await session.planNow(minutes: minutes)
            return nil
        } catch {
            return PaneSession.describe(error)
        }
    }

    /// Pins or unpins a Work item: pinned ones are listed first, and fill a plan's first rows.
    func setPinned(_ item: WorkItem, _ pinned: Bool) {
        change(item, pinned ? .pin : .unpin)
    }

    /// Sets the item's due date, in `timeZone`. Only the user sets one.
    func setDue(_ item: WorkItem, _ date: Date, timeZone: TimeZone) async -> String? {
        do {
            let due = Due(at: UInt64(max(1, date.timeIntervalSince1970)), timezone: timeZone.identifier)
            try await session.update(item, .edit, due: due)
            return nil
        } catch {
            return PaneSession.describe(error)
        }
    }

    /// Writes a Result's proposed notes to the workspace's `NOTES.md`. Returns why it could not.
    func applyNotes(_ request: AssistantRequest, _ result: AssistResult) async -> String? {
        do {
            try await session.applyNotes(of: request, result: result)
            return nil
        } catch {
            return PaneSession.describe(error)
        }
    }

    /// The current plan's mark on the item's newest request: no answer yet, or a Result
    /// older than the item's last change. The host derives both.
    func requestMark(_ item: WorkItem) -> String? {
        guard usesPlans, let request = latestRequest(of: item) else { return nil }
        if session.isUnanswered(request) { return "Not answered yet" }
        if session.isStale(request) { return "Result older than the item" }
        return nil
    }

    /// The title a Today row shows: its Work item's or its Inbox item's.
    func title(of row: PlanRow) -> String {
        if let work = row.work { return workItem(work)?.title ?? work }
        if let id = row.item { return item(id)?.question ?? id }
        return ""
    }
}

extension Minutes {
    /// "30–60 min", or "45 min" when the range is one value.
    var shown: String {
        low == high ? "\(estimated) min" : "\(low)–\(high) min"
    }
}

extension Due {
    /// The date in the zone it was set in, with the zone when it is not this device's.
    var shown: String {
        let zone = TimeZone(identifier: timezone) ?? .current
        var style = Date.FormatStyle(date: .abbreviated, time: .shortened)
        style.timeZone = zone
        let date = Date(timeIntervalSince1970: TimeInterval(at)).formatted(style)
        return zone.identifier == TimeZone.current.identifier ? date : "\(date) (\(zone.identifier))"
    }
}

extension BasisKind {
    var shown: String {
        switch self {
        case .due: "Due"
        case .dependency: "Depends"
        case .goal: "Goal"
        case .time: "Time"
        case .manual: "You"
        case .policy: "Policy"
        case .hypothesis: "Hypothesis"
        case .correction: "Correction"
        case .other: "Other"
        }
    }
}

extension DelegateProposal {
    /// The delegate's name is its workspace's (ADR 0049 §8).
    var shown: String { "\(workspace)'s assistant · \(scope.shown)" }
}

extension SortAction {
    var shown: String {
        switch self {
        case .work: "Make a Work item"
        case .link: "Add to a Work item"
        case .note: "Keep as a note"
        }
    }
}
