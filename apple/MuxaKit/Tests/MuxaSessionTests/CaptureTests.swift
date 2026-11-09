import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with Captures and Work items (`work` level 8, ADR 0046), as the host answers them.
class CaptureBridge: FakeBridge, @unchecked Sendable {
    var captures: [Capture] = [CaptureTests.capture("c-00000001", "Look at the flaky test\nIt failed twice.")]
    var work: [WorkItem] = [CaptureTests.work("w-00000001", "Plan the migration")]
    /// The connection drops on the next `capture`, before any answer.
    var dropOnCapture = false
    private let lock = NSLock()
    private var _captureRequests: [Request] = []
    /// `capture` requests, in order; `received` records only what `FakeBridge` answers.
    var captureRequests: [Request] { lock.withLock { _captureRequests } }

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        if case .capture = req { lock.withLock { _captureRequests.append(req) } }
        switch req {
        case .listInbox(let id):
            emit(.inboxList(inbox: id, entries: [], items: [], work: work, captures: captures.filter { $0.state == .received }))
            emit(.result(id: id, error: nil, sessions: nil))
        case .capture(let id, let clientId, let text, _, let workspace, _):
            if dropOnCapture {
                await close()
                return
            }
            let saved = captures.first { $0.clientId == clientId }
                ?? Capture(id: String(format: "c-%08x", captures.count + 1), rev: 1, clientId: clientId, text: text,
                           source: .app, state: .received, createdAt: 5, updatedAt: 5, workspace: workspace)
            if !captures.contains(saved) { captures.append(saved) }
            emit(.captureSaved(capture: id, entry: saved))
            emit(.result(id: id, error: nil, sessions: nil))
        case .sortCapture(let id, let capture, let rev, let action, _, _, _):
            guard let i = captures.firstIndex(where: { $0.id == capture }), captures[i].rev == rev else {
                return emit(.result(id: id, error: ErrorBody(code: .conflict, message: "stale"), sessions: nil))
            }
            captures[i].rev += 1
            captures[i].state = .sorted
            var made: WorkItem?
            if action == .work {
                let item = CaptureTests.work(String(format: "w-%08x", work.count + 1), "Look at the flaky test")
                work.insert(item, at: 0)
                captures[i].work = [item.id]
                made = item
            }
            emit(.captureSaved(capture: id, entry: captures[i], work: made))
            emit(.result(id: id, error: nil, sessions: nil))
        case .updateWork(let id, let item, let rev, let action, _, _, _):
            guard let i = work.firstIndex(where: { $0.id == item }), work[i].rev == rev else {
                return emit(.result(id: id, error: ErrorBody(code: .conflict, message: "stale"), sessions: nil))
            }
            work[i].rev += 1
            switch action {
            case .complete: work[i].state = .done
            case .later: work[i].laterAt = 9
            case .reopen:
                work[i].state = .open
                work[i].laterAt = nil
            default: break
            }
            emit(.workSaved(work: id, entry: work[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        default:
            try await super.send(line)
        }
    }
}

@MainActor
struct CaptureTests {
    nonisolated static func capture(_ id: String, _ text: String) -> Capture {
        Capture(id: id, rev: 1, clientId: "client-\(id)", text: text, source: .tmux, state: .received, createdAt: 1, updatedAt: 1)
    }

    nonisolated static func work(_ id: String, _ title: String) -> WorkItem {
        WorkItem(id: id, rev: 1, title: title, state: .open, createdAt: 1, updatedAt: 1, workspace: "work")
    }

    func connected(_ bridge: CaptureBridge, level: String = "work:8") async throws -> PaneSession {
        bridge.capabilities = ["task-records", level]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    @Test func connectListsWorkAndUnsortedCaptures() async throws {
        let session = try await connected(CaptureBridge())
        #expect(session.canCapture)
        #expect(session.openWork.map(\.id) == ["w-00000001"])
        #expect(session.captures.map(\.id) == ["c-00000001"])
    }

    @Test func anOlderHostKeepsNoCapturesAndIsNeverAskedForOne() async throws {
        let bridge = CaptureBridge()
        let session = try await connected(bridge, level: "work:7")
        #expect(!session.canCapture)
        #expect(session.workList.isEmpty)
        #expect(session.captures.isEmpty)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.capturesUnsupported))) {
            try await session.capture(clientId: "x", text: "hi", workspace: nil)
        }
        #expect(bridge.captureRequests.isEmpty)
    }

    @Test func aCaptureIsSavedWithItsClientIdAndListedFirst() async throws {
        let bridge = CaptureBridge()
        let session = try await connected(bridge)
        let saved = try await session.capture(clientId: "id-1", text: "Renew the domain", workspace: "work")
        #expect(saved.text == "Renew the domain")
        #expect(session.captures.first?.id == saved.id)
        // Sent again, the host returns the same Capture: one stays listed.
        let again = try await session.capture(clientId: "id-1", text: "Renew the domain", workspace: "work")
        #expect(again.id == saved.id)
        #expect(session.captures.filter { $0.id == saved.id }.count == 1)
        let sent = bridge.captureRequests.compactMap { req -> CaptureSource? in
            if case .capture(_, "id-1", _, let source, "work", nil) = req { source } else { nil }
        }
        #expect(sent == [.app, .app])
    }

    @Test func aDroppedConnectionIsAConnectionLoss() async throws {
        let bridge = CaptureBridge()
        let session = try await connected(bridge)
        bridge.dropOnCapture = true
        do {
            try await session.capture(clientId: "id-2", text: "Lost", workspace: nil)
            Issue.record("the capture cannot be saved without a connection")
        } catch {
            #expect(PaneSession.isConnectionLoss(error))
        }
        #expect(!PaneSession.isConnectionLoss(PaneSession.Failure.bridge(ErrorBody(code: .badRequest, message: "too long"))))
    }

    @Test func sortingIntoWorkListsTheItemAndRemovesTheCapture() async throws {
        let session = try await connected(CaptureBridge())
        let made = try await session.sort(session.captures[0], .work)
        #expect(made?.title == "Look at the flaky test")
        #expect(session.captures.isEmpty)
        #expect(session.openWork.map(\.id) == [made?.id, "w-00000001"])
        #expect(session.knownCaptures["c-00000001"]?.state == .sorted, "its text stays known")
    }

    @Test func aStaleRevListsAgainAndSaysItChanged() async throws {
        let bridge = CaptureBridge()
        let session = try await connected(bridge)
        let seen = session.workList[0]
        _ = try await session.update(seen, .later)
        #expect(session.workList[0].laterAt == 9)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: PaneSession.workConflict))) {
            try await session.update(seen, .complete)
        }
        #expect(session.workList[0].rev == 2, "the list shows the host's state")
        _ = try await session.update(session.workList[0], .complete)
        #expect(session.openWork.isEmpty)
    }

    @Test func aTaskFromAWorkItemNamesItAndItsRev() async throws {
        let bridge = CaptureBridge()
        let session = try await connected(bridge)
        let item = session.workList[0]
        _ = try await session.createTask(NewTaskRecord(workspace: "work", title: "T", goal: "G", work: item.id, workRev: item.rev))
        #expect(bridge.received.contains {
            if case .createTask(_, _, _, _, _, _, _, _, _, _, "w-00000001", 1) = $0 { true } else { false }
        })
    }

    @Test func anOlderHostGetsNoWorkFieldsOnATask() async throws {
        let bridge = CaptureBridge()
        let session = try await connected(bridge, level: "work:7")
        _ = try await session.createTask(NewTaskRecord(workspace: "work", title: "T", goal: "G", work: "w-1", workRev: 1))
        #expect(bridge.received.contains {
            if case .createTask(_, _, _, _, _, _, _, _, _, _, nil, nil) = $0 { true } else { false }
        })
    }
}

