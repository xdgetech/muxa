import MuxaV2
import SwiftUI

/// Where the Bots tab goes: a Bot's page, a Routine without a Bot, and from a
/// Bot's page one of its items or its conversations (ADR 0086).
enum BotsDestination: Hashable {
    case bot(workspace: String, name: String)
    case routine(id: String)
    case item(id: String)
    case conversation(terminal: String)
}

extension View {
    /// The pages the Bots tab pushes.
    func botsDestinations() -> some View {
        navigationDestination(for: BotsDestination.self) { BotsDestinationView(destination: $0) }
    }
}

/// One pushed page of the Bots tab.
struct BotsDestinationView: View {
    @Environment(NextModel.self) private var model
    let destination: BotsDestination

    var body: some View {
        switch destination {
        case let .bot(w, n):
            if let e = model.botEntries().first(where: { $0.workspace.name == w && $0.name == n }) {
                BotPage(entry: e)
            } else {
                ContentUnavailableView("This Bot is gone", systemImage: "sparkles")
            }
        case let .routine(id):
            if let r = model.routines.first(where: { $0.id == id }) {
                RoutinePane(routine: r)
            } else {
                ContentUnavailableView("This Routine is gone", systemImage: "clock")
            }
        case let .item(id):
            if let item = (model.inbox.items + model.inbox.done).first(where: { $0.id == id }) {
                let position = model.inbox.items.firstIndex(where: { $0.id == id }).map { $0 + 1 }
                InboxItemPane(item: item, position: position)
            } else {
                ContentUnavailableView("Not in the Inbox", systemImage: "tray")
            }
        case let .conversation(id):
            if let t = model.terminals.first(where: { $0.id == id && $0.live }) {
                BotConversationScreen(terminal: t)
            } else {
                ContentUnavailableView("This chat has ended", systemImage: "bubble.left.and.bubble.right")
            }
        }
    }
}

/// The Bots by Workspace, then the Routines without a Bot (failed and paused
/// first), as cards. The phone pushes the picked one; the tablet shows it beside.
struct BotsList: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    var picked: BotsDestination?
    /// The tablet's chevron-less rows mark the picked one instead.
    var showsChevrons = true
    let pick: (BotsDestination) -> Void

    /// A Bot with a live talk opens on it, whatever tab its page was left on
    /// (ADR 0086): the same route an Inbox item takes, on the phone and the tablet.
    private func open(_ e: BotEntry, as dest: BotsDestination) {
        if let talk = model.liveTalk(of: e), nav.showOnBotPage(talk) { return }
        pick(dest)
    }

    var body: some View {
        let entries = model.botEntries()
        let workspaces = entries.map(\.workspace.name).reduce(into: [String]()) { if !$0.contains($1) { $0.append($1) } }
        List {
            ForEach(workspaces, id: \.self) { w in
                Section {
                    ForEach(entries.filter { $0.workspace.name == w }) { e in
                        let dest = BotsDestination.bot(workspace: w, name: e.name)
                        Button { open(e, as: dest) } label: { BotCard(entry: e, picked: picked == dest, chevron: showsChevrons) }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("bot.\(w).\(e.name)")
                            .cardRow()
                    }
                } header: {
                    GroupHeader(title: w).padding(.leading, 6)
                }
            }
            if entries.isEmpty {
                Text(model.settings.settings == nil ? "Reading the Bots…" : "No Bots yet. Add one with +.")
                    .foregroundStyle(Palette.textSecondary)
                    .cardRow(vertical: 12, horizontal: 18)
            }
            let loose = model.routinesWithoutBot
            if !loose.isEmpty {
                Section {
                    ForEach(loose) { r in
                        let dest = BotsDestination.routine(id: r.id)
                        Button { pick(dest) } label: {
                            HStack(spacing: 8) {
                                RoutineRow(routine: r)
                                if showsChevrons {
                                    Image(systemName: "chevron.right")
                                        .font(.system(size: 12, weight: .bold))
                                        .foregroundStyle(Palette.textTertiary)
                                }
                            }
                            .padding(.horizontal, 14)
                            .padding(.vertical, 4)
                            .background(picked == dest ? Palette.rowSelected : Palette.card, in: RoundedRectangle(cornerRadius: 14))
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .routineElement(r)
                        .cardRow()
                    }
                } header: {
                    GroupHeader(title: "Routines without a Bot", count: loose.count).padding(.leading, 6)
                }
            }
            RoutineProblems()
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .refreshable {
            await model.refreshRoutines()
            await model.settings.load()
        }
        .task {
            await model.refreshRoutines()
            if model.settings.settings == nil { await model.settings.load() }
        }
        .accessibilityIdentifier("bots.list")
    }
}

/// A Bot's card: its mark, its name with a dot when it needs the user or runs,
/// and its status line.
struct BotCard: View {
    let entry: BotEntry
    var picked = false
    var chevron = true

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "sparkles")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(entry.bot.spec.enabled ? Palette.accent : Palette.textTertiary)
                .frame(width: 34, height: 34)
                .background((entry.bot.spec.enabled ? Palette.accent.opacity(0.16) : Palette.pill),
                            in: RoundedRectangle(cornerRadius: 10))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(entry.name).font(.system(size: 16, weight: .semibold)).foregroundStyle(Palette.text)
                    if let color = entry.work.status.dot { StateDot(color: color, size: 8) }
                }
                Text(entry.statusLine).font(.system(size: 13)).foregroundStyle(Palette.textSecondary).lineLimit(1)
            }
            Spacer(minLength: 0)
            if entry.work.status == .needsYou {
                Text("\(entry.work.needsYou.count)")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundStyle(Palette.onAmber)
                    .padding(.horizontal, 7)
                    .padding(.vertical, 2)
                    .background(TaskState.needsYou.color, in: Capsule())
            }
            if chevron {
                Image(systemName: "chevron.right").font(.system(size: 12, weight: .bold)).foregroundStyle(Palette.textTertiary)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(picked ? Palette.rowSelected : Palette.card, in: RoundedRectangle(cornerRadius: 14))
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

extension NextNavigation {
    /// iOS: a Bot's terminal opens on its Bot page, not as a Task (ADR 0086):
    /// a talk on Chat, a run or a member on Terminal. False for any other
    /// terminal, which opens as before.
    func showOnBotPage(_ t: MuxaV2.Terminal) -> Bool {
        guard t.belongsToBot, let workspace = t.labels["muxa.workspace"], let bot = t.bot ?? t.lead else { return false }
        focus = false
        tab = .bots
        taskView = t.isTaskAgent ? .chat : .terminal
        botTabs["\(workspace)/\(bot)"] = .chats
        botRoutedTo = "\(workspace)/\(bot)"
        botRoute = BotRoute(workspace: workspace, bot: bot, terminal: t.id)
        return true
    }
}
