import Foundation
import MuxaProtocol
import MuxaSession

/// A workspace's staff and a request's chain of steps (ADR 0051) on a host with `work`
/// level 12. A level 11 host keeps one assistant per workspace, as before.
extension AppModel {
    var usesStaff: Bool { usesPlans && session.canUseStaff }

    /// The members Ask offers: only when the workspace has more than one (ADR 0051 §7).
    func pickableStaff(in workspace: String?) -> [StaffMember] {
        let staff = session.staff(of: workspace)
        return staff.count > 1 ? staff : []
    }

    /// Approve or Reject on a waiting hand-off, from its Work item. Returns the host's
    /// message when it refused: a budget reached, or another device answered first.
    func answerHandoff(_ item: InboxItem, _ action: InboxAction) async -> String? {
        do {
            try await session.answer(item, action)
            return nil
        } catch {
            return PaneSession.describe(error)
        }
    }
}

extension StepBy {
    /// Who picked the step's member. An unknown value reads as unknown (PROTOCOL.md).
    var shown: String {
        switch self {
        case .user: "You"
        case .overall: "Overall assistant"
        case .lead: "Lead"
        case .handoff: "Hand-off"
        case .other: "Unknown"
        }
    }
}

extension AssistantRequest {
    /// "chief → researcher → writer".
    var chainShown: String { chain.joined(separator: " → ") }
}
