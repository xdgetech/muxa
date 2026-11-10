import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// A bridge with workspace memory (`work` level 14, ADR 0053), as the host answers: each
/// workspace's mode in `task-list`, and a `memory` item whose approve needs its hash and
/// appends once.
final class MemoryBridge: PlanBridge, @unchecked Sendable {
    /// The texts approve appended to WORKSPACE.md, in order.
    var appended: [String] = []
    /// Approve is refused with this: the file's cap.
    var capRefusal: ErrorBody?
    private let sentLock = NSLock()
    private var _memoryAnswers: [Request] = []
    /// The answers to `memory` items the session sent, in order.
    var memoryAnswers: [Request] { sentLock.withLock { _memoryAnswers } }

    override init() {
        super.init()
        items = [MemoryTests.item]
    }

    override func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        guard case .updateInboxEntry(let id, let entry, let action, _, _, _, _, let hash) = req,
              let i = items.firstIndex(where: { $0.id == entry }), let memory = items[i].memory
        else { return try await super.send(line) }
        sentLock.withLock { _memoryAnswers.append(req) }
        if action == .approve {
            guard hash == memory.hash else {
                let refusal = ErrorBody(code: .badRequest, message: "this app cannot show the whole memory text")
                return emit(.result(id: id, error: refusal, sessions: nil))
            }
            if let capRefusal { return emit(.result(id: id, error: capRefusal, sessions: nil)) }
            if items[i].state == .open {
                appended.append(memory.text)
                workspaces[0].memoryAt = 20
            }
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
struct MemoryTests {
    /// The first item of `protocol/fixtures/items/messages.jsonl`'s memory `inbox-list`.
    nonisolated static let proposal = MemoryProposal(
        task: "t-00b001", title: "Rotate the API keys",
        text: "- API keys live in Vault under kv/work/api; rotate them with `make rotate-keys`.\n"
            + "- Decision: keys rotate every 90 days, because the vendor expires them at 100.\n",
        hash: "sha256:4f4f83d8f4e4547bd05edb2c47b11522c43ac7999f3b8f97a0e14143832aba75")

    nonisolated static let item = InboxItem(
        id: "i-4e5f6a7b", kind: .memory, state: .open,
        question: "Rotate the API keys proposes to add to the workspace memory", workspace: "work",
        title: proposal.title, createdAt: 8, task: proposal.task, memory: proposal, answers: [.approve, .reject, .later])

    func connected(_ bridge: MemoryBridge, level: String = "work:14") async throws -> PaneSession {
        bridge.capabilities = ["task-records", level]
        bridge.workspaces = [
            WorkspaceInfo(name: "work", memory: .review, memoryAt: 10), WorkspaceInfo(name: "home", memory: .auto),
            WorkspaceInfo(name: "notes", memory: .off),
        ]
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        await session.loadTasks()
        return session
    }

    private func approves(_ bridge: MemoryBridge) -> [String?] {
        bridge.memoryAnswers.compactMap {
            if case .updateInboxEntry(_, _, .approve, _, _, _, _, let hash) = $0 { hash } else { nil }
        }
    }

    @Test func theModeAndTheItemAreListedAtLevel14() async throws {
        let session = try await connected(MemoryBridge())
        #expect(session.memoryMode(of: "work") == .review)
        #expect(session.memoryMode(of: "home") == .auto)
        #expect(session.memoryMode(of: "notes") == .off)
        #expect(session.memoryMode(of: "gone") == nil)
        #expect(session.memoryChangedAt(of: "work") == 10)
        #expect(session.memoryChangedAt(of: "home") == nil)
        #expect(session.memoryStatus(of: "t-00b001") == .waiting(item: "i-4e5f6a7b"))
        #expect(session.memoryStatus(of: "t-other") == nil)
    }

    @Test func aLevel13HostHasNoMemory() async throws {
        let bridge = MemoryBridge()
        let session = try await connected(bridge, level: "work:13")
        #expect(session.memoryMode(of: "work") == nil)
        #expect(session.memoryChangedAt(of: "work") == nil)
        #expect(session.memoryStatus(of: "t-00b001") == nil)
        let item = try #require(session.inboxItems.first)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.memoryUnsupported))) {
            try await session.approveMemory(item, shown: Self.proposal)
        }
        #expect(approves(bridge).isEmpty, "nothing reached the host")
    }

    /// Approve sends the hash of the text shown; the host appends once, and the Task's
    /// detail says it was added.
    @Test func approveSendsTheHashOfTheShownText() async throws {
        let bridge = MemoryBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        let saved = try await session.approveMemory(item, shown: try #require(item.memory))
        #expect(approves(bridge) == [Self.proposal.hash])
        #expect(saved.state == .answered && saved.answer == .approve)
        #expect(bridge.appended == [Self.proposal.text])
        #expect(session.memoryStatus(of: "t-00b001") == .added(item: "i-4e5f6a7b"))
        #expect(session.memoryChangedAt(of: "work") == 20, "the workspaces are listed again")
        // A second approve, as from another device, appends nothing more.
        _ = try await session.approveMemory(item, shown: Self.proposal)
        #expect(bridge.appended.count == 1)
    }

    /// The generic answer path never approves a memory item: it has no hash to send.
    @Test func answerCannotApproveAMemoryItem() async throws {
        let bridge = MemoryBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        await #expect(throws: PaneSession.Failure.bridge(ErrorBody(code: .unsupported, message: PaneSession.memoryNotShown))) {
            try await session.answer(item, .approve)
        }
        #expect(approves(bridge).isEmpty)
    }

    /// A text that is not the listed one, or that does not hash to its `hash`, is not sent.
    @Test func approveRefusesATextThatIsNotTheListedOne() async throws {
        let bridge = MemoryBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        var edited = Self.proposal
        edited.text += "- Also: delete the old keys.\n"
        var rehashed = edited
        rehashed.hash = MemoryHash.of(task: edited.task, title: edited.title, text: edited.text)
        var forged = Self.proposal
        forged.hash = "sha256:" + String(repeating: "0", count: 64)
        let refused = PaneSession.Failure.bridge(ErrorBody(code: .conflict, message: PaneSession.memoryNotShown))
        for shown in [edited, rehashed, forged] {
            await #expect(throws: refused) { try await session.approveMemory(item, shown: shown) }
        }
        var notMemory = item
        notMemory.kind = .write
        await #expect(throws: refused) { try await session.approveMemory(notMemory, shown: Self.proposal) }
        #expect(approves(bridge).isEmpty)
    }

    /// The file's cap is a conflict with the sizes: the host's words reach the user, and
    /// the item stays open.
    @Test func theCapKeepsTheHostsReason() async throws {
        let bridge = MemoryBridge()
        let cap = ErrorBody(code: .conflict, message: "WORKSPACE.md would be 17,210 bytes; the cap is 16 KiB.")
        bridge.capRefusal = cap
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        await #expect(throws: PaneSession.Failure.bridge(cap)) { try await session.approveMemory(item, shown: Self.proposal) }
        #expect(session.memoryStatus(of: "t-00b001") == .waiting(item: "i-4e5f6a7b"))
    }

    @Test func rejectAppendsNothing() async throws {
        let bridge = MemoryBridge()
        let session = try await connected(bridge)
        let item = try #require(session.inboxItems.first)
        let saved = try await session.answer(item, .reject)
        #expect(saved.answer == .reject)
        #expect(bridge.appended.isEmpty)
        #expect(session.memoryStatus(of: "t-00b001") == .rejected(item: "i-4e5f6a7b"))
    }

    /// Only the screen that shows the whole text offers Approve.
    @Test func approveIsOfferedOnlyWhereTheTextIsShownWhole() {
        let generic = ItemAnswers(Self.item)
        #expect(generic.decisions == [.reject])
        #expect(generic.main != .approve)
        #expect(ItemAnswers(Self.item, showsWhole: true).decisions == [.approve, .reject])
        var bare = Self.item
        bare.memory = nil
        #expect(!ItemAnswers(bare, showsWhole: true).decisions.contains(.approve))
    }

    /// The hashes of `protocol/fixtures/items/messages.jsonl`'s memory items.
    @Test func theHashMatchesTheHostsFixture() {
        #expect(MemoryHash.of(task: Self.proposal.task, title: Self.proposal.title, text: Self.proposal.text)
                == Self.proposal.hash)
        #expect(MemoryHash.of(task: "t-00b004", title: "Move CI to arm runners",
                              text: "- CI runs on arm64 runners since November 2026. x86 images are no longer built.\n")
                == "sha256:a3610dd52492f10423a18af4eaff467b99d92a5afb62fa72c1897c68747de24c")
    }
}
