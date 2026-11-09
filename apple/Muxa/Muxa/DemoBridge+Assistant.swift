import Foundation
import MuxaProtocol

/// `capture`, `sort-capture`, `update-work`, `get-work` and the assistant requests
/// (`work` level 8, ADR 0046), answered as the bridge with its daemon answers them.
extension DemoBridge {
    func serveCaptures(_ req: Request) async {
        guard Self.workLevel >= workAssistant else { return unsupported(req.id, Self.noRequest) }
        switch req {
        case .capture(let id, let clientId, let text, _, let workspace, let assist?):
            captureAndAsk(id, clientId: clientId, text: text, workspace: workspace, assist: assist)
        case .capture(let id, let clientId, let text, _, let workspace, nil):
            // The UI tests' connection loss: the capture never gets an answer, once per launch.
            if Self.dropsCapture.take() { return await close() }
            let outcome = lock.withLock { () -> Result<Capture, DemoWork.Refusal> in
                do throws(DemoWork.Refusal) {
                    return .success(try work.assistant.capture(clientId: clientId, text: text, workspace: workspace))
                } catch { return .failure(error) }
            }
            switch outcome {
            case .success(let saved):
                emit(.captureSaved(capture: id, entry: saved))
                ok(id)
            case .failure(let refusal):
                emit(.result(id: id, error: refusal.body, sessions: nil))
            }
        case .sortCapture(let id, let capture, let rev, let action, let target, let title, _):
            let outcome = lock.withLock { () -> Result<(Capture, WorkItem?), DemoWork.Refusal> in
                do throws(DemoWork.Refusal) {
                    return .success(try work.assistant.sort(capture, rev: rev, action: action, into: target, title: title))
                } catch { return .failure(error) }
            }
            switch outcome {
            case .success(let (saved, item)):
                emit(.captureSaved(capture: id, entry: saved, work: item))
                ok(id)
            case .failure(let refusal):
                emit(.result(id: id, error: refusal.body, sessions: nil))
            }
        case .updateWork(let id, let item, let rev, let action, let title, let workspace, _, let due):
            // A level 8 host knows neither pins nor due dates (ADR 0047 §7).
            if Self.workLevel < workPlans, action == .pin || action == .unpin || due != nil {
                return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "unknown action"), sessions: nil))
            }
            let outcome = lock.withLock { () -> Result<WorkItem, DemoWork.Refusal> in
                do throws(DemoWork.Refusal) {
                    let laterAt = max(work.nextLaterAt, work.assistant.lastLaterAt + 1)
                    return .success(try work.assistant.update(
                        item, rev: rev, action: action, laterAt: laterAt, title: title, workspace: workspace, due: due))
                } catch { return .failure(error) }
            }
            switch outcome {
            case .success(let saved):
                emit(.workSaved(work: id, entry: saved))
                ok(id)
            case .failure(let refusal):
                emit(.result(id: id, error: refusal.body, sessions: nil))
            }
        case .getWork(let id, let item):
            advanceRuns()
            let outcome = lock.withLock { () -> Result<(WorkItem, [Capture], [AssistantRequest]), DemoWork.Refusal> in
                do throws(DemoWork.Refusal) {
                    return .success(try work.assistant.detail(item, shown: shownWork()))
                } catch { return .failure(error) }
            }
            switch outcome {
            case .success(let (entry, captures, requests)):
                emit(.workDetail(work: id, entry: entry, captures: captures, requests: requests))
                ok(id)
            case .failure(let refusal):
                emit(.result(id: id, error: refusal.body, sessions: nil))
            }
        case .requestAssistant(let id, let item, let rev, let scope, let workspace, let purpose):
            changeRequest(id) { a throws(DemoWork.Refusal) in
                try a.ask(item, rev: rev, scope: scope, workspace: workspace, purpose: purpose)
            }
        case .cancelRequest(let id, let request, let version):
            changeRequest(id) { a throws(DemoWork.Refusal) in try a.cancel(request, version: version) }
        default:
            unsupported(req.id, Self.noRequest)
        }
    }

    /// The Work items as `inbox-list` sends them, with states derived from the Tasks' items,
    /// and their requests as their runs' ages make them.
    func listedWork() -> (work: [WorkItem], captures: [Capture], requests: [AssistantRequest]) {
        guard Self.workLevel >= workAssistant else { return ([], [], []) }
        advanceRuns()
        return lock.withLock {
            let shown = shownWork()
            return (shown, work.assistant.shownCaptures, work.assistant.listedRequests(of: shown))
        }
    }

    /// Call with the lock held.
    private func shownWork() -> [WorkItem] {
        let waiting = Set(work.shownItems.filter { $0.state == .open }.compactMap(\.task))
        return work.assistant.shownWork { waiting.contains($0) }
    }

    /// `capture-saved`, then the result: with the refusal when the assistant did not start.
    private func captureAndAsk(_ id: UInt64, clientId: String, text: String, workspace: String?, assist: AssistSpec) {
        let outcome = lock.withLock { () -> Result<(Capture, WorkItem?, AssistantRequest?, DemoAssistant.RunChange, DemoWork.Refusal?), DemoWork.Refusal> in
            do throws(DemoWork.Refusal) {
                return .success(try work.assistant.capture(clientId: clientId, text: text, workspace: workspace, assist: assist))
            } catch { return .failure(error) }
        }
        switch outcome {
        case .success(let (saved, item, request, change, refusal)):
            emit(.captureSaved(capture: id, entry: saved, work: item, request: request))
            emit(.result(id: id, error: refusal?.body, sessions: nil))
            apply(change)
        case .failure(let refusal):
            emit(.result(id: id, error: refusal.body, sessions: nil))
        }
    }

    /// `work-saved` with the request, then the result; the run's session starts or closes.
    private func changeRequest(
        _ id: UInt64,
        _ change: (inout DemoAssistant) throws(DemoWork.Refusal) -> (WorkItem, AssistantRequest, DemoAssistant.RunChange)
    ) {
        let outcome = lock.withLock { () -> Result<(WorkItem, AssistantRequest, DemoAssistant.RunChange), DemoWork.Refusal> in
            do throws(DemoWork.Refusal) { return .success(try change(&work.assistant)) } catch { return .failure(error) }
        }
        switch outcome {
        case .success(let (item, request, runs)):
            emit(.workSaved(work: id, entry: item, request: request))
            ok(id)
            apply(runs)
        case .failure(let refusal):
            emit(.result(id: id, error: refusal.body, sessions: nil))
        }
    }

    /// Starts and closes the runs' sessions, and moves a new run on at each step of its delivery.
    func apply(_ change: DemoAssistant.RunChange) {
        guard change.started != nil || !change.ended.isEmpty else { return }
        lock.withLock {
            let ended = Set(change.ended.map(\.task))
            sessions.removeAll { $0.taskId.map(ended.contains) ?? false }
            if let run = change.started { sessions.append(Self.runSession(run, number: 200 + work.assistant.runCount)) }
        }
        emit(.sessionsChanged)
        guard change.started != nil else { return }
        for step in [DemoAssistant.receivedAfter, DemoAssistant.processingAfter, DemoAssistant.resultAfter] {
            Task { [weak self] in
                try? await Task.sleep(for: .seconds(step + 0.2))
                guard let self else { return }
                if !self.advanceRuns() { self.emit(.tasksChanged) }
            }
        }
    }

    /// Moves the runs on by their age. A run with its Result closes its session; then
    /// `sessions-changed` makes the app list again. True when one did.
    @discardableResult
    private func advanceRuns() -> Bool {
        let ended = lock.withLock { () -> [DemoRun] in
            let ended = work.assistant.advance()
            let tasks = Set(ended.map(\.task))
            sessions.removeAll { $0.taskId.map(tasks.contains) ?? false }
            return ended
        }
        if !ended.isEmpty { emit(.sessionsChanged) }
        return !ended.isEmpty
    }

    /// A run's session, named as the host names it, with its agent at work.
    private static func runSession(_ run: DemoRun, number n: Int) -> Session {
        Session(id: "$\(n)", name: run.session, taskId: run.task, panes: [
            Pane(id: "%\(n)", window: "@\(n)", index: 0, role: .agent, cols: 100, rows: 30, command: "claude",
                 path: "/work/assistant/\(run.session)", dead: false, shellForeground: false,
                 agent: Agent(state: .running, since: UInt64(run.started.timeIntervalSince1970))),
        ])
    }

    /// The UI tests' `-demo-drop-capture`: the first capture finds the connection gone.
    static let dropsCapture = Once(armed: {
        #if DEBUG
        ProcessInfo.processInfo.arguments.contains("-demo-drop-capture")
        #else
        false
        #endif
    }())
}

/// True the first time it is taken, if armed; false after that.
final class Once: @unchecked Sendable {
    private let lock = NSLock()
    private var armed: Bool

    init(armed: Bool) { self.armed = armed }

    func take() -> Bool {
        lock.withLock {
            defer { armed = false }
            return armed
        }
    }
}
