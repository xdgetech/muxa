import Foundation
import MuxaProtocol

/// Assistant requests on Work items behind the host's `work` level 8 (ADR 0046 §9). A
/// request needs the host's daemon; without it the host refuses with `unsupported`.
extension PaneSession {
    /// A Work item as `get-work` read it: every Capture it links, sorted or not, and its requests.
    public struct LoadedWork: Equatable, Sendable {
        public var entry: WorkItem
        public var captures: [Capture]
        public var requests: [AssistantRequest]

        public init(entry: WorkItem, captures: [Capture], requests: [AssistantRequest]) {
            self.entry = entry
            self.captures = captures
            self.requests = requests
        }
    }

    /// `capture {assist}` saved the Capture, but the assistant did not start. `reason`
    /// is the host's; the Capture waits in Unsorted.
    public struct AssistNotStarted: Error, Equatable {
        public let capture: Capture
        public let reason: ErrorBody
    }

    /// Reads one Work item for its detail view. Its Captures' texts become known too.
    @discardableResult
    public func readWork(_ id: String) async throws -> LoadedWork {
        try requireCaptures()
        var requestId: UInt64 = 0
        do {
            try await request { rid in
                requestId = rid
                return .getWork(id: rid, work: id)
            }
        } catch {
            readWorks[requestId] = nil
            throw error
        }
        guard let loaded = readWorks.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the Work item."))
        }
        workDetails[id] = loaded
        for c in loaded.captures { knownCaptures[c.id] = c }
        if workList.contains(where: { $0.id == id }) { keep(loaded.entry) }
        return loaded
    }

    /// The requests of `item`, in its order, as last listed or read.
    public func requests(of item: WorkItem) -> [AssistantRequest] {
        let read = workDetails[item.id]?.requests ?? []
        return item.requests.compactMap { id in
            assistantRequests.first { $0.id == id } ?? read.first { $0.id == id }
        }
    }

    /// Asks the workspace's agent to work on `item`. Asking again with the item's new
    /// `rev` adds a version to that workspace's request. A refusal keeps the host's
    /// message: no daemon, a budget reached with its reset time, or a stale `rev`.
    @discardableResult
    public func ask(
        _ item: WorkItem, scope: AssistScope, purpose: String? = nil, workspace: String? = nil
    ) async throws -> AssistantRequest {
        try requireCaptures()
        let made = try await changingRequest { id in
            .requestAssistant(id: id, work: item.id, rev: item.rev, scope: scope, workspace: workspace, purpose: purpose)
        }
        // The item's order and derived state are the list's.
        await loadWork()
        return made
    }

    /// Stops the request's current version. Its delivery is `cancel-requested` until the
    /// run's session is gone.
    @discardableResult
    public func cancel(_ request: AssistantRequest) async throws -> AssistantRequest {
        try requireCaptures()
        let saved = try await changingRequest { id in
            .cancelRequest(id: id, request: request.id, version: request.version)
        }
        await loadWork()
        return saved
    }

    private func changingRequest(_ make: (UInt64) -> Request) async throws -> AssistantRequest {
        var requestId: UInt64 = 0
        do {
            try await self.request { id in
                requestId = id
                return make(id)
            }
        } catch {
            savedWork[requestId] = nil
            savedRequests[requestId] = nil
            // A refused or failed run is shown as the host now has it.
            if case Failure.bridge = error { await loadWork() }
            throw error
        }
        savedWork[requestId] = nil
        guard let saved = savedRequests.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the request."))
        }
        return saved
    }

    /// A newer state of a request replaces it in the list and in a read Work item.
    func keep(_ request: AssistantRequest) {
        if let i = assistantRequests.firstIndex(where: { $0.id == request.id }) {
            assistantRequests[i] = request
        } else {
            assistantRequests.append(request)
        }
        keepRead(request)
    }

    /// A read Work item shows the request's latest state.
    func keepRead(_ request: AssistantRequest) {
        guard var read = workDetails[request.work] else { return }
        if let i = read.requests.firstIndex(where: { $0.id == request.id }) {
            read.requests[i] = request
        } else {
            read.requests.append(request)
        }
        workDetails[request.work] = read
    }
}

extension Delivery {
    /// The run of this version may still deliver: Cancel stops it, and the item is not
    /// asked again until it ends.
    public var isActive: Bool {
        switch self {
        case .pending, .received, .processing, .unconfirmed: true
        default: false
        }
    }
}

extension AssistantRequest {
    /// The newest result that no newer version replaced.
    public var currentResult: AssistResult? { results.first { !$0.stale } }

    /// Results of older versions, newest first.
    public var earlierResults: [AssistResult] { results.filter(\.stale) }
}