@MainActor
struct CaptureDraftTests {
    @Test func aCaptureNeedsTextWithinTheLimit() {
        #expect(CaptureDraft(text: "  \n ").problem == "Write something first.")
        #expect(CaptureDraft(text: String(repeating: "a", count: CaptureDraft.maxBytes)).problem == nil)
        #expect(CaptureDraft(text: String(repeating: "あ", count: CaptureDraft.maxBytes / 3 + 1)).problem != nil, "bytes, not characters")
        #expect(CaptureDraft(text: " Renew the domain \n").trimmed == "Renew the domain")
    }

    @Test func eachDraftHasItsOwnClientId() {
        #expect(CaptureDraft().clientId != CaptureDraft().clientId)
    }

    @Test func aTaskTakesTheFirstLineAsTitleAndTheTextAsGoal() {
        let base = TaskRecordDraft(workspace: "work", repo: "/src/app")
        let draft = CaptureDraft(text: "\n  Fix the flaky test  \nIt failed twice on main.").taskDraft(base)
        #expect(draft.title == "Fix the flaky test")
        #expect(draft.goal == "Fix the flaky test  \nIt failed twice on main.")
        #expect(draft.repo == "/src/app")
        #expect(CaptureDraft(text: String(repeating: "x", count: 100)).taskDraft(base).title.count == CaptureDraft.titleLength)
        #expect(CaptureDraft(text: " ").taskDraft(base) == base, "an empty capture opens the form as it is")
    }
}

@MainActor
struct UnsentCaptureTests {
    private func file() -> URL {
        FileManager.default.temporaryDirectory.appending(path: "unsent-\(UUID().uuidString).json")
    }

    @Test func unsentCapturesSurviveARestartPerHost() {
        let url = file()
        defer { try? FileManager.default.removeItem(at: url) }
        let a = UUID(), b = UUID()
        let store = UnsentCaptures(file: url)
        store.add(UnsentCapture(clientId: "1", host: a, text: "first"))
        store.add(UnsentCapture(clientId: "2", host: b, text: "other host"))
        store.add(UnsentCapture(clientId: "3", host: a, text: "second", workspace: "work"))
        let reloaded = UnsentCaptures(file: url)
        #expect(reloaded.on(a).map(\.text) == ["first", "second"], "oldest first")
        #expect(reloaded.on(b).map(\.clientId) == ["2"])
        #expect(reloaded.on(a).last?.workspace == "work")
    }

    @Test func theSameClientIdIsKeptOnceAndRemovedWhenSent() {
        let url = file()
        defer { try? FileManager.default.removeItem(at: url) }
        let host = UUID()
        let store = UnsentCaptures(file: url)
        store.add(UnsentCapture(clientId: "1", host: host, text: "draft"))
        store.add(UnsentCapture(clientId: "1", host: host, text: "draft"))
        store.setProblem("1", "The host refused it.")
        #expect(store.on(host).count == 1)
        #expect(UnsentCaptures(file: url).on(host).first?.problem == "The host refused it.")
        store.remove("1")
        #expect(UnsentCaptures(file: url).all.isEmpty)
    }

    @Test func anUnreadableFileIsReported() throws {
        let url = file()
        defer { try? FileManager.default.removeItem(at: url) }
        try Data("not json".utf8).write(to: url)
        let store = UnsentCaptures(file: url)
        #expect(store.all.isEmpty)
        #expect(store.storageError != nil)
    }
}
