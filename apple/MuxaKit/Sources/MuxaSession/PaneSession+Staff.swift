import Foundation
import MuxaProtocol

/// A workspace's staff and a request's chain of steps (ADR 0051) on a host with `work`
/// level 12. Clients say "staff member"; the wire field stays `worker`.
extension PaneSession {
    public var canUseStaff: Bool { workLevel >= workStaff }

    /// Shown when the host has no staff to ask by name.
    public static let staffUnsupported = "This host cannot ask a staff member. Update muxa on the host."

    /// The workspace's staff in configuration order, the lead marked. Empty below level 12,
    /// and for a workspace without `staff`.
    public func staff(of workspace: String?) -> [StaffMember] {
        guard canUseStaff, let workspace else { return [] }
        return assistantInfo?.delegates.first { $0.workspace == workspace }?.staff ?? []
    }

    /// The member who takes a request that names no one.
    public func lead(of workspace: String?) -> StaffMember? {
        let staff = staff(of: workspace)
        return staff.first(where: \.lead) ?? staff.first
    }

    /// The `handoff` Inbox item that waits on the step's hand-off, open or moved down.
    public func handoffItem(for request: AssistantRequest, step: UInt32) -> InboxItem? {
        handoffItems(for: request, step: step).first { $0.state == .open || $0.state == .later }
    }

    /// The `handoff` items of one step, whatever their state, newest first.
    public func handoffItems(for request: AssistantRequest, step: UInt32) -> [InboxItem] {
        inboxItems.filter {
            $0.kind == .handoff && $0.handoff?.request == request.id && $0.handoff?.version == request.version
                && $0.handoff?.step == step
        }
    }

    func requireStaff() throws {
        guard canUseStaff else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.staffUnsupported))
        }
    }
}

extension AssistantRequest {
    /// The result of step `step` of the current version; step 1 when a result names none.
    public func result(ofStep step: UInt32) -> AssistResult? {
        results.first { $0.version == version && ($0.step ?? 1) == step }
    }

    /// The newest step's hand-off that started no step yet: the chain waits on it.
    public var waitingHandoff: (step: RequestStep, handoff: Handoff)? {
        guard let last = steps.last, let handoff = last.handoff, last.delivery == .resulted else { return nil }
        return (last, handoff)
    }

    /// The members in the chain, the waiting one last: "a → b → c".
    public var chain: [String] {
        steps.map(\.worker) + (waitingHandoff.map { [$0.handoff.worker] } ?? [])
    }
}
