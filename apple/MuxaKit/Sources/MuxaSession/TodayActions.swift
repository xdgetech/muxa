import Foundation
import MuxaProtocol

/// What a Today row asks the user to do (ADR 0059 §1–§5), at `work` level 17. The verbs and
/// labels are the client's, from the target's kind; muxa for tmux uses the same words.
extension PaneSession {
    /// Today rows name their action, a Work item's Send goes to the secretary, and Re-rank
    /// says the runs left today.
    public var namesTodayActions: Bool { workLevel >= workToday }

    /// Results and the secretary propose Tasks, which Start Task starts in plan mode (ADR 0060).
    public var proposesTasks: Bool { workLevel >= workTaskProposals }

    /// What a Re-rank has left today, when the host says it.
    public var overallRuns: RunBudget? { namesTodayActions ? assistantInfo?.overallRuns : nil }

    /// The secretary's workspace, when the host takes a Work item's Send: level 17 and an
    /// overall workspace.
    public var secretary: String? { namesTodayActions ? assistantWorkspace : nil }

    /// Shown when the host cannot send a message to the secretary.
    public static let secretaryUnsupported =
        "This host cannot send a message to the secretary. Set up an overall assistant, or update muxa on the host."

    /// The verb of a Today row: "Start" for a Work item, else from its Inbox item's kind.
    /// An item no longer listed is "Open".
    public func todayVerb(_ row: PlanRow) -> String {
        if row.work != nil { return "Start" }
        guard let id = row.item, let item = inboxItems.first(where: { $0.id == id }) else { return "Open" }
        return item.todayVerb
    }

    /// Who gets a Today row's Send, or nil when its target takes no text.
    public func todaySend(_ row: PlanRow) -> TodaySend? {
        if let work = row.work {
            guard let secretary else { return nil }
            return TodaySend(route: .secretary(work: work, workspace: secretary),
                             label: "Send to the secretary (\(secretary)), with this Work item…")
        }
        guard let id = row.item, let item = inboxItems.first(where: { $0.id == id }) else { return nil }
        return item.todaySend
    }

    /// Types `text` into the secretary's session after the Work item `work` (ADR 0059 §4): the
    /// overall workspace's root Ask session, opened or started. Lists sessions again and
    /// returns it. The host refuses with `conflict` while a menu shows or a person types.
    public func sendToSecretary(_ text: String, work: String? = nil) async throws -> StartedSession {
        guard let workspace = secretary else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.secretaryUnsupported))
        }
        return try await startRoot(workspace: workspace, text: text, work: work)
    }
}

/// Where a Today row's Send goes, and the label that names the receiver (ADR 0059 §4).
public struct TodaySend: Equatable, Sendable {
    public enum Route: Equatable, Sendable {
        /// An answer on the Inbox item that takes the text: `reply` or `ask-agent`.
        case answer(InboxAction)
        /// `start-session` with `root`, `text` and `work`, to the overall workspace.
        case secretary(work: String, workspace: String)
    }

    public var route: Route
    public var label: String

    public init(route: Route, label: String) {
        self.route = route
        self.label = label
    }
}

extension RunBudget {
    /// "2 of 3 runs left today".
    public var shown: String { "\(left) of \(max) runs left today" }
}

extension InboxItem {
    /// The verb of a Today row whose target is this item (ADR 0059 §1, ADR 0060 §8.6). A
    /// briefing whose question ends in "?" asks to decide; another one is to read.
    public var todayVerb: String {
        switch kind {
        case .review: "Review changes"
        case .ask: "Allow or deny"
        case .result: "Read result"
        case .write: "Check text to send"
        case .briefing: question.trimmingCharacters(in: .whitespacesAndNewlines).hasSuffix("?") ? "Decide" : "Read"
        case .failure: "Fix"
        case .memory: "Check memory note"
        case .proposal: "Approve or reject"
        case .sort: "Sort capture"
        case .delegate: "Approve request"
        case .hypothesis: "Confirm or reject"
        case .handoff: "Approve hand-off"
        case .newTask: "Start Task"
        case .other: "Open"
        }
    }

    /// A Today row's Send on this item: `reply` names the agent or member it reaches, else
    /// `ask-agent` the workspace's Ask session. Nil: the item takes no text.
    public var todaySend: TodaySend? {
        if answers.contains(.reply) {
            let label = switch kind {
            case .result: "Send to \(result?.worker ?? "the assistant")…"
            case .write: "Revise: send to \(write?.worker ?? "the assistant")…"
            case .newTask: "Send to the secretary…"
            case .ask where isPlan: "Keep planning & send to the Task's agent…"
            case .ask: "Deny & send to the Task's agent…"
            default: "Send to the Task's agent…"
            }
            return TodaySend(route: .answer(.reply), label: label)
        }
        if answers.contains(.askAgent) {
            let what = kind == .briefing ? "briefing" : "item"
            return TodaySend(route: .answer(.askAgent), label: "Send to the \(workspace) Ask session, with this \(what)…")
        }
        return nil
    }
}
