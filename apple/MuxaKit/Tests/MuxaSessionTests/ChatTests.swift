import Foundation
import MuxaProtocol
import MuxaTransport
import Testing

@testable import MuxaSession

private func entry(_ id: String, _ text: String = "") -> ChatEntry {
    ChatEntry(id: id, kind: .user, text: text.isEmpty ? id : text)
}

struct ChatLogTests {
    @Test func resetsUpsertsAndDropsWhatWasAlreadyCovered() {
        var log = ChatLog(pane: "%1")
        log.apply(ChatUpdate(chat: 1, seq: 1, reset: true, entries: [entry("a"), entry("b")],
                             status: Agent(state: .running), agent: "claude-code", truncated: true))
        #expect(log.entries.map(\.id) == ["a", "b"])
        #expect(log.agent == "claude-code")
        #expect(log.truncated)
        #expect(log.loaded)

        log.apply(ChatUpdate(chat: 1, seq: 2, entries: [entry("b", "B"), entry("c")]))
        #expect(log.entries.map(\.text) == ["a", "B", "c"])
        #expect(log.status == nil)

        // A late duplicate of an earlier message changes nothing.
        log.apply(ChatUpdate(chat: 1, seq: 2, entries: [entry("d")]))
        #expect(log.entries.map(\.id) == ["a", "b", "c"])

        log.apply(ChatUpdate(chat: 1, seq: 3, reset: true, entries: [entry("z")]))
        #expect(log.entries.map(\.id) == ["z"])
        #expect(!log.truncated)
    }

    @Test func thePromptIsTheLatestUpdates() throws {
        var log = ChatLog(pane: "%1")
        let prompt = ChatPrompt(entry: "t:1", kind: .permission, options: [.init(label: "Allow", keys: ["1"])])
        log.apply(ChatUpdate(chat: 1, seq: 1, reset: true, entries: [], status: Agent(state: .waiting), prompt: prompt))
        #expect(log.prompt == prompt)
        log.apply(ChatUpdate(chat: 1, seq: 2, entries: [], status: Agent(state: .running)))
        #expect(log.prompt == nil)

        // A kind this client does not know still decodes, so the update is kept.
        let line = #"{"entry":"t:2","kind":"sandbox","options":[{"label":"Go","keys":["1"],"cancel":true}]}"#
        let decoded = try JSONDecoder().decode(ChatPrompt.self, from: Data(line.utf8))
        #expect(decoded.kind == .other("sandbox"))
        #expect(decoded.options.first?.cancel == true)
    }
}

@MainActor
struct ChatSessionTests {
    func connected(_ bridge: FakeBridge) async throws -> PaneSession {
        let session = PaneSession()
        try await session.connect {
            bridge.start()
            return bridge
        }
        return session
    }

    private func chatIds(_ bridge: FakeBridge) -> [UInt64] {
        bridge.received.compactMap { if case .chat(let id, _) = $0 { id } else { nil } }
    }

    @Test func followsAChatAndDropsAnEarlierOne() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["chat"]
        let session = try await connected(bridge)
        await session.follow(pane: "%1")
        let first = try #require(chatIds(bridge).last)
        bridge.emit(.chatEntries(ChatUpdate(chat: first, seq: 1, reset: true, entries: [entry("a")])))
        try await waitUntil { session.chat?.entries.map(\.id) == ["a"] }

        // After a new chat, the old one's messages are ignored.
        await session.unfollow()
        await session.follow(pane: "%1")
        let second = try #require(chatIds(bridge).last)
        bridge.emit(.chatEntries(ChatUpdate(chat: first, seq: 2, entries: [entry("stale")])))
        bridge.emit(.chatEntries(ChatUpdate(chat: second, seq: 1, reset: true, entries: [entry("new")])))
        try await waitUntil { session.chat?.entries.map(\.id) == ["new"] }

        bridge.emit(.chatEnded(chat: second, reason: "Claude Code exited"))
        try await waitUntil { session.chat?.ended == "Claude Code exited" }
    }

    @Test func anUnsupportedPaneIsRememberedAndNoChatWithoutTheCapability() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["chat"]
        bridge.unsupportedChats = ["%9"]
        let session = try await connected(bridge)
        await session.follow(pane: "%9")
        #expect(session.chat == nil)
        #expect(session.unsupportedChats == ["%9"])
        await session.follow(pane: "%9")
        #expect(chatIds(bridge).count == 1, "an unsupported pane is not asked again")

        let old = FakeBridge()
        let plain = try await connected(old)
        await plain.follow(pane: "%1")
        #expect(plain.chat == nil)
        #expect(chatIds(old).isEmpty)
    }

    @Test func aReconnectFollowsTheSameChatAgain() async throws {
        let bridge = FakeBridge()
        bridge.capabilities = ["chat"]
        let session = try await connected(bridge)
        try await session.watch(pane: "%1")
        await session.follow(pane: "%1")

        let next = FakeBridge()
        next.capabilities = ["chat"]
        try await session.connect {
            next.start()
            return next
        }
        #expect(chatIds(next).count == 1)
        #expect(session.chat?.pane == "%1")
    }
}
