import Foundation
import MuxaProtocol

/// Captures and Work items behind the host's `work` level 8 (ADR 0046). None of them
/// is a Task or an Inbox item, and nothing here starts an agent.
extension PaneSession {
    public var canCapture: Bool { workLevel >= workAssistant }

    /// Shown when the host keeps no Captures.
    public static let capturesUnsupported = "This host cannot save captures. Update muxa on the host."
    /// Shown when another device changed the Capture or the Work item first.
    public static let workConflict = "This changed on another device. The Inbox now shows its latest state. Try again."

    /// The Work items the Inbox lists: neither done nor withdrawn, in the host's order.
    public var openWork: [WorkItem] { workList.filter(\.isOpen) }

    /// Saves the user's text on the host. The same `clientId` returns the stored Capture
    /// and writes nothing, so a capture kept unsent is safe to send again. With `assist`
    /// the host also makes a Work item in `workspace` and asks its assistant; when the
    /// assistant cannot start, the Capture is still saved and `AssistNotStarted` says why.
    @discardableResult
    public func capture(clientId: String, text: String, workspace: String?, assist: AssistSpec? = nil) async throws -> Capture {
        try requireCaptures()
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .capture(id: id, clientId: clientId, text: text, source: .app, workspace: workspace, assist: assist)
            }
        } catch Failure.bridge(let body) where assist != nil {
            savedRequests[requestId] = nil
            guard let kept = savedCaptures.removeValue(forKey: requestId) else { throw Failure.bridge(body) }
            throw AssistNotStarted(capture: kept.capture, reason: body)
        } catch {
            savedCaptures[requestId] = nil
            savedRequests[requestId] = nil
            throw error
        }
        savedRequests[requestId] = nil
        guard let saved = savedCaptures.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the capture."))
        }
        return saved.capture
    }

    /// Sorts an unsorted Capture: into a new Work item, into `work`, or as a note.
    /// Returns the Work item it went into, if any.
    @discardableResult
    public func sort(_ capture: Capture, _ action: SortAction, into work: WorkItem? = nil) async throws -> WorkItem? {
        try requireCaptures()
        var requestId: UInt64 = 0
        do {
            try await changing {
                try await request { id in
                    requestId = id
                    return .sortCapture(id: id, capture: capture.id, rev: capture.rev, action: action, work: work?.id)
                }
            }
        } catch {
            savedCaptures[requestId] = nil
            throw error
        }
        guard let saved = savedCaptures.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the capture."))
        }
        // The host's order and derived states.
        await loadWork()
        return saved.work
    }

    /// Changes a Work item: Later, Complete, Withdraw or Reopen. Its Tasks stay as they are.
    @discardableResult
    public func update(_ item: WorkItem, _ action: WorkAction) async throws -> WorkItem {
        try requireCaptures()
        var requestId: UInt64 = 0
        do {
            try await changing {
                try await request { id in
                    requestId = id
                    return .updateWork(id: id, work: item.id, rev: item.rev, action: action)
                }
            }
        } catch {
            savedWork[requestId] = nil
            throw error
        }
        guard let saved = savedWork.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the Work item."))
        }
        // Later moves the item down (ADR 0040): its order is the list's.
        await loadWork()
        return saved
    }

    /// A stale `rev` lists the Inbox again and says so; nothing changed then.
    private func changing(_ send: () async throws -> Void) async throws {
        do {
            try await send()
        } catch Failure.bridge(let body) where body.code == .conflict {
            await loadWork()
            throw Failure.bridge(ErrorBody(code: .conflict, message: Self.workConflict))
        }
    }

    func requireCaptures() throws {
        guard canCapture else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.capturesUnsupported))
        }
    }

    /// The lists of one `inbox-list`. An older host sends none of them.
    func keepListed(_ inbox: InboxListing) {
        guard canCapture else {
            workList = []
            captures = []
            assistantRequests = []
            return
        }
        workList = inbox.work
        captures = inbox.captures
        assistantRequests = inbox.requests
        for c in inbox.captures { knownCaptures[c.id] = c }
        for r in inbox.requests { keepRead(r) }
    }

    /// An unsorted Capture is listed, newest first; a sorted one leaves the list.
    func keep(_ capture: Capture) {
        knownCaptures[capture.id] = capture
        let i = captures.firstIndex { $0.id == capture.id }
        switch (capture.state, i) {
        case (.received, let i?): captures[i] = capture
        case (.received, nil): captures.insert(capture, at: 0)
        case (_, let i?): captures.remove(at: i)
        case (_, nil): break
        }
    }

    /// A newer state of a listed Work item replaces it; a new one goes first.
    func keep(_ item: WorkItem) {
        if let i = workList.firstIndex(where: { $0.id == item.id }) {
            workList[i] = item
        } else {
            workList.insert(item, at: 0)
        }
    }
}

extension WorkItem {
    /// Neither done nor withdrawn. An unknown state counts as open (PROTOCOL.md).
    public var isOpen: Bool { state != .done && state != .withdrawn }
}
