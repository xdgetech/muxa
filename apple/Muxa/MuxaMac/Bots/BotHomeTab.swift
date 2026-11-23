import MuxaV2
import SwiftUI

/// A Bot's Home (ADR 0086): its tiles, then Needs you, In progress, Planned
/// and Recent. An item opens in Home, a terminal in Focus.
struct BotHomeTab: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let entry: BotEntry
    let newChat: () -> Void

    private var work: BotWork { entry.work }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            let tiles = tiles
            if !tiles.isEmpty {
                LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: tiles.count), spacing: 10) {
                    ForEach(tiles, id: \.0) { FactCard(label: $0.0, value: $0.1) }
                }
            }
            if model.routines(of: entry).isEmpty { noRoutines }
            if !work.needsYou.isEmpty {
                section("Needs you", count: work.needsYou.count, color: TaskState.needsYou.color, id: "bot.home.needsyou") {
                    ForEach(work.needsYou) { e in
                        switch e {
                        case let .item(i): itemRow(i)
                        case let .terminal(t): terminalRow(t)
                        }
                    }
                }
            }
            if !work.inProgress.isEmpty {
                section("In progress", count: work.inProgress.count, color: Palette.okText, id: "bot.home.inprogress") {
                    ForEach(work.inProgress) { terminalRow($0) }
                }
            }
            if !work.planned.isEmpty {
                section("Planned", count: nil, color: Palette.textTertiary, id: "bot.home.planned") {
                    ForEach(work.planned) { plannedRow($0) }
                }
            }
            if !work.recent.isEmpty {
                section("Recent", count: work.recent.count, color: Palette.textTertiary, id: "bot.home.recent") {
                    ForEach(work.recent.prefix(10)) { itemRow($0, done: true) }
                }
            }
        }
    }

    // MARK: Tiles

    /// Next run, last run, items today and when the memory changed; a tile
    /// without data is left out.
    private var tiles: [(String, String)] {
        var out: [(String, String)] = []
        if let next = work.planned.first {
            out.append(("Next run", next.nextDue.map { "\(next.name) · \(routineStamp($0))" } ?? "\(next.name) · On review"))
        }
        if let last = model.routines(of: entry).compactMap({ r in r.last.map { (r, $0) } }).max(by: { $0.1.at < $1.1.at }) {
            var parts = [routineStamp(last.1.at), last.1.ok ? "OK" : "Failed"]
            if let n = last.1.published { parts.append(n == 1 ? "1 item" : "\(n) items") }
            out.append(("Last run", parts.joined(separator: " · ")))
        }
        if let today = itemsToday { out.append(("Items today", "\(today) of \(entry.bot.spec.dailyItems ?? 3)")) }
        if let at = entry.bot.memoryHistory.first?.at {
            out.append(("Memory", "updated \(routineStamp(at))"))
        }
        return out
    }

    /// The host's count against the daily limit today. Its "N more" item and
    /// its talks' proposals do not count, so it never passes the limit. Nil
    /// from a server that does not send it.
    private var itemsToday: Int? {
        let today = Date.now.formatted(.iso8601.year().month().day())
        let count = model.inbox.bots?.first { $0.workspace == entry.workspace.name && $0.bot == entry.name }
        return count?.addedToday(today).map(Int.init)
    }

    private var noRoutines: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("No Routines yet").font(.system(size: 14, weight: .semibold))
                Text("Tell \(entry.name) in a chat what to watch. It proposes Routines you apply.")
                    .foregroundStyle(Palette.textSecondary)
            }
            Spacer(minLength: 0)
            Button("Open chat", action: newChat)
                .buttonStyle(ActionButtonStyle(kind: .primary, compact: true))
                .accessibilityIdentifier("bot.home.openchat")
        }
        .padding(14)
        .raisedCard()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("bot.home.noroutines")
    }

    // MARK: Rows

    private func section(_ title: String, count: Int?, color: Color, id: String,
                         @ViewBuilder rows: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            SectionLabel(title: title, count: count, color: color).padding(.bottom, 4)
            rows()
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(id)
    }

    private func itemRow(_ item: InboxItem, done: Bool = false) -> some View {
        Button {
            nav.showItem = item.id
            nav.focus = false
            nav.tab = .inbox
        } label: {
            BotRow {
                if done { Badge(text: "Done") } else { KindChip(kind: item.kind) }
            } title: {
                Text(item.title).foregroundStyle(done ? Palette.textSecondary : Palette.text)
            } subtitle: {
                done ? nil : item.origin
            } trailing: {
                Text(done ? "" : "Open in Home")
            }
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("bot.item.\(item.id)")
    }

    private func terminalRow(_ t: MuxaV2.Terminal) -> some View {
        Button { nav.open(t) } label: {
            BotRow {
                StateDot(color: t.state.color, size: 8).frame(width: 58)
            } title: {
                Text(t.title)
            } subtitle: {
                t.botRole(of: entry.name)
            } trailing: {
                Text(t.isTaskAgent ? "Open chat" : "Answer it")
            }
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("bot.terminal.\(t.id)")
    }

    private func plannedRow(_ r: Routine) -> some View {
        BotRow {
            Color.clear.frame(width: 58, height: 1)
        } title: {
            Text(r.name)
        } subtitle: {
            r.nextDue.map { "next \(routineStamp($0)) · \(r.timing)" } ?? "On review"
        } trailing: {
            EmptyView()
        }
    }
}

/// A row of a Bot's lists: a leading mark, a title over a subtitle, and a hint.
struct BotRow<Leading: View, Title: View, Trailing: View>: View {
    @ViewBuilder var leading: Leading
    @ViewBuilder var title: Title
    var subtitle: () -> String?
    @ViewBuilder var trailing: Trailing

    init(@ViewBuilder leading: () -> Leading, @ViewBuilder title: () -> Title, subtitle: @escaping () -> String?,
         @ViewBuilder trailing: () -> Trailing) {
        self.leading = leading()
        self.title = title()
        self.subtitle = subtitle
        self.trailing = trailing()
    }

    var body: some View {
        HStack(spacing: 10) {
            leading
            VStack(alignment: .leading, spacing: 2) {
                title.font(.system(size: 13.5)).lineLimit(1)
                if let s = subtitle() {
                    Text(s).font(.system(size: 12)).foregroundStyle(Palette.textTertiary).lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            trailing.font(.system(size: 12)).foregroundStyle(Palette.textTertiary)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .frame(minHeight: 44)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

extension MuxaV2.Terminal {
    /// What the terminal is to the Bot: `Chat`, `Run of scan`, `research, a member`, with its state.
    func botRole(of bot: String) -> String {
        let role: String
        if lead == bot {
            role = "\(self.bot ?? "a member"), a member"
        } else if isTaskAgent {
            role = "Chat"
        } else if let r = routineName {
            role = "Run of \(r)"
        } else {
            role = labels["muxa.kind"] == "handoff" ? "Run as a member" : "Run"
        }
        guard let state = status?.state else { return role }
        return "\(role) · \(TaskState(state).title.lowercased())"
    }
}
