import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with write proposals (`work` level 13, ADR 0052), as the host answers: the
/// writer in `inbox-list`, and a `write` item whose approve needs the write's hash and
/// starts the writer's Task once.
final class WriteBridge: PlanBridge, @unchecked Sendable {
    /// The writer's Task, once approved.
    var writerTask: TaskRecord?
    private let sentLock = NSLock()
    private var _writeAnswers: [Request] = []
    /// The answers to `write` items the session sent, in order.
    var writeAnswers: [Request] { sentLock.withLock { _writeAnswers } }

    override init() {
        super.init()
        assistant = AssistantInfo(overall: "assistant", delegates: [
            DelegateInfo(workspace: "work", share: .summaries, overallRequests: .review, staff: StaffTests.staff,
                         writer: WriterInfo(worker: "poster", description: "Posts approved text to Slack."),
                         writes: .review),
            DelegateInfo(workspace: "home", share: .outline, overallRequests: .none, writes: Writes.none),
        ])
        requests = [WriteTests.request]
        work[0].requests = ["q-00000001"]
        items = [WriteTests.item]
    }

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        guard case .updateInboxEntry(let id, let entry, let action, _, _, _, _, let hash) = req,
              let i = items.firstIndex(where: { $0.id == entry }), let write = items[i].write
        else { return try await super.send(line) }
        sentLock.withLock { _writeAnswers.append(req) }
        if action == .approve {
            guard hash == write.hash else {
                let refusal = ErrorBody(code: .badRequest, message: "this app cannot show the whole write")
                return emit(.result(id: id, error: refusal, sessions: nil))
            }
            let record = writerTask ?? TaskRecord(
                id: "t-write01", rev: 1, workspace: "work", title: write.action, goal: "muxa-write: \(entry) \(write.hash)",
                status: .open, createdAt: 9, updatedAt: 9, worker: "poster", work: "w-00000001")
            if writerTask == nil { taskRecords.append(record) }
            writerTask = record
            items[i].task = record.id
            requests[0].results[0].writes[0].task = record.id
            emit(.taskSaved(task: id, record: record))
        }
        if action != .later {
            items[i].state = .answered
            items[i].answer = action
            items[i].answers = []
        }
        emit(.inboxSaved(inbox: id, entry: nil, item: items[i]))
        emit(.result(id: id, error: nil, sessions: nil))
    }
}

@MainActor
struct WriteTests {
    nonisolated static let write = ResultWrite(
        action: "Post to #release", target: "slack:#release", content: "Release 1.4 ships today.\n**Not bold**",
        hash: WriteHash.of(action: "Post to #release", target: "slack:#release",
                           content: "Release 1.4 ships today.\n**Not bold**"),
        reason: "The team asked for a note.", item: "i-0000000w")

    nonisolated static var request: AssistantRequest {
        var r = AssistantBridge.request("q-00000001", work: "w-00000001", scope: .draft, results: [
            AssistResult(version: 1, at: 8, summary: "Drafted.", writes: [write]),
        ])
        r.delivery = .resulted
        return r
    }

    nonisolated static let item = InboxItem(
        id: "i-0000000w", kind: .write, state: .open, question: "researcher proposes to write to slack:#release",
        workspace: "work", title: write.action, createdAt: 8, summary: write.reason,
        write: WriteProposal(request: "q-00000001", version: 1, worker: "researcher", action: write.action,
                             target: write.target, content: write.content, hash: write.hash),
        answers: [.approve, .reject, .later])

    func connected(_ bridge: WriteBridge, level: String = "work:13") async throws -> PaneSession {
        bridge.capabilities = ["task-records", level]
        bridge.workspaces = [WorkspaceInfo(name: "assistant"), WorkspaceInfo(name: "work"), WorkspaceInfo(name: "home")]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        await session.loadTasks()
        return session
    }

    private func approves(_ bridge: WriteBridge) -> [String?] {
        bridge.writeAnswers.compactMap {
            if case .updateInboxEntry(_, _, .approve, _, _, _, _, let hash) = $0 { hash } else { nil }
        }
    }

    @Test func theWriterAndTheModeAreListedAtLevel13() async throws {
        let session = try await connected(WriteBridge())
        #expect(session.writer(of: "work")?.worker == "poster")
        #expect(session.writesMode(of: "work") == .review)
        #expect(session.writer(of: "home") == nil)
        #expect(session.writesMode(of: "home") == Writes.none)
        let request = try #require(session.requests(of: session.workList[0]).first)
        #expect(session.writeStatus(Self.write, of: request) == .waiting(item: "i-0000000w"))
    }

