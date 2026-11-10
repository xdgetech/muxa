import Foundation
import MuxaProtocol

/// `add-policy`, `update-policy`, `update-plan`, `forget` and `confirm` (`work` level 11,
/// ADR 0050), answered as the bridge answers them.
extension DemoBridge {
    /// A level 10 host answers the new requests `bad-request`, as any unknown request.
    func servePolicies(_ req: Request) {
        guard Self.workLevel >= workPolicies else {
            return emit(.result(id: req.id, error: ErrorBody(code: .badRequest, message: "unknown request"), sessions: nil))
        }
        let scopes = policyScopes
        switch req {
        case .addPolicy(let id, let text, let scope, let from, let until):
            savePolicy(id) { w throws(DemoWork.Refusal) in
                try w.addPolicy(text: text, scope: scope, from: from, until: until, scopes: scopes)
            }
        case .updatePolicy(let id, let policy, let rev, let action, let text, let scope, let from, let until):
            savePolicy(id) { w throws(DemoWork.Refusal) in
                try w.updatePolicy(policy, rev: rev, action: action, text: text, scope: scope, from: from, until: until,
                                   scopes: scopes)
            }
        case .updatePlan(let id, let plan, let rev, let action, let order, let row, let text):
            let outcome = lock.withLock { () -> Result<Plan, DemoWork.Refusal> in
                do throws(DemoWork.Refusal) {
                    return .success(try work.updatePlan(plan, rev: rev, action: action, order: order, row: row, text: text))
                } catch { return .failure(error) }
            }
            switch outcome {
            case .success(let saved):
                emit(.planSaved(plan: id, entry: saved))
                ok(id)
            case .failure(let refusal):
                emit(.result(id: id, error: refusal.body, sessions: nil))
            }
        case .forget(let id, let record):
            let outcome = lock.withLock { () -> DemoWork.Refusal? in
                do throws(DemoWork.Refusal) {
                    try work.forget(record)
                    return nil
                } catch { return error }
            }
            if let outcome { return emit(.result(id: id, error: outcome.body, sessions: nil)) }
            ok(id)
        default:
            unsupported(req.id, Self.noRequest)
        }
    }

    /// The memory as `inbox-list` sends it; none below level 11.
    func listedMemory() -> (policies: [Policy], hypotheses: [Hypothesis], corrections: [Correction]) {
        guard Self.workLevel >= workPolicies else { return ([], [], []) }
        return lock.withLock { work.listedMemory }
    }

    /// Confirm makes a policy of the hypothesis: `policy-saved`, `inbox-saved`, then the result.
    func confirmHypothesis(id: UInt64, item: String) {
        guard Self.workLevel >= workPolicies else { return unsupported(id, Self.noRequest) }
        let outcome = lock.withLock { () -> Result<(Policy, InboxItem), DemoWork.Refusal> in
            do throws(DemoWork.Refusal) { return .success(try work.confirm(item)) } catch { return .failure(error) }
        }
        switch outcome {
        case .success(let (policy, saved)):
            emit(.policySaved(policy: id, entry: policy))
            emit(.inboxSaved(inbox: id, entry: nil, item: saved))
            ok(id)
        case .failure(let refusal):
            emit(.result(id: id, error: refusal.body, sessions: nil))
        }
    }

    /// `edit` and `add` send `policy-saved`; `delete` only the result.
    private func savePolicy(_ id: UInt64, _ change: (inout DemoWork) throws(DemoWork.Refusal) -> Policy?) {
        let outcome = lock.withLock { () -> Result<Policy?, DemoWork.Refusal> in
            do throws(DemoWork.Refusal) { return .success(try change(&work)) } catch { return .failure(error) }
        }
        switch outcome {
        case .success(let saved):
            if let saved { emit(.policySaved(policy: id, entry: saved)) }
            ok(id)
        case .failure(let refusal):
            emit(.result(id: id, error: refusal.body, sessions: nil))
        }
    }
}
