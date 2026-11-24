import Foundation
import Testing

@testable import MuxaV2

/// The demo server's talks with Bots, settings proposals and the Capture
/// assistant's suggestions (ADR 0083, 0084).
struct DemoTalkTests {
    func open() async throws -> V2Client {
        try await V2Client.open(DemoChannel(), name: "test")
    }

    @Test(.timeLimit(.minutes(1)))
    func applyingAProposalChangesTheSettingsAndMarksItDone() async throws {
        let c = try await open()
        let item = try #require(try await c.inbox().items.first { $0.settingsProposal != nil })
        #expect(item.id == "d12" && item.bot == "pr-watch")
        let inbox = try await c.inbox(.inboxApply(item: item.id))
        #expect(inbox.done.first?.id == "d12")
        let app = try #require(try await c.settings().workspaces.first { $0.name == "app" })
        let sweep = try #require(app.routines.first { $0.name == "nightly-sweep" })
        #expect(!sweep.spec.enabled && sweep.spec.bot == "pr-watch")
        #expect(app.bots.first { $0.name == "pr-watch" }?.spec.commands == ["gh pr list:*", "gh pr diff:*"])
    }

    @Test(.timeLimit(.minutes(1)))
    func aProposalIsAConflictOnceTheSettingsChanged() async throws {
        let c = try await open()
        let settings = try await c.settings()
        _ = try await c.settingsUpdate(revision: settings.revision, ops: [.routinesGate(workspace: "Personal", enabled: true)])
        await #expect(throws: V2Error.refused(code: "conflict",
                                              message: "the settings changed since the Bot proposed this; ask it again")) {
            try await c.inbox(.inboxApply(item: "d12"))
        }
        #expect(try await c.inbox().items.contains { $0.id == "d12" })
        await #expect(throws: V2Error.self) { try await c.inbox(.inboxApply(item: "d1")) }
    }

    @Test(.timeLimit(.minutes(1)))
    func acceptingASuggestionAppliesItAndClearsIt() async throws {
        let c = try await open()
        let item = try #require(try await c.inbox().items.first { $0.suggestion != nil })
        #expect(item.id == "d13" && item.suggestion?.bot == "pr-watch" && item.suggestion?.taskGoal != nil)
        let inbox = try await c.inbox(.inboxSuggestion(item: item.id, accept: true))
        let accepted = try #require(inbox.items.first { $0.id == "d13" })
        #expect(accepted.suggestion == nil)
        #expect(accepted.workspace == "app" && accepted.due == "2026-11-28")
        #expect(accepted.note == "It fails about once in ten CI runs.")
        await #expect(throws: V2Error.self) { try await c.inbox(.inboxSuggestion(item: item.id, accept: true)) }
    }

    @Test(.timeLimit(.minutes(1)))
    func dismissingASuggestionLeavesTheItemAsItWas() async throws {
        let c = try await open()
        let inbox = try await c.inbox(.inboxSuggestion(item: "d13", accept: false))
        let item = try #require(inbox.items.first { $0.id == "d13" })
        #expect(item.suggestion == nil && item.workspace == nil && item.due == nil && item.note == nil)
    }

    @Test(.timeLimit(.minutes(1)))
    func aSplitMakesItsPartsInThePlaceOfTheItem() async throws {
        var content = DemoContent.standard
        let at = try #require(content.inbox.firstIndex { $0.id == "d13" })
        content.inbox[at].suggestion = ItemSuggestion(workspace: "app", split: ["Reproduce it", "Fix it"])
        let c = try await V2Client.open(DemoChannel(content: content), name: "test")
        let inbox = try await c.inbox(.inboxSuggestion(item: "d13", accept: true))
        #expect(Array(inbox.items[at...].prefix(2)).map(\.title) == ["Reproduce it", "Fix it"])
        #expect(inbox.items[at].workspace == "app")
        #expect(inbox.done.first?.id == "d13")
    }

    @Test(.timeLimit(.minutes(1)))
    func aTalkIsATaskThatNamesItsBot() async throws {
        let c = try await open()
        let (t, existing) = try await c.startTask(workspace: "app", title: "Talk to pr-watch", bot: "pr-watch")
        #expect(!existing && t.isTaskAgent && t.bot == "pr-watch")
        await #expect(throws: V2Error.self) {
            try await c.startTask(workspace: "app", title: "Talk to Bad", bot: "Bad Name")
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func theAssistantsSettingsAreEdited() async throws {
        let c = try await open()
        let settings = try await c.settings()
        #expect(settings.assistant?.enabled == true)
        let memory = settings.assistant?.memoryText
        let after = try await c.settingsUpdate(
            revision: settings.revision, ops: [.assistantUpdate(enabled: false, worker: "reviewer", soulText: "New soul.\n")])
        #expect(after.assistant == SettingsAssistant(enabled: false, worker: "reviewer", soulText: "New soul.\n",
                                                     memoryText: memory))
    }

    @Test(.timeLimit(.minutes(1)))
    func aTeamTakesOnlyBotsThatLeadNone() async throws {
        let c = try await open()
        let settings = try await c.settings()
        let triage = BotSpec(name: "triage")
        let lead = BotSpec(name: "pr-watch", members: ["triage"], handoffsPerDay: 4)
        let after = try await c.settingsUpdate(
            revision: settings.revision,
            ops: [.botAdd(workspace: "app", bot: triage), .botUpdate(workspace: "app", bot: lead)])
        #expect(after.workspaces[0].bots.first { $0.name == "pr-watch" }?.spec.members == ["triage"])
        await #expect(throws: V2Error.self) {
            try await c.settingsUpdate(revision: after.revision,
                                       ops: [.botUpdate(workspace: "app", bot: BotSpec(name: "triage", members: ["pr-watch"]))])
        }
    }
}

/// The demo's pr-watch as its Bot page groups it (ADR 0086).
struct DemoBotWorkTests {
    @Test(.timeLimit(.minutes(1)))
    func prWatchHasItemsATalkARunAndAPlannedScan() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let settings = try await c.settings()
        let bot = try #require(settings.workspaces[0].bots.first { $0.name == "pr-watch" })
        #expect(bot.memoryHistory.map(\.version) == [1, 2])
        let w = BotWork(bot: "pr-watch", workspace: "app", enabled: bot.spec.enabled, inbox: try await c.inbox(),
                        terminals: try await c.terminals(), routines: try await c.routines().routines)
        #expect(w.items.map(\.id) == ["d10", "d11", "d12"])
        #expect(w.terminals.map(\.id) == ["t7", "t8"])
        #expect(w.planned.map(\.name) == ["scan"])
        #expect(w.status == .needsYou)
        #expect(try await c.inbox().items.first { $0.id == "d12" }?.terminal == "t7")
    }
}
