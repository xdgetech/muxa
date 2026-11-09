import Foundation
import MuxaProtocol

/// `capture`, `sort-capture` and `update-work` (`work` level 8, ADR 0046), answered as
/// the bridge answers them. `request-assistant` stays `unsupported`: there is no daemon.
extension DemoBridge {
    func serveCaptures(_ req: Request) async {
        guard Self.workLevel >= workAssistant else { return unsupported(req.id, Self.noRequest) }
        switch req {
        case .capture(let id, let clientId, let text, _, let workspace, _):
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
        case .updateWork(let id, let item, let rev, let action, _, _, _):
            let outcome = lock.withLock { () -> Result<WorkItem, DemoWork.Refusal> in
                do throws(DemoWork.Refusal) {
                    let laterAt = max(work.nextLaterAt, work.assistant.lastLaterAt + 1)
                    return .success(try work.assistant.update(item, rev: rev, action: action, laterAt: laterAt))
                } catch { return .failure(error) }
            }
            switch outcome {
            case .success(let saved):
                emit(.workSaved(work: id, entry: saved))
                ok(id)
            case .failure(let refusal):
                emit(.result(id: id, error: refusal.body, sessions: nil))
            }
        default:
            // Assistant requests need the daemon, which the demo does not have.
            unsupported(req.id, "Automatic processing is off: the demo has no muxa daemon.")
        }
    }

    /// The Work items as `inbox-list` sends them, with states derived from the Tasks' items.
    func listedWork() -> (work: [WorkItem], captures: [Capture]) {
        guard Self.workLevel >= workAssistant else { return ([], []) }
        return lock.withLock {
            let waiting = Set(work.shownItems.filter { $0.state == .open }.compactMap(\.task))
            return (work.assistant.shownWork { waiting.contains($0) }, work.assistant.shownCaptures)
        }
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
