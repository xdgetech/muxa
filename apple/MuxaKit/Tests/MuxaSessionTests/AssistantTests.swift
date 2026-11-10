import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge that also answers assistant requests (ADR 0048 §9) as the host does, with
/// a daemon unless `daemon` is false.
class AssistantBridge: CaptureBridge, @unchecked Sendable {
    var requests: [AssistantRequest] = []
    /// `request-assistant` is refused with this, after a `work-saved` with a failed request.
    var refusal: ErrorBody?
    private let lock = NSLock()
    private var _sent: [Request] = []
    /// The assistant requests the session sent, in order.
    var sent: [Request] { lock.withLock { _sent } }

    static let budget = ErrorBody(
        code: .conflict,
        message: "workspace \"work\" started 20 assistant runs today, its max_runs_per_day. The count resets at 00:00.")

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        switch req {
        case .listInbox(let id):
            emit(.inboxList(inbox: id, entries: [], items: [], work: work,
                            captures: captures.filter { $0.state == .received }, requests: requests))
            emit(.result(id: id, error: nil, sessions: nil))
        case .getWork(let id, let item):
            record(req)
            guard let entry = work.first(where: { $0.id == item }) else {
                return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "no Work item \(item)"), sessions: nil))
            }
            emit(.workDetail(work: id, entry: entry, captures: captures.filter { entry.captures.contains($0.id) },
                             requests: requests.filter { $0.work == item }))
            emit(.result(id: id, error: nil, sessions: nil))
        case .requestAssistant(let id, let item, let rev, let scope, let workspace, let purpose, _):
            record(req)
            ask(id, item, rev: rev, scope: scope, workspace: workspace, purpose: purpose)
        case .cancelRequest(let id, let request, let version):
            record(req)
            guard let i = requests.firstIndex(where: { $0.id == request }), requests[i].version == version,
                  let w = work.firstIndex(where: { $0.id == requests[i].work })
            else { return emit(.result(id: id, error: ErrorBody(code: .conflict, message: "stale"), sessions: nil)) }
            requests[i].delivery = .cancelRequested
            emit(.workSaved(work: id, entry: work[w], request: requests[i]))
            emit(.result(id: id, error: nil, sessions: nil))
        case .capture(let id, let clientId, let text, _, let workspace, let assist?):
            record(req)
            let saved = Capture(id: "c-0000000a", rev: 1, clientId: clientId, text: text, source: .app,
                                state: .received, createdAt: 5, updatedAt: 5, workspace: workspace)
            captures.append(saved)
            if let refusal {
                emit(.captureSaved(capture: id, entry: saved))
                return emit(.result(id: id, error: refusal, sessions: nil))
            }
            let item = WorkItem(id: "w-0000000a", rev: 2, title: text, state: .inProgress, createdAt: 5, updatedAt: 5,
                                workspace: workspace, captures: [saved.id], requests: ["q-0000000a"])
            let made = Self.request("q-0000000a", work: item.id, scope: assist.scope)
            var sorted = saved
            sorted.state = .sorted
            sorted.work = [item.id]
            captures[captures.count - 1] = sorted
            work.insert(item, at: 0)
            requests.append(made)
            emit(.captureSaved(capture: id, entry: sorted, work: item, request: made))
            emit(.result(id: id, error: nil, sessions: nil))
        default:
            try await super.send(line)
        }
    }

    private func ask(_ id: UInt64, _ item: String, rev: UInt64, scope: AssistScope, workspace: String?, purpose: String?) {
        guard let w = work.firstIndex(where: { $0.id == item }) else {
            return emit(.result(id: id, error: ErrorBody(code: .badRequest, message: "no Work item \(item)"), sessions: nil))
        }
        if let refusal {
            return emit(.result(id: id, error: refusal, sessions: nil))
        }
        guard work[w].rev == rev else {
            return emit(.result(id: id, error: ErrorBody(code: .conflict, message: "\(item) changed"), sessions: nil))
        }
        work[w].rev += 1
        var made: AssistantRequest
        if let i = requests.firstIndex(where: { $0.work == item }) {
            requests[i].version += 1
            requests[i].delivery = .pending
            made = requests[i]
        } else {
            made = Self.request(String(format: "q-%08x", requests.count + 1), work: item, scope: scope)
            made.purpose = purpose
            if let workspace { made.workspace = workspace }
            requests.append(made)
            work[w].requests.append(made.id)
        }
        emit(.workSaved(work: id, entry: work[w], request: made))
        emit(.result(id: id, error: nil, sessions: nil))
    }

    private func record(_ req: Request) { lock.withLock { _sent.append(req) } }

    static func request(_ id: String, work: String, scope: AssistScope, results: [AssistResult] = []) -> AssistantRequest {
        AssistantRequest(id: id, work: work, version: 1, workspace: "work", scope: scope, delivery: .pending,
                         deliveryAt: 5, createdAt: 5, updatedAt: 5, run: "t-run001", results: results)
    }
}

