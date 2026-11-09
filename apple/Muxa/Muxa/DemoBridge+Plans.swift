import Foundation
import MuxaProtocol

/// `plan-now`, `apply-notes` and approving the overall assistant's proposals (`work`
/// level 9, ADR 0047), answered as the bridge with its daemon answers them.
extension DemoBridge {
    /// A level 8 host answers the new requests `bad-request`, as any unknown request.
    func servePlans(_ req: Request) {
        guard Self.workLevel >= workPlans else {
            return emit(.result(id: req.id, error: ErrorBody(code: .badRequest, message: "unknown request"), sessions: nil))
        }
        switch req {
        case .planNow(let id, let minutes):
            let outcome = lock.withLock { () -> Result<Plan, DemoWork.Refusal> in
                do throws(DemoWork.Refusal) { return .success(try work.planNow(minutes: minutes)) } catch { return .failure(error) }
            }
            switch outcome {
            case .success(let plan):
                emit(.planSaved(plan: id, entry: plan))
                ok(id)
                finishPlanLater()
            case .failure(let refusal):
                emit(.result(id: id, error: refusal.body, sessions: nil))
            }
        case .applyNotes(let id, let source, let version, let base):
            let outcome = lock.withLock { () -> Result<DemoNotesApplied, DemoWork.Refusal> in
                do throws(DemoWork.Refusal) {
                    return .success(try work.applyNotes(source: source, version: version, base: base))
                } catch { return .failure(error) }
            }
            switch outcome {
            case .success(.request(let item, let request)):
                emit(.workSaved(work: id, entry: item, request: request))
                ok(id)
            case .success(.plan(let plan)):
                emit(.planSaved(plan: id, entry: plan))
                ok(id)
            case .failure(let refusal):
                emit(.result(id: id, error: refusal.body, sessions: nil))
            }
        default:
            unsupported(req.id, Self.noRequest)
        }
    }

    /// The plans and the overall assistant as `inbox-list` sends them; none below level 9.
    func listedPlans() -> (plans: [Plan], assistant: AssistantInfo?) {
        guard Self.workLevel >= workPlans else { return ([], nil) }
        return lock.withLock {
            _ = work.finishPlan()
            return (work.listedPlans, work.plans.assistant)
        }
    }

    func isOverallProposal(_ id: String) -> Bool {
        guard Self.workLevel >= workPlans else { return false }
        return lock.withLock {
            work.taskItems.contains { $0.id == id && ($0.kind == .sort || $0.kind == .delegate) }
        }
    }

    /// Approve runs the proposal once: `capture-saved` or `work-saved` with what it made,
    /// then `inbox-saved`, then the result. A refusal leaves the item open.
    func approveProposal(id: UInt64, item: String) {
        let outcome = lock.withLock { () -> Result<DemoProposalOutcome, DemoWork.Refusal> in
            do throws(DemoWork.Refusal) { return .success(try work.approveProposal(item)) } catch { return .failure(error) }
        }
        switch outcome {
        case .success(.delegated(let entry, let request, let change, let saved)):
            emit(.workSaved(work: id, entry: entry, request: request))
            emit(.inboxSaved(inbox: id, entry: nil, item: saved))
            ok(id)
            apply(change)
        case .success(.sorted(let capture, let entry, let saved)):
            emit(.captureSaved(capture: id, entry: capture, work: entry))
            emit(.inboxSaved(inbox: id, entry: nil, item: saved))
            ok(id)
        case .success(.resolved(let saved, let refusal)):
            emit(.inboxSaved(inbox: id, entry: nil, item: saved))
            emit(.result(id: id, error: refusal.body, sessions: nil))
        case .failure(let refusal):
            emit(.result(id: id, error: refusal.body, sessions: nil))
        }
    }

    /// The plan run ends after `planAfter`; `tasks-changed` makes the app list again.
    private func finishPlanLater() {
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(DemoPlans.planAfter + 0.2))
            guard let self else { return }
            if self.lock.withLock({ self.work.finishPlan() }) { self.emit(.tasksChanged) }
        }
    }
}
