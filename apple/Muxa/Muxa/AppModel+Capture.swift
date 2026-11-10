import Foundation
import MuxaProtocol
import MuxaSession

/// Captures and Work items (ADR 0048) on a host with `work` level 9. An older host
/// keeps New Task behind `+`, and its Inbox has decisions only.
extension AppModel {
    /// `+` opens Capture. It still does while the connection is down, for the host
    /// that last connected with Captures: the capture waits on this device.
    var capturesFirst: Bool {
        isConnected ? session.canCapture : captureHostID != nil && captureHostID == currentHostID
    }

    /// The Inbox has Work and Unsorted below the decisions.
    var usesWork: Bool { isConnected && session.canCapture }

    /// This host's captures that wait on this device.
    var unsentHere: [UnsentCapture] { unsent.on(currentHostID) }

    func workItem(_ id: String) -> WorkItem? { session.workList.first { $0.id == id } }
    func listedCapture(_ id: String) -> Capture? { session.captures.first { $0.id == id } }

    func startCapture() {
        guard capturesFirst else { return startNewTask() }
        capture = CaptureDraft()
        // Ask Assistant needs a workspace to pick.
        if isConnected, session.workspaces.isEmpty { Task { await session.loadTasks() } }
    }

    /// Saves the capture on the host. While the connection is down it stays on this
    /// device as Not sent, with the same `clientId`. Throws what the host refused.
    /// With `assist` the host also asks its assistant; that needs the connection, and a
    /// refusal throws `AssistNotStarted` with the Capture kept in Unsorted.
    func save(_ draft: CaptureDraft, assist: AssistSpec? = nil) async throws {
        guard draft.problem == nil, let host = currentHostID else { return }
        do {
            try await session.capture(
                clientId: draft.clientId, text: draft.trimmed, workspace: draft.workspace, assist: assist)
        } catch let error where assist == nil && PaneSession.isConnectionLoss(error) {
            unsent.add(UnsentCapture(clientId: draft.clientId, host: host, text: draft.trimmed, workspace: draft.workspace))
        }
        capture = nil
    }

    /// "Start a Task instead…": the New Task form with the text as its goal, once the
    /// Capture sheet has closed. Nothing is saved.
    func startTaskInstead(of draft: CaptureDraft) async {
        if session.workspaces.isEmpty { await session.loadTasks() }
        let repo = currentSession?.repoDir ?? session.sessions.knownRepos.first?.dir ?? ""
        var base = TaskRecordDraft(repo: repo, in: session.pickableWorkspaces)
        if let workspace = draft.workspace, workspace != base.workspace {
            base = TaskRecordDraft(workspace: workspace)
        }
        taskAfterCapture = draft.taskDraft(base)
        capture = nil
    }

    func captureSheetClosed() {
        guard let draft = taskAfterCapture else { return refocusPane() }
        taskAfterCapture = nil
        newRecord = draft
    }

    /// Sends this host's unsent captures, oldest first. The host returns the stored
    /// Capture for a `clientId` it has, so a capture that did arrive is not made twice.
    func resendUnsent() async {
        guard isConnected, session.canCapture else { return }
        for c in unsentHere {
            do {
                try await session.capture(clientId: c.clientId, text: c.text, workspace: c.workspace)
                unsent.remove(c.clientId)
            } catch let error where PaneSession.isConnectionLoss(error) {
                return
            } catch {
                unsent.setProblem(c.clientId, PaneSession.describe(error))
            }
        }
    }

    // MARK: Sorting and changing

    /// Sorts an unsorted Capture. A Capture shown in the detail gives way to its Work item.
    func sort(_ capture: Capture, _ action: SortAction, into work: WorkItem? = nil) {
        Task {
            do {
                let made = try await session.sort(capture, action, into: work)
                if detail == .capture(capture.id) { detail = made.map { .work($0.id) } }
            } catch {
                laterFailure = PaneSession.describe(error)
            }
        }
    }

    /// Later, Complete, Withdraw or Reopen. Its Tasks stay as they are.
    func change(_ item: WorkItem, _ action: WorkAction) {
        Task {
            do {
                try await session.update(item, action)
            } catch {
                laterFailure = PaneSession.describe(error)
            }
        }
    }

    /// The New Task form for a Work item: one Task per item `rev`, on any device.
    func startTask(for item: WorkItem) async {
        if session.workspaces.isEmpty { await session.loadTasks() }
        let workspace = item.workspace ?? session.pickableWorkspaces.first?.name ?? ""
        let texts = item.captures.compactMap { session.knownCaptures[$0]?.text }
        newRecord = TaskRecordDraft(
            workspace: workspace, title: String(item.title.prefix(CaptureDraft.titleLength)),
            goal: texts.isEmpty ? item.title : texts.joined(separator: "\n\n"), work: item.id, workRev: item.rev)
    }
}

extension WorkState {
    /// How a row says it. An unknown state reads as open (PROTOCOL.md).
    var shown: String {
        switch self {
        case .open, .other: "open"
        case .inProgress: "in progress"
        case .waitingDecision: "waiting on decision"
        case .done: "done"
        case .withdrawn: "withdrawn"
        }
    }
}
