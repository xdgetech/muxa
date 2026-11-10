import Foundation
import MuxaProtocol

/// The overall assistant's plans and the delegates' notes behind the host's `work`
/// level 10 (ADR 0049). A plan never changes a Work item, an Inbox item or their order.
extension PaneSession {
    public var canPlan: Bool { workLevel >= workPlans }

    /// Shown when the host has no plans, notes or pins.
    public static let plansUnsupported = "This host has no plans. Update muxa on the host."
    /// Shown when the live `NOTES.md` no longer is the file the run read.
    public static let notesConflict = "The notes changed on the host. Ask the assistant again."

    /// The newest plan, whatever its delivery: a pending one is being made, a failed or
    /// cancelled one has its `reason`.
    public var newestPlan: Plan? { plans.first }

    /// The newest plan with an answer: the one Today shows.
    public var currentPlan: Plan? { plans.first { $0.delivery == .resulted } }

    /// A plan is being made: Plan Now would be `conflict`.
    public var isPlanning: Bool { newestPlan?.delivery.isActive ?? false }

    /// The workspaces New Task, Capture and Ask offer: all but the overall assistant's,
    /// which takes no Tasks and no requests.
    public var pickableWorkspaces: [WorkspaceInfo] {
        guard let overall = assistantInfo?.overall else { return workspaces }
        return workspaces.filter { $0.name != overall }
    }

    /// The current plan's row for a Work item, if Today lists it.
    public func planRow(forWork id: String) -> PlanRow? {
        currentPlan?.top.first { $0.work == id }
    }

    /// The current plan says no run answered this request yet.
    public func isUnanswered(_ request: AssistantRequest) -> Bool {
        currentPlan?.unanswered.contains(request.id) ?? false
    }

    /// The current plan says this request's newest result is older than its Work item.
    public func isStale(_ request: AssistantRequest) -> Bool {
        currentPlan?.stale.contains(request.id) ?? false
    }

    /// Asks the overall assistant for a plan now; `minutes` is the time the user has.
    /// Returns the `pending` plan. A plan being made is `conflict`; no overall
    /// workspace or no daemon is `unsupported`, with the host's message.
    @discardableResult
    public func planNow(minutes: UInt32? = nil) async throws -> Plan {
        try requirePlans()
        var requestId: UInt64 = 0
        defer { savedPlans[requestId] = nil }
        do {
            try await request { id in
                requestId = id
                return .planNow(id: id, minutes: minutes)
            }
        } catch Failure.bridge(let body) {
            // Another device may have asked first: show the plan being made.
            if body.code == .conflict { await loadWork() }
            throw Failure.bridge(body)
        }
        guard let saved = savedPlans[requestId] else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the plan."))
        }
        return saved
    }

    /// Writes a Result's proposed notes to the workspace's live `NOTES.md`. A live file
    /// that changed since the run read it is `conflict` with `notesConflict`.
    public func applyNotes(of request: AssistantRequest, result: AssistResult) async throws {
        guard let notes = result.notes else { return }
        try await applyNotes(source: request.id, version: result.version, notes: notes)
    }

    /// Writes a plan's proposed notes to the overall workspace's `NOTES.md`.
    public func applyNotes(of plan: Plan) async throws {
        guard let notes = plan.notes else { return }
        try await applyNotes(source: plan.id, version: plan.rev, notes: notes)
    }

    /// This device applied the Result's notes.
    public func notesApplied(_ request: AssistantRequest, _ result: AssistResult) -> Bool {
        appliedNotes.contains(Self.notesKey(request.id, result.version))
    }

    private func applyNotes(source: String, version: UInt64, notes: Notes) async throws {
        try requirePlans()
        var requestId: UInt64 = 0
        defer {
            savedWork[requestId] = nil
            savedRequests[requestId] = nil
            savedPlans[requestId] = nil
        }
        do {
            try await request { id in
                requestId = id
                return .applyNotes(id: id, source: source, version: version, base: notes.base)
            }
        } catch Failure.bridge(let body) where body.code == .conflict {
            throw Failure.bridge(ErrorBody(code: .conflict, message: Self.notesConflict))
        }
        appliedNotes.insert(Self.notesKey(source, version))
    }

    private static func notesKey(_ source: String, _ version: UInt64) -> String { "\(source) \(version)" }

    func requirePlans() throws {
        guard canPlan else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.plansUnsupported))
        }
    }

    /// A newer state of a listed plan replaces it; a new one goes first.
    func keep(_ plan: Plan) {
        if let i = plans.firstIndex(where: { $0.id == plan.id }) {
            plans[i] = plan
        } else {
            plans.insert(plan, at: 0)
        }
    }
}