@MainActor
struct AssistantTests {
    func connected(_ bridge: AssistantBridge, level: String = "work:9") async throws -> PaneSession {
        bridge.capabilities = ["task-records", level]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    static let result = AssistResult(
        version: 2, at: 9, summary: "Two plans fit.",
        findings: [Finding(text: "Plan A is cheaper.", sources: [ResultSource(url: "https://example.com/pricing")])],
        openQuestions: ["Which region?"], next: ["Ask for a quote."], files: ["/w/assistant/draft.md"],
        possibleDuplicates: [PossibleDuplicate(work: "w-00000002", reason: "Same vendor.")])

    @Test func theInboxListsRequestsWithTheirResults() async throws {
        let bridge = AssistantBridge()
        bridge.work[0].requests = ["q-00000001"]
        var resulted = AssistantBridge.request("q-00000001", work: "w-00000001", scope: .compare, results: [
            Self.result, AssistResult(version: 1, at: 7, summary: "Older.", stale: true),
        ])
        resulted.delivery = .resulted
        bridge.requests = [resulted]
        let session = try await connected(bridge)
        let shown = session.requests(of: session.workList[0])
        #expect(shown.map(\.delivery) == [.resulted])
        #expect(shown.first?.currentResult?.summary == "Two plans fit.")
        #expect(shown.first?.earlierResults.map(\.summary) == ["Older."])
    }

    @Test func readingAWorkItemKeepsEveryCaptureText() async throws {
        let bridge = AssistantBridge()
        var sorted = CaptureTests.capture("c-00000002", "Sorted on another device")
        sorted.state = .sorted
        bridge.captures.append(sorted)
        bridge.work[0].captures = ["c-00000002"]
        let session = try await connected(bridge)
        #expect(session.knownCaptures["c-00000002"] == nil, "a sorted Capture is not listed")
        let read = try await session.readWork("w-00000001")
        #expect(read.captures.map(\.text) == ["Sorted on another device"])
        #expect(session.workDetails["w-00000001"]?.captures.map(\.id) == ["c-00000002"])
        #expect(session.knownCaptures["c-00000002"]?.text == "Sorted on another device")
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .badRequest, message: "no Work item w-9"))) {
            try await session.readWork("w-9")
        }
    }

    @Test func askingSendsTheItemsRevAndListsTheRequest() async throws {
        let bridge = AssistantBridge()
        let session = try await connected(bridge)
        let item = session.workList[0]
        let made = try await session.ask(item, scope: .draft, purpose: "A memo")
        #expect(made.id == "q-00000001")
        #expect(made.delivery == .pending)
        #expect(bridge.sent.contains {
            if case .requestAssistant(_, "w-00000001", 1, .draft, nil, "A memo", nil) = $0 { true } else { false }
        })
        #expect(session.workList[0].requests == ["q-00000001"])
        #expect(session.requests(of: session.workList[0]).map(\.id) == ["q-00000001"])
        // Asking again sends the new rev, and the host adds a version.
        let again = try await session.ask(session.workList[0], scope: .draft)
        #expect(again.version == 2)
        #expect(bridge.sent.contains {
            if case .requestAssistant(_, "w-00000001", 2, .draft, nil, nil, nil) = $0 { true } else { false }
        })
    }

    @Test func aRefusalKeepsTheHostsMessage() async throws {
        let bridge = AssistantBridge()
        bridge.refusal = AssistantBridge.budget
        let session = try await connected(bridge)
        await #expect(throws: PaneSession.Failure.bridge(AssistantBridge.budget)) {
            try await session.ask(session.workList[0], scope: .research)
        }
        #expect(PaneSession.describe(PaneSession.Failure.bridge(AssistantBridge.budget)).contains("resets at"))
    }

    @Test func cancelSendsTheRequestsVersion() async throws {
        let bridge = AssistantBridge()
        let session = try await connected(bridge)
        _ = try await session.ask(session.workList[0], scope: .research)
        let made = try await session.ask(session.workList[0], scope: .research)
        let cancelled = try await session.cancel(made)
        #expect(cancelled.delivery == .cancelRequested)
        #expect(!cancelled.delivery.isActive)
        #expect(bridge.sent.contains {
            if case .cancelRequest(_, "q-00000001", 2) = $0 { true } else { false }
        })
    }

    @Test func aCaptureWithAssistMakesAWorkItemAndARequest() async throws {
        let bridge = AssistantBridge()
        let session = try await connected(bridge)
        let saved = try await session.capture(
            clientId: "id-1", text: "Compare hosting", workspace: "work", assist: AssistSpec(scope: .compare))
        #expect(saved.state == .sorted)
        #expect(session.workList.first?.id == "w-0000000a")
        #expect(session.assistantRequests.map(\.id) == ["q-0000000a"])
        #expect(bridge.sent.contains {
            if case .capture(_, "id-1", _, .app, "work", AssistSpec(scope: .compare)) = $0 { true } else { false }
        })
    }

    @Test func aCaptureKeptWithoutItsAssistantSaysWhy() async throws {
        let bridge = AssistantBridge()
        let off = ErrorBody(code: .unsupported, message: "Automatic processing is off.")
        bridge.refusal = off
        let session = try await connected(bridge)
        do {
            try await session.capture(clientId: "id-2", text: "Later", workspace: "work", assist: AssistSpec(scope: .research))
            Issue.record("the assistant cannot start without the daemon")
        } catch let error as PaneSession.AssistNotStarted {
            #expect(error.reason == off)
            #expect(error.capture.state == .received)
            #expect(PaneSession.describe(error) == off.message)
        }
        #expect(session.captures.contains { $0.clientId == "id-2" }, "the Capture waits in Unsorted")
    }

    @Test func anOlderHostIsNeverAsked() async throws {
        let bridge = AssistantBridge()
        let session = try await connected(bridge, level: "work:8")
        let item = CaptureTests.work("w-00000001", "Plan")
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.capturesUnsupported))) {
            try await session.ask(item, scope: .research)
        }
        await #expect(throws: PaneSession.Failure.self) { try await session.readWork(item.id) }
        #expect(bridge.sent.isEmpty)
    }
}

@MainActor
struct SeenResultsTests {
    @Test func aResultIsNewUntilShownPerHostAndVersion() {
        let url = FileManager.default.temporaryDirectory.appending(path: "seen-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: url) }
        let host = UUID()
        var request = AssistantBridge.request("q-00000001", work: "w-00000001", scope: .research)
        let seen = SeenResults(file: url)
        #expect(!seen.isNew(request, on: host), "no Result yet")
        request.results = [AssistResult(version: 1, at: 5, summary: "One.")]
        #expect(seen.isNew(request, on: host))
        seen.markSeen(request, on: host)
        #expect(!SeenResults(file: url).isNew(request, on: host), "kept across a restart")
        #expect(SeenResults(file: url).isNew(request, on: UUID()), "another host's request")
        request.results.insert(AssistResult(version: 2, at: 9, summary: "Two."), at: 0)
        request.results[1].stale = true
        #expect(seen.isNew(request, on: host), "a new version's Result")
        #expect(!seen.keys.joined().contains("One."), "ids and versions only")
    }
}