    @Test func aLevel12HostListsNoWriterAndApprovesNoWrite() async throws {
        let bridge = WriteBridge()
        let session = try await connected(bridge, level: "work:12")
        #expect(session.writer(of: "work") == nil)
        #expect(session.writesMode(of: "work") == nil)
        let item = try #require(session.inboxItems.first)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.writesUnsupported))) {
            try await session.approveWrite(item, shown: try #require(item.write))
        }
        #expect(approves(bridge).isEmpty, "nothing reached the host")
    }

    /// Approve sends the hash of the content shown, and the writer's Task comes back once.
    @Test func approveSendsTheHashOfTheShownWrite() async throws {
        let bridge = WriteBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        let saved = try await session.approveWrite(item, shown: try #require(item.write))
        #expect(approves(bridge) == [Self.write.hash])
        #expect(saved.task == "t-write01")
        #expect(saved.state == .answered)
        #expect(session.taskRecords.contains { $0.id == "t-write01" && $0.worker == "poster" })
        let request = try #require(session.requests(of: session.workList[0]).first)
        #expect(session.writeStatus(try #require(request.results.first?.writes.first), of: request) == .taskStarted("t-write01"))
        // A second approve, as from another device, returns the same Task.
        let again = try await session.approveWrite(item, shown: try #require(item.write))
        #expect(again.task == "t-write01")
        #expect(session.savedTasks.isEmpty && session.savedItems.isEmpty)
    }

    /// The generic answer path never approves a write: it has no hash to send.
    @Test func answerCannotApproveAWrite() async throws {
        let bridge = WriteBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.writeNotShown))) {
            try await session.answer(item, .approve)
        }
        #expect(approves(bridge).isEmpty)
        #expect(session.inboxItems.first?.state == .open)
    }

    /// A write that is not the listed one, or whose text does not hash to its `hash`, is not sent.
    @Test func approveRefusesAWriteThatIsNotTheListedOne() async throws {
        let bridge = WriteBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        let listed = try #require(item.write)
        var edited = listed
        edited.content += " Also delete the channel."
        var rehashed = edited
        rehashed.hash = WriteHash.of(action: edited.action, target: edited.target, content: edited.content)
        var forged = listed
        forged.hash = "sha256:" + String(repeating: "0", count: 64)
        let refused = PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: PaneSession.writeNotShown))
        for shown in [edited, rehashed, forged] {
            await #expect(throws: refused) { try await session.approveWrite(item, shown: shown) }
        }
        var notWrite = item
        notWrite.kind = .proposal
        await #expect(throws: refused) { try await session.approveWrite(notWrite, shown: listed) }
        #expect(approves(bridge).isEmpty)
    }

    @Test func rejectTakesAReasonAndStartsNoTask() async throws {
        let bridge = WriteBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        let saved = try await session.reject(item, reason: "Wrong channel.")
        #expect(saved.answer == .reject)
        #expect(bridge.writeAnswers.contains {
            if case .updateInboxEntry(_, "i-0000000w", .reject, nil, "Wrong channel.", nil, nil, nil) = $0 { true } else { false }
        })
        #expect(bridge.writerTask == nil)
        let request = try #require(session.requests(of: session.workList[0]).first)
        #expect(session.writeStatus(Self.write, of: request) == .rejected)
    }

    /// Only the screen that shows the whole write offers Approve; a row or a payload-less item never does.
    @Test func approveIsOfferedOnlyWhereTheWriteIsShownWhole() {
        let generic = ItemAnswers(Self.item)
        #expect(!generic.decisions.contains(.approve))
        #expect(generic.main != .approve)
        #expect(generic.decisions == [.reject])
        #expect(generic.later)
        let shown = ItemAnswers(Self.item, showsWrite: true)
        #expect(shown.decisions == [.approve, .reject])
        var bare = Self.item
        bare.write = nil
        #expect(!ItemAnswers(bare, showsWrite: true).decisions.contains(.approve))
        // Other kinds keep Approve.
        #expect(ItemAnswers(StaffTests.handoffItem).decisions.contains(.approve))
    }

    @Test func aWriteFromAnOlderVersionIsMarked() throws {
        var current = Self.request
        let write = try #require(Self.item.write)
        #expect(!write.isOlder(than: current))
        current.version = 2
        #expect(write.isOlder(than: current))
        #expect(!write.isOlder(than: nil))
    }

    /// The hash of `protocol/fixtures/assistant/messages.jsonl`'s second write.
    @Test func theHashMatchesTheHostsFixture() {
        #expect(WriteHash.of(action: "Comment on ENG-12", target: "linear:ENG-12", content: "The fix ships in 2.4.")
                == "sha256:e495150141fce15c0e08a8f84d0164f0eec6531c80e3405b5281cf4a871014c5")
    }
}
