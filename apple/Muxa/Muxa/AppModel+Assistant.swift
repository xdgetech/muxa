import Foundation
import MuxaProtocol
import MuxaSession

/// A workspace's assistant on Work items (ADR 0048 §10). Requests run on the host's
/// daemon; the app asks, cancels, and shows the delivery and the Result.
extension AppModel {
    /// The item's newest request: the one a row and Cancel speak of.
    func latestRequest(of item: WorkItem) -> AssistantRequest? {
        session.requests(of: item).last
    }

    /// The run's session while it lives, to open as a Task's session opens.
    func runSession(_ request: AssistantRequest) -> Session? {
        guard let run = request.run else { return nil }
        return session.sessions.first { $0.taskId == run && $0.defaultPane != nil }
    }

    /// The newest request has a Result this device has not shown.
    func hasNewResult(_ item: WorkItem) -> Bool {
        guard let request = latestRequest(of: item) else { return false }
        return seenResults.isNew(request, on: currentHostID)
    }

    func markResultsSeen(_ item: WorkItem) {
        for request in session.requests(of: item) { seenResults.markSeen(request, on: currentHostID) }
    }

    /// Asks the assistant; `worker` names a staff member, nil lets the lead decide.
    /// Returns the host's message when it refused.
    func askAssistant(
        _ item: WorkItem, scope: AssistScope, purpose: String, workspace: String?, worker: String? = nil
    ) async -> String? {
        let p = purpose.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            try await session.ask(item, scope: scope, purpose: p.isEmpty ? nil : p,
                                  workspace: item.workspace == nil ? workspace : nil, worker: worker)
            return nil
        } catch {
            return PaneSession.describe(error)
        }
    }

    /// Stops the request's current version. Returns the host's message when it refused.
    func cancelRequest(_ request: AssistantRequest) async -> String? {
        do {
            try await session.cancel(request)
            return nil
        } catch {
            return PaneSession.describe(error)
        }
    }

    /// Reads the item again for its detail. Returns why it could not be read.
    func readWork(_ id: String) async -> String? {
        do {
            try await session.readWork(id)
            return nil
        } catch {
            return PaneSession.describe(error)
        }
    }
}

extension AssistScope {
    /// What the user can ask for, in the order the picker shows.
    static let choices: [AssistScope] = [.research, .compare, .draft]

    var shown: String {
        switch self {
        case .research: "research"
        case .compare: "compare"
        case .draft: "draft"
        case .other(let raw): raw
        }
    }
}

extension Delivery {
    /// How a row says it. An unknown value reads as unknown (PROTOCOL.md).
    var shown: String {
        switch self {
        case .pending: "pending"
        case .received: "received"
        case .processing: "processing"
        case .unconfirmed: "not confirmed"
        case .resulted: "result in"
        case .failed: "failed"
        case .timedOut: "timed out"
        case .cancelRequested: "cancelling"
        case .cancelled: "cancelled"
        case .other: "unknown"
        }
    }

    /// What the user can do about it, when there is something to say.
    var advice: String? {
        switch self {
        case .unconfirmed: "The prompt may not have arrived. It is never typed again by itself: cancel, or ask again."
        case .failed: "No valid result came back. Ask again."
        case .timedOut: "No result in the workspace's run time. The agent still runs; a late result is still kept."
        default: nil
        }
    }
}
