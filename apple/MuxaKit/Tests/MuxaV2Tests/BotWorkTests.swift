import Foundation
import Testing

@testable import MuxaV2

/// A Bot's work grouped by PROTOCOL-v2.md, "A Bot's work" (ADR 0086), against
/// the cases of `Fixtures/bot-work.json`.
struct BotWorkTests {
    struct Case: Decodable {
        var bot: String
        var workspace: String
        var enabled: Bool
        var items: [String]
        var terminals: [String]
        var needsYou: [String]
        var inProgress: [String]
        var planned: [String]
        var recent: [String]
        var status: String
    }

    static let fixture: Data = {
        guard let url = Bundle.module.url(forResource: "bot-work", withExtension: "json", subdirectory: "Fixtures"),
              let data = try? Data(contentsOf: url) else { return Data() }
        return data
    }()

    @Test func eachCaseGroupsAsTheProtocolSays() throws {
        let data = Self.fixture
        let inbox = try Inbox.field(of: data)
        let terminals: [Terminal] = try field("terminals", of: data)
        let routines: [Routine] = try field("routines", of: data)
        let now: Int64 = try field("now", of: data)
        let cases: [Case] = try field("cases", of: data)
        #expect(cases.count == 7)
        for c in cases {
            let w = BotWork(bot: c.bot, workspace: c.workspace, enabled: c.enabled, inbox: inbox, terminals: terminals,
                            routines: routines, now: now)
            let name = "\(c.workspace)/\(c.bot)"
            #expect(w.items.map(\.id) == c.items, "\(name) items")
            #expect(w.terminals.map(\.id) == c.terminals, "\(name) terminals")
            #expect(w.needsYou.map(\.id) == c.needsYou, "\(name) needs you")
            #expect(w.inProgress.map(\.id) == c.inProgress, "\(name) in progress")
            #expect(w.planned.map(\.name) == c.planned, "\(name) planned")
            #expect(w.recent.map(\.id) == c.recent, "\(name) recent")
            #expect(w.status.rawValue == c.status, "\(name) status")
        }
    }

    @Test func memoryHistoryDecodesAndIsOptional() throws {
        let wire = #"{"name":"b","enabled":true,"memory_versions":2,"memory_history":[{"version":1,"at":200},{"version":2,"at":100}]}"#
        let bot = try Coding.plainDecoder.decode(SettingsBot.self, from: Data(wire.utf8))
        #expect(bot.memoryHistory == [MemoryVersion(version: 1, at: 200), MemoryVersion(version: 2, at: 100)])
        let old = try Coding.plainDecoder.decode(SettingsBot.self, from: Data(#"{"name":"b","enabled":true}"#.utf8))
        #expect(old.memoryHistory.isEmpty)
        #expect(!String(decoding: try Coding.plainEncoder.encode(old), as: UTF8.self).contains("memory_history"))
    }

    @Test func aMembersRunNamesItsLead() {
        let t = Terminal(id: "t6", name: nil, live: true, status: nil,
                         labels: ["muxa.kind": "handoff", "muxa.bot": "writer", "muxa.lead": "weekly"], cols: 80, rows: 24)
        #expect(t.lead == "weekly" && t.bot == "writer")
    }
}

/// Which terminals are a Bot's, and so stay off Focus (ADR 0086).
struct BotTerminalTests {
    @Test func aTalkARunAndAMemberBelongToABotAndAPlainTaskDoesNot() {
        func t(_ labels: [String: String]) -> Terminal {
            Terminal(id: "t", name: nil, live: true, status: nil, labels: labels, cols: 80, rows: 24)
        }
        #expect(t(["muxa.kind": "task", "muxa.role": "agent", "muxa.bot": "pr-watch"]).belongsToBot)
        #expect(t(["muxa.kind": "routine", "muxa.role": "agent", "muxa.bot": "pr-watch"]).belongsToBot)
        #expect(t(["muxa.kind": "handoff", "muxa.role": "agent", "muxa.lead": "weekly"]).belongsToBot)
        #expect(!t(["muxa.kind": "task", "muxa.role": "agent"]).belongsToBot)
    }
}
