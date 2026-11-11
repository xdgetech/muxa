import Foundation
import MuxaProtocol
import MuxaSession

/// Today rows that say what to do (ADR 0059 §7) and proposed Tasks (ADR 0060 §8), on a host
/// with `work` level 17 and 18. An older host keeps the rows and items as they were.
extension AppModel {
    /// Rows name their verb, Do and Send; Plan Now is Re-rank.
    var namesTodayActions: Bool { usesPlans && session.namesTodayActions }

    /// The verb before a row's title.
    func verb(of row: PlanRow) -> String { session.todayVerb(row) }

    /// A row's main button (ADR 0059 §3): a Task's review or menu opens its session; any
    /// other target opens where its answers and evidence are.
    func primary(of row: PlanRow) -> TodayPrimary {
        if let id = row.item, let item = item(id), item.kind == .review || item.kind == .ask, itemSession(item) != nil {
            return TodayPrimary(label: "Open Session", opensSession: item)
        }
        if row.work != nil { return TodayPrimary(label: "Open Work Item") }
        let noun = row.item.flatMap(item).map(\.kind.openedNoun) ?? "Item"
        return TodayPrimary(label: "Open \(noun)")
    }

    func perform(_ primary: TodayPrimary, on row: PlanRow) {
        markPlanSeen()
        if let item = primary.opensSession {
            open(item, as: hasChat(item) ? .chat : .terminal)
        } else {
            open(row)
        }
    }

    /// What a row's Send asks for, or nil when its target takes no text.
    func sendTarget(of row: PlanRow) -> SendTarget? {
        session.todaySend(row).map { SendTarget(send: $0, item: row.item) }
    }

    /// A Work item's Send to the secretary, from Work detail (ADR 0059 §4).
    func secretaryTarget(for work: WorkItem) -> SendTarget? {
        guard work.isOpen, let secretary = session.secretary else { return nil }
        return SendTarget(send: TodaySend(route: .secretary(work: work.id, workspace: secretary),
                                          label: "Send to the secretary (\(secretary)), with this Work item…"),
                          item: nil)
    }

    /// Sends `text` where `target` says. The secretary's and the Ask session's Chat opens;
    /// a `reply` answers the item. Returns the host's message when it refused.
    func send(_ target: SendTarget, text: String) async -> String? {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return "Write what to send first." }
        do {
            switch target.send.route {
            case .secretary(let work, _):
                let started = try await session.sendToSecretary(text, work: work)
                if let s = session.sessions.first(where: { $0.id == started.id }) { await open(s, as: .chat) }
            case .answer(let action):
                guard let id = target.item, let item = item(id) else { return "The item is no longer listed." }
                if action == .askAgent {
                    try await askAgent(item, text: text)
                } else {
                    try await session.answer(item, action, text: text)
                }
            }
            return nil
        } catch {
            return PaneSession.describe(error)
        }
    }

    /// "Re-rank (2 of 3 runs left today)" at level 17, else Plan Now.
    var rerankTitle: String { namesTodayActions ? "Re-rank" : "Plan Now" }

    // MARK: Proposed Tasks (ADR 0060)

    /// The Task an item started (a `result`, `new-task` or `write` item): its session as a
    /// Sessions row opens it, else its detail.
    func openTask(_ id: String) {
        if let s = session.sessions.first(where: { $0.taskId == id && $0.defaultPane != nil }) {
            Task { await open(s) }
        } else {
            select(.task(id))
        }
    }
}

/// A Today row's main button and what it opens.
struct TodayPrimary {
    var label: String
    /// A `review` or `ask` whose Task's session the button opens.
    var opensSession: InboxItem?
}

/// What a Send prompt sends, and to whom.
struct SendTarget: Identifiable {
    var send: TodaySend
    /// The Inbox item an `answer` route answers.
    var item: String?

    var id: String { send.label + (item ?? "") }
}

extension ItemKind {
    /// What a Today row's Open button opens.
    var openedNoun: String {
        switch self {
        case .result: "Result"
        case .write: "Text to Send"
        case .briefing: "Briefing"
        case .failure: "Failure"
        case .newTask: "Proposed Task"
        default: "Item"
        }
    }
}
