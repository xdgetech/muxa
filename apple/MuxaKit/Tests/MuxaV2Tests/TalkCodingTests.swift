import Foundation
import Testing

@testable import MuxaV2

/// Talks with Bots, their teams and the Capture assistant on the wire
/// (ADR 0083, 0084, 0085).
struct TalkCodingTests {
    func wire(_ r: V2Request) throws -> String { String(decoding: try r.json(id: 1), as: UTF8.self) }

    func plain<T: Encodable>(_ value: T) throws -> String {
        String(decoding: try Coding.plainEncoder.encode(value), as: UTF8.self)
    }

    @Test func theNewRequestsMatchTheSpec() throws {
        #expect(try wire(.inboxApply(item: "i3")) == #"{"id":1,"item":"i3","type":"inbox-apply"}"#)
        #expect(try wire(.inboxSuggestion(item: "i3", accept: false))
            == #"{"accept":false,"id":1,"item":"i3","type":"inbox-suggestion"}"#)
        let talk = try V2Request.startTask(workspace: "w", title: "Talk to pr-watch", bot: "pr-watch").json(id: 2)
        let fields = try JSONSerialization.jsonObject(with: talk) as? [String: Any]
        #expect(fields?["bot"] as? String == "pr-watch")
        let plainTask = try JSONSerialization.jsonObject(with: try V2Request.startTask(workspace: "w", title: "T").json(id: 3))
        #expect((plainTask as? [String: Any])?["bot"] == nil)
    }

