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

/// The daily count comes from the host (ADR 0086), not from counting items.
struct BotCountTests {
    @Test func todaysCountIsTheHostsAndAnotherDayIsZero() throws {
        let json = #"{"workspace":"app","bot":"scout","done":1,"deleted":0,"day":"2026-11-23","added":3}"#
        let count = try JSONDecoder().decode(BotCount.self, from: Data(json.utf8))
        #expect(count.addedToday("2026-11-23") == 3)
        #expect(count.addedToday("2026-11-24") == 0)
        let old = try JSONDecoder().decode(BotCount.self, from: Data(#"{"workspace":"app","bot":"scout","done":1,"deleted":0}"#.utf8))
        #expect(old.addedToday("2026-11-23") == nil, "an older server sends no count")
    }
}

/// A Workspace may let the user's claude.ai connectors in (ADR 0087).
struct ClaudeConnectorsTests {
    @Test func theSettingDecodesAndTheOpSendsIt() throws {
        let on = try Coding.plainDecoder.decode(
            SettingsWorkspace.self, from: Data(#"{"name":"w","root":"/w","claude_connectors":true}"#.utf8))
        #expect(on.claudeConnectors)
        let old = try Coding.plainDecoder.decode(SettingsWorkspace.self, from: Data(#"{"name":"w","root":"/w"}"#.utf8))
        #expect(!old.claudeConnectors)
        let op = SettingsOp.workspaceUpdate(workspace: "w", claudeConnectors: true)
        let json = String(decoding: try Coding.plainEncoder.encode(op), as: UTF8.self)
        #expect(json == #"{"claude_connectors":true,"op":"workspace-update","workspace":"w"}"#)
        #expect(try Coding.plainDecoder.decode(SettingsOp.self, from: Data(json.utf8)) == op)
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemoTurnsThemOn() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let s = try await c.settings()
        #expect(s.workspaces[0].claudeConnectors == false)
        let after = try await c.settingsUpdate(revision: s.revision,
                                               ops: [.workspaceUpdate(workspace: "app", claudeConnectors: true)])
        #expect(after.workspaces[0].claudeConnectors)
    }
}

/// `[workspace.bots]`: permission rules for a Workspace's Bots only (ADR 0087).
struct BotRulesTests {
    @Test func theRulesDecodeAndTheOpSendsThem() throws {
        let wire = #"{"name":"w","root":"/w","bot_rules":{"allow":["WebSearch"],"deny":["Bash"]}}"#
        let w = try Coding.plainDecoder.decode(SettingsWorkspace.self, from: Data(wire.utf8))
        #expect(w.botRules == BotRules(allow: ["WebSearch"], deny: ["Bash"]))
        let old = try Coding.plainDecoder.decode(SettingsWorkspace.self, from: Data(#"{"name":"w","root":"/w"}"#.utf8))
        #expect(old.botRules.isEmpty)
        let op = SettingsOp.workspaceUpdate(workspace: "w", botRules: BotRules(allow: ["mcp__claude_ai_Gmail"]))
        let json = String(decoding: try Coding.plainEncoder.encode(op), as: UTF8.self)
        #expect(json == #"{"bot_rules":{"allow":["mcp__claude_ai_Gmail"],"deny":[]},"op":"workspace-update","workspace":"w"}"#)
        #expect(try Coding.plainDecoder.decode(SettingsOp.self, from: Data(json.utf8)) == op)
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemosAppWorkspaceHasRulesAndTheyChange() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let s = try await c.settings()
        #expect(s.workspaces[0].botRules == BotRules(allow: ["WebSearch", "WebFetch"], deny: ["Bash"]))
        let after = try await c.settingsUpdate(
            revision: s.revision, ops: [.workspaceUpdate(workspace: "app", botRules: BotRules(deny: ["Bash", "Write"]))])
        #expect(after.workspaces[0].botRules == BotRules(deny: ["Bash", "Write"]))
    }
}

/// Removing a Bot moves its Routines in the same settings-update.
struct BotRemovalTests {
    @Test func itsRoutinesMoveThenTheBotGoes() throws {
        let scan = RoutineSpec(name: "scan", schedule: "*/30 9-18 * * 1-5", instructions: "routines/scan.md",
                               bot: "pr-watch", readOnly: true)
        let digest = RoutineSpec(name: "digest", at: "09:00", instructions: "routines/digest.md")
        let w = SettingsWorkspace(name: "app", root: "/demo", routines: [
            SettingsRoutine(spec: scan), SettingsRoutine(spec: digest),
        ])
        var moved = scan
        moved.bot = "triage"
        #expect(w.botRemoval("pr-watch", movingRoutinesTo: "triage") == [
            .routineUpdate(workspace: "app", routine: moved), .botRemove(workspace: "app", name: "pr-watch"),
        ])
        moved.bot = nil
        #expect(w.botRemoval("pr-watch", movingRoutinesTo: nil).first == .routineUpdate(workspace: "app", routine: moved))
        #expect(w.botRemoval("triage", movingRoutinesTo: nil) == [.botRemove(workspace: "app", name: "triage")])
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemoAppliesItAllOrNone() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let s = try await c.settings()
        await #expect(throws: V2Error.self) {
            try await c.settingsUpdate(revision: s.revision, ops: [.botRemove(workspace: "app", name: "pr-watch")])
        }
        let after = try await c.settingsUpdate(
            revision: s.revision, ops: s.workspaces[0].botRemoval("pr-watch", movingRoutinesTo: nil))
        #expect(after.workspaces[0].bots.map(\.name) == ["scout"])
        #expect(after.workspaces[0].routines.first { $0.name == "scan" }?.spec.bot == nil)
    }
}

/// A Bot's own shell commands (ADR 0088).
struct BotCommandsTests {
    @Test func commandsGoOnTheWireOnlyWhenThere() throws {
        let b = BotSpec(name: "b", commands: ["/x/collect.sh delivery:*"])
        let op = SettingsOp.botUpdate(workspace: "w", bot: b)
        let json = String(decoding: try Coding.plainEncoder.encode(op), as: UTF8.self)
        #expect(json == #"{"bot":{"commands":["\/x\/collect.sh delivery:*"],"enabled":true,"name":"b"},"op":"bot-update","workspace":"w"}"#)
        #expect(try Coding.plainDecoder.decode(SettingsOp.self, from: Data(json.utf8)) == op)
        #expect(!String(decoding: try Coding.plainEncoder.encode(BotSpec(name: "b")), as: UTF8.self).contains("commands"))
        let old = try Coding.plainDecoder.decode(BotSpec.self, from: Data(#"{"name":"b","enabled":true}"#.utf8))
        #expect(old.commands.isEmpty)
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemosPrWatchHasACommand() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let bot = try #require(try await c.settings().workspaces[0].bots.first { $0.name == "pr-watch" })
        #expect(bot.spec.commands == ["gh pr list:*"])
    }
}

/// Renaming a Bot carries its Routines, teams and items along (ADR 0089).
struct BotRenameTests {
    @Test func theOpIsOnTheWire() throws {
        let op = SettingsOp.botRename(workspace: "w", from: "a", to: "b")
        let json = String(decoding: try Coding.plainEncoder.encode(op), as: UTF8.self)
        #expect(json == #"{"from":"a","op":"bot-rename","to":"b","workspace":"w"}"#)
        #expect(try Coding.plainDecoder.decode(SettingsOp.self, from: Data(json.utf8)) == op)
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemoRenamesTheBotItsRoutinesAndItsItems() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let s = try await c.settings()
        let after = try await c.settingsUpdate(
            revision: s.revision, ops: [.botRename(workspace: "app", from: "pr-watch", to: "reviews")])
        #expect(after.workspaces[0].bots.map(\.name) == ["reviews", "scout"])
        #expect(after.workspaces[0].routines.first { $0.name == "scan" }?.spec.bot == "reviews")
        let inbox = try await c.inbox()
        #expect(inbox.items.filter { $0.bot == "reviews" }.map(\.id) == ["d10", "d11", "d12"])
        #expect(!inbox.items.contains { $0.bot == "pr-watch" })
        await #expect(throws: V2Error.self) {
            try await c.settingsUpdate(revision: after.revision,
                                       ops: [.botRename(workspace: "app", from: "reviews", to: "Bad Name")])
        }
    }
}
