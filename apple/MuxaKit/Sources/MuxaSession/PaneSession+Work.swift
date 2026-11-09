import Foundation
import MuxaProtocol

/// Routines and stored Inbox entries behind the host's `work` level 1 (ADR 0026).
extension PaneSession {
    /// The host's `work` level; 0 when it has none (ADR 0027 §5).
    public var workLevel: UInt32 { MuxaProtocol.workLevel(capabilities) }

    public var canUseRoutines: Bool { workLevel >= workRoutines }

    /// Shown when the host has no Routines.
    public static let routinesUnsupported = "This host has no Routines. Update muxa on the host."

    /// Lists the Routines and the stored entries again. A failure lands in `workError`.
    public func loadWork() async {
        guard canUseRoutines else { return }
        do {
            let routines = try await listing { .listRoutines(id: $0) } take: { self.routineListings.removeValue(forKey: $0) }
            let inbox = try await listing { .listInbox(id: $0) } take: { self.inboxListings.removeValue(forKey: $0) }
            if let routines { self.routines = routines }
            if let inbox {
                inboxEntries = inbox.entries
                // An older host sends no items; its Inbox is built from the Tasks.
                inboxItems = canUseItems ? inbox.items : []
                keepListed(inbox)
            }
            workError = nil
        } catch {
            workError = Self.describe(error)
        }
    }

    /// Starts a run of `routine` now. The bridge without the daemon refuses it as
    /// `unsupported`; its message says what to run on the host.
    public func runRoutine(_ routine: RoutineInfo) async throws -> RoutineInfo {
        try requireRoutines()
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .runRoutine(id: id, workspace: routine.workspace, routine: routine.name)
            }
        } catch {
            savedRoutines[requestId] = nil
            // A run already open, or a Routine disabled since: show the host's state.
            await loadWork()
            throw error
        }
        let saved = savedRoutines.removeValue(forKey: requestId)
        // The run's session starts with it.
        try? await refresh()
        guard let saved else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the Routine."))
        }
        return saved
    }

    /// Approves a proposal: the host creates one Task, also when asked twice. Returns
    /// the entry with that Task's id, once the task list has it.
    public func approve(_ entry: InboxEntry) async throws -> InboxEntry {
        let saved = try await updateEntry(entry, .approve, until: nil)
        try? await refresh()
        await loadTasks()
        return saved
    }

    /// Dismisses a briefing or a proposal. It never touches a Task.
    public func dismiss(_ entry: InboxEntry) async throws -> InboxEntry {
        try await updateEntry(entry, .dismiss, until: nil)
    }

    /// Hides the entry until `until`, epoch seconds in the future.
    public func later(_ entry: InboxEntry, until: UInt64) async throws -> InboxEntry {
        try await updateEntry(entry, .later, until: until)
    }

    private func updateEntry(_ entry: InboxEntry, _ action: InboxAction, until: UInt64?) async throws -> InboxEntry {
        try requireRoutines()
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return .updateInboxEntry(id: id, entry: entry.id, action: action, until: until)
            }
        } catch {
            savedEntries[requestId] = nil
            // Another device may have answered it first.
            await loadWork()
            throw error
        }
        guard let saved = savedEntries.removeValue(forKey: requestId) else {
            throw Failure.bridge(ErrorBody(code: .internal, message: "The bridge did not return the Inbox entry."))
        }
        return saved
    }

    private func requireRoutines() throws {
        guard canUseRoutines else {
            throw Failure.bridge(ErrorBody(code: .unsupported, message: Self.routinesUnsupported))
        }
    }

    /// One `inbox-list`, kept until its result arrives.
    struct InboxListing {
        var entries: [InboxEntry]
        var items: [InboxItem]
        var work: [WorkItem]
        var captures: [Capture]
        var requests: [AssistantRequest]
        var plans: [Plan]
        var assistant: AssistantInfo?
    }

    /// Sends a list request; its list arrives just before the result. Nil when none came.
    private func listing<T>(_ make: (UInt64) -> Request, take: (UInt64) -> T?) async throws -> T? {
        var requestId: UInt64 = 0
        do {
            try await request { id in
                requestId = id
                return make(id)
            }
        } catch {
            _ = take(requestId)
            throw error
        }
        return take(requestId)
    }

    /// A newer state of a listed Routine replaces it.
    func keep(_ routine: RoutineInfo) {
        if let i = routines.firstIndex(where: { $0.workspace == routine.workspace && $0.name == routine.name }) {
            routines[i] = routine
        } else {
            routines.append(routine)
        }
    }

    func keep(_ entry: InboxEntry) {
        if let i = inboxEntries.firstIndex(where: { $0.id == entry.id }) {
            inboxEntries[i] = entry
        } else {
            inboxEntries.insert(entry, at: 0)
        }
    }
}