    @Test func anItemDecodesWithItsSettingsProposalAndSuggestion() throws {
        let wire = #"""
            {"inbox":{"revision":2,"items":[
            {"id":"i1","kind":"decide","title":"pr-watch proposes: more items","created":5,"source":"task","workspace":"w","bot":"pr-watch",
             "settings_proposal":{"revision":"abc","ops":[
               {"op":"bot-update","workspace":"w","bot":{"name":"pr-watch","enabled":true,"daily_items":5,"members":["triage"],"handoffs_per_day":4}},
               {"op":"routine-add","workspace":"w","routine":{"name":"sweep","instructions":"routines/sweep.md","enabled":false,"bot":"pr-watch","read_only":true,"on":"review"},"instructions_text":"Sweep.\n"}]}},
            {"id":"i2","kind":"todo","title":"flaky test","created":5,"source":"capture",
             "suggestion":{"kind":"todo","workspace":"w","due":"2026-11-28","split":["a","b"],"task_goal":"Fix it","bot":"pr-watch"}}],
            "done":[]}}
            """#
        // `daily_items` from a host before ADR 0091 decodes and is dropped.
        let inbox = try Inbox.field(of: Data(wire.utf8))
        let proposal = try #require(inbox.items[0].settingsProposal)
        #expect(proposal.revision == "abc")
        #expect(proposal.ops == [
            .botUpdate(workspace: "w", bot: BotSpec(name: "pr-watch", members: ["triage"], handoffsPerDay: 4)),
            .routineAdd(workspace: "w",
                        routine: RoutineSpec(name: "sweep", instructions: "routines/sweep.md", enabled: false, bot: "pr-watch",
                                             readOnly: true, on: "review"),
                        instructionsText: "Sweep.\n"),
        ])
        #expect(inbox.items[1].suggestion == ItemSuggestion(
            kind: .todo, workspace: "w", due: "2026-11-28", split: ["a", "b"], taskGoal: "Fix it", bot: "pr-watch"))
        #expect(inbox.items[1].settingsProposal == nil)
    }

    @Test func anOpThisClientDoesNotKnowLeavesTheInboxReadable() throws {
        let wire = #"{"inbox":{"revision":1,"items":[{"id":"i1","kind":"decide","title":"t","created":5,"source":"task","settings_proposal":{"revision":"r","ops":[{"op":"from-the-future"}]}}],"done":[]}}"#
        let inbox = try Inbox.field(of: Data(wire.utf8))
        #expect(inbox.items.map(\.id) == ["i1"])
        #expect(inbox.items[0].settingsProposal == nil)
    }

    @Test func anOlderServersItemsAndSettingsStillDecode() throws {
        let wire = #"{"inbox":{"revision":1,"items":[{"id":"i1","kind":"todo","title":"t","created":5,"source":"capture","suggestion":{}}],"done":[]}}"#
        let item = try Inbox.field(of: Data(wire.utf8)).items[0]
        #expect(item.suggestion == ItemSuggestion())
        let settings = try Coding.plainDecoder.decode(Settings.self, from: Data(#"{"revision":"1"}"#.utf8))
        #expect(settings.assistant == nil)
        let bot = try Coding.plainDecoder.decode(BotSpec.self, from: Data(#"{"name":"b","enabled":true}"#.utf8))
        #expect(bot.members.isEmpty && bot.handoffsPerDay == nil)
    }

    @Test func theDemosInboxCarriesTheProposalOnTheWire() throws {
        let item = try #require(DemoContent.standard.inbox.first { $0.settingsProposal != nil })
        let object = Inbox(revision: 1, items: [item], done: []).wireObject
        let data = try JSONSerialization.data(withJSONObject: ["inbox": object])
        #expect(try Inbox.field(of: data).items == [item])
    }

    @Test func aBotSendsItsTeamOnlyWhenItLeadsOne() throws {
        #expect(try plain(BotSpec(name: "b")) == #"{"enabled":true,"name":"b"}"#)
        #expect(try plain(BotSpec(name: "b", members: ["c"], handoffsPerDay: 3))
            == #"{"enabled":true,"handoffs_per_day":3,"members":["c"],"name":"b"}"#)
    }

    @Test func aRoutineOnReviewHasNoTime() throws {
        let spec = RoutineSpec(name: "r", instructions: "r.md", on: "review")
        #expect(try plain(spec)
            == #"{"direct_writes":false,"enabled":true,"instructions":"r.md","name":"r","on":"review","read_only":false}"#)
        let wire = #"{"routines":[{"workspace":"w","name":"r","enabled":true,"on":"review"}]}"#
        let routines: [Routine] = try field("routines", of: Data(wire.utf8))
        #expect(routines.first?.on == "review" && routines.first?.at == nil)
    }

    @Test func theAssistantsSettingsAndOp() throws {
        let wire = #"{"revision":"1","assistant":{"enabled":true,"worker":"sorter","soul_text":"I sort.","memory_text":"Notes."}}"#
        let settings = try Coding.plainDecoder.decode(Settings.self, from: Data(wire.utf8))
        #expect(settings.assistant == SettingsAssistant(enabled: true, worker: "sorter", soulText: "I sort.", memoryText: "Notes."))
        let cases: [(SettingsOp, String)] = [
            (.assistantUpdate(enabled: false), #"{"enabled":false,"op":"assistant-update"}"#),
            (.assistantUpdate(enabled: true, worker: "sorter", soulText: "I sort.\n"),
             #"{"enabled":true,"op":"assistant-update","soul_text":"I sort.\n","worker":"sorter"}"#),
        ]
        for (op, json) in cases {
            #expect(try plain(op) == json)
            #expect(try Coding.plainDecoder.decode(SettingsOp.self, from: Data(json.utf8)) == op)
        }
        #expect(!"\(SettingsOp.assistantUpdate(enabled: true, soulText: "secret soul"))".contains("secret soul"))
    }

    @Test func aTalkTasksTerminalNamesItsBot() {
        let t = Terminal(id: "t9", name: "Talk", live: true, status: nil,
                         labels: ["muxa.kind": "task", "muxa.role": "agent", "muxa.bot": "pr-watch"], cols: 80, rows: 24)
        #expect(t.bot == "pr-watch")
    }
}
