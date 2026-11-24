import Foundation
import MuxaV2

/// What the Bots list picked: a Bot of a Workspace, or a Routine without a Bot.
enum BotPick: Hashable {
    case bot(workspace: String, name: String)
    case routine(id: String)
}

/// The tabs of a Bot's page; on the Mac `⌘1`…`⌘5` pick them in Bots.
enum BotTab: Int, CaseIterable {
    case home = 1, chats, routines, files, settings

    var title: String {
        switch self {
        case .home: "Home"
        case .chats: "Chats"
        case .routines: "Routines"
        case .files: "Files"
        case .settings: "Settings"
        }
    }
}

/// What New Bot did: whether the Bot was added, and its first talk.
struct BotCreation {
    var added: Bool
    var talk: Terminal?
    var error: String?
}

/// A Bot of a Workspace as the Bots page shows it: its settings and its work.
struct BotEntry: Identifiable {
    let workspace: SettingsWorkspace
    let bot: SettingsBot
    let work: BotWork

    var id: BotPick { .bot(workspace: workspace.name, name: bot.name) }
    var name: String { bot.name }
}

extension NextModel {
    /// Every Bot of the host's settings, Workspace by Workspace, with its work
    /// grouped as PROTOCOL-v2.md, "A Bot's work", says.
    func botEntries(now: Int64 = Int64(Date().timeIntervalSince1970)) -> [BotEntry] {
        (settings.settings?.workspaces ?? []).flatMap { w in
            w.bots.map { b in
                BotEntry(workspace: w, bot: b, work: BotWork(
                    bot: b.name, workspace: w.name, enabled: b.spec.enabled, inbox: inbox, terminals: terminals,
                    routines: routines, now: now))
            }
        }
    }

    /// Routines no Bot owns: failed or held (paused) ones first.
    var routinesWithoutBot: [Routine] {
        let own = routines.filter { $0.bot == nil }
        func held(_ r: Routine) -> Bool { !r.enabled || r.last?.ok == false }
        return own.filter(held) + own.filter { !held($0) }
    }

    /// The Bot's live talks: Task agents with its `muxa.bot`.
    func talks(of e: BotEntry) -> [Terminal] {
        e.work.terminals.filter { $0.isTaskAgent && $0.bot == e.name }
    }

    /// Its Routines as the live list has them.
    func routines(of e: BotEntry) -> [Routine] {
        routines.filter { $0.bot == e.name && $0.workspace == e.workspace.name }
    }

    /// A new talk; a time in the title keeps it apart from the open ones,
    /// since a folder Task with the same title is the same Task.
    /// `goal` is the first message, as typed on the Bot's Home.
    func startChat(with e: BotEntry, goal: String? = nil, now: Date = .now) async -> Terminal? {
        let stamp = now.formatted(date: .abbreviated, time: .shortened)
        return await startTalk(workspace: e.workspace.name, bot: e.name, title: "Talk to \(e.name) · \(stamp)", goal: goal)
    }

    /// Its most recent live talk, one whose agent is not done: a Bot's page
    /// first opens on it (ADR 0086).
    func liveTalk(of e: BotEntry) -> Terminal? {
        talks(of: e).last { $0.status?.state != "done" }
    }

    /// New Bot (ADR 0086 decision 3): `bot-add`, then its first talk. A talk
    /// that did not start leaves the Bot added and says so.
    func createBot(_ spec: BotSpec, in workspace: String) async -> BotCreation {
        if let failure = await settings.save([.botAdd(workspace: workspace, bot: spec)]) {
            return BotCreation(added: false, talk: nil, error: failure)
        }
        let goal = spec.description.map { "You are \(spec.name): \($0). Ask me what to watch, then propose your Routines." }
        guard let t = await startTalk(workspace: workspace, bot: spec.name, title: "Talk to \(spec.name)", goal: goal)
        else {
            return BotCreation(added: true, talk: nil,
                               error: "\(spec.name) was added, but its chat did not start: \(requestError ?? "unknown error")")
        }
        return BotCreation(added: true, talk: t, error: nil)
    }

    /// Pause or Resume: `bot-update` with the Bot's switch; its Routines follow it.
    func setBot(_ e: BotEntry, enabled: Bool) async {
        var spec = e.bot.spec
        spec.enabled = enabled
        if let failure = await settings.save([.botUpdate(workspace: e.workspace.name, bot: spec)]) {
            requestError = failure
        }
        await refreshRoutines()
    }
}
