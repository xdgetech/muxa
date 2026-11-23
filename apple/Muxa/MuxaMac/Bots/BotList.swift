import MuxaV2
import SwiftUI

/// The left column of Bots: each Workspace's Bots with a dot when one needs
/// the user or runs, then the Routines no Bot owns, failed and paused first,
/// the Workspaces whose Routines are turned off, and New Bot.
struct BotList: View {
    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    let entries: [BotEntry]
    let current: BotPick?
    let newBot: () -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(workspaces, id: \.self) { w in
                    let bots = entries.filter { $0.workspace.name == w }
                    label(w, count: bots.count)
                    ForEach(bots) { e in
                        BotListRow(entry: e, selected: current == e.id) { chrome.botPick = e.id }
                    }
                }
                let loose = model.routinesWithoutBot
                if !loose.isEmpty {
                    label("Routines without a Bot", count: loose.count)
                    ForEach(loose) { r in
                        RoutineListRow(routine: r, selected: current == .routine(id: r.id)) {
                            chrome.botPick = .routine(id: r.id)
                        }
                    }
                }
                RoutinesOffSection()
                Button(action: newBot) {
                    Text("+ New Bot")
                        .foregroundStyle(Palette.accent)
                        .padding(.horizontal, 10)
                        .frame(maxWidth: .infinity, minHeight: 36, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(model.settings.settings == nil || model.settings.readOnly)
                .accessibilityIdentifier("bots.add")
                if let error = model.requestError {
                    Text(error).font(.system(size: 12)).foregroundStyle(Palette.late).padding(8)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 14)
        }
    }

    private var workspaces: [String] {
        entries.map(\.workspace.name).reduce(into: []) { if !$0.contains($1) { $0.append($1) } }
    }

    private func label(_ title: String, count: Int) -> some View {
        SectionLabel(title: title, count: count)
            .padding(.horizontal, 8)
            .padding(.top, 8)
            .padding(.bottom, 4)
    }
}

/// A Bot's 48 pt row: its mark, its name with a dot, and its status line.
struct BotListRow: View {
    let entry: BotEntry
    let selected: Bool
    let pick: () -> Void

    var body: some View {
        Button(action: pick) {
            HStack(spacing: 10) {
                BotMark(paused: !entry.bot.spec.enabled)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(entry.name).font(.system(size: 13.5, weight: .medium)).foregroundStyle(Palette.text)
                        if let color = entry.work.status.dot { StateDot(color: color, size: 7) }
                    }
                    Text(entry.statusLine)
                        .font(.system(size: 12))
                        .foregroundStyle(Palette.textTertiary)
                }
                .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
            .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
            .selectableRow(selected)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("bot.\(entry.workspace.name).\(entry.name)")
    }
}

/// The sparkle tile that marks a Bot.
struct BotMark: View {
    var paused = false
    var size: CGFloat = 28

    var body: some View {
        Image(systemName: "sparkles")
            .font(.system(size: size * 0.45, weight: .semibold))
            .foregroundStyle(paused ? Palette.textTertiary : Palette.accent)
            .frame(width: size, height: size)
            .background((paused ? Palette.pill : Palette.accent.opacity(0.16)), in: RoundedRectangle(cornerRadius: size * 0.28))
            .accessibilityHidden(true)
    }
}

extension BotWork.Status {
    var title: String {
        switch self {
        case .needsYou: "Needs you"
        case .running: "Running"
        case .paused: "Paused"
        case .idle: "Idle"
        }
    }

    /// The list's dot: amber when it needs the user, green while it runs.
    var dot: Color? {
        switch self {
        case .needsYou: TaskState.needsYou.color
        case .running: Palette.okText
        case .paused, .idle: nil
        }
    }

    var badgeForeground: Color {
        switch self {
        case .needsYou: TaskState.needsYou.color
        case .running: Palette.okText
        case .paused, .idle: Palette.textSecondary
        }
    }

    var badgeBackground: Color {
        switch self {
        case .needsYou: Palette.bannerBack
        case .running: Palette.okBack
        case .paused, .idle: Palette.pill
        }
    }
}

extension BotEntry {
    /// `2 need you · next scan 16:30`, `Running · 1 run`, `Paused`, `Idle · On review`.
    var statusLine: String {
        var parts: [String]
        switch work.status {
        case .needsYou: parts = ["\(work.needsYou.count) need\(work.needsYou.count == 1 ? "s" : "") you"]
        case .running: parts = [work.terminals.count == 1 ? "Running" : "Running · \(work.terminals.count)"]
        case .paused: return "Paused"
        case .idle: parts = ["Idle"]
        }
        if let next = work.planned.first { parts.append(next.plannedText) }
        return parts.joined(separator: " · ")
    }
}

extension Routine {
    /// `next scan Nov 23, 16:30`, or `scan on review`.
    var plannedText: String {
        if let due = nextDue { return "next \(name) \(routineStamp(due))" }
        return "\(name) on review"
    }
}
