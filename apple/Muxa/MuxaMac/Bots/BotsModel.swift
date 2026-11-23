import Foundation
import MuxaV2

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
        return own.filter { $0.group != .running } + own.filter { $0.group == .running }
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
    func startChat(with e: BotEntry, now: Date = .now) async -> Terminal? {
        let stamp = now.formatted(date: .abbreviated, time: .shortened)
        return await startTalk(workspace: e.workspace.name, bot: e.name, title: "Talk to \(e.name) · \(stamp)")
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
