import MuxaV2
import SwiftUI

/// Bots, the Mac's third place beside Home and Focus (ADR 0086): the Bots by
/// Workspace and the Routines without a Bot on the left, the picked one on
/// the right. It replaces the Routines page.
struct BotsView: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(MacChrome.self) private var chrome

    var body: some View {
        let entries = model.botEntries()
        VStack(spacing: 0) {
            MacHeaderBar {
                HeaderBrand()
                ModeSwitch(mode: .bots)
            } trailing: {
                HeaderButton(title: "New Bot", systemImage: "plus") { chrome.showNewBot = true }
                    .disabled(model.settings.settings == nil || model.settings.readOnly)
                    .accessibilityIdentifier("bots.new")
            }
            HStack(spacing: 0) {
                BotList(entries: entries, current: current(entries)) { chrome.showNewBot = true }
                    .column(width: 340)
                detail(entries)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Palette.pane)
            }
            // Chats has no Run now; its own split fills the pane.
            KeyLegend(items: [
                (Shortcut.newChat.display, "new chat"),
                (chrome.botTab == .chats ? "" : Shortcut.runBot.display, "run now"),
                (Shortcut.botTabs.display, "tabs"),
                (Shortcut.bots.display, "Bots"),
            ].filter { !$0.0.isEmpty })
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .background(Palette.ground)
        .sheet(isPresented: Bindable(chrome).showNewBot) { NewBotSheet() }
        .modifier(CloseTaskConfirm())
        .task {
            await model.refreshRoutines()
            await model.settings.load()
        }
    }

    /// The picked Bot or Routine, else the first Bot, else the first Routine.
    private func current(_ entries: [BotEntry]) -> BotPick? {
        switch chrome.botPick {
        case let .bot(w, n)? where entries.contains(where: { $0.workspace.name == w && $0.name == n }):
            return chrome.botPick
        case let .routine(id)? where model.routines.contains(where: { $0.id == id }):
            return chrome.botPick
        default:
            return entries.first?.id ?? model.routinesWithoutBot.first.map { .routine(id: $0.id) }
        }
    }

    @ViewBuilder
    private func detail(_ entries: [BotEntry]) -> some View {
        switch current(entries) {
        case let .bot(w, n)?:
            if let e = entries.first(where: { $0.workspace.name == w && $0.name == n }) {
                BotDetailPane(entry: e).id(e.id)
            }
        case let .routine(id)?:
            if let r = model.routines.first(where: { $0.id == id }) { RoutineDetailPane(routine: r) }
        case nil:
            ContentUnavailableView {
                Label(model.settings.settings == nil ? "Loading Bots" : "No Bots yet", systemImage: "sparkles")
            } description: {
                Text("A Bot owns Routines and starts from its soul and memory. New Bot starts with a chat.")
            }
        }
    }
}
