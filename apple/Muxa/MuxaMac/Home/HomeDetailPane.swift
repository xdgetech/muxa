import MuxaV2
import SwiftUI

/// The picked item as the desktop mock's detail: where it stands and came
/// from, its title, its live Task, its note, and the actions at the foot.
struct HomeDetailPane: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let item: InboxItem
    /// Where the item stands in the user's order, from 1; nil when done.
    let position: Int?
    var onDone: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    meta
                    Text(item.title)
                        .font(.system(size: 21, weight: .semibold))
                        .lineSpacing(4)
                        .foregroundStyle(Palette.text)
                        .textSelection(.enabled)
                        .accessibilityIdentifier("detail.title")
                    if let task = liveTerminal { taskLine(task) }
                    noteView(item.note).padding(.top, 8)
                    ItemSources(item: item).padding(.top, 8)
                    ItemAssist(item: item, live: position != nil, onDone: onDone)
                    if let error = model.requestError {
                        Text(error).foregroundStyle(Palette.late)
                            .accessibilityIdentifier("detail.error")
                    }
                }
                .frame(maxWidth: 760, alignment: .leading)
                .padding(.horizontal, 28)
                .padding(.vertical, 18)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            HomeActions(item: item, live: position != nil, terminal: liveTerminal, onDone: onDone)
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
    }

    private var meta: some View {
        HStack(spacing: 8) {
            KindChip(kind: item.kind)
            if let position {
                Text("#\(position)").font(.system(size: 12, weight: .semibold)).foregroundStyle(Palette.textMid)
            } else {
                Text("Done").font(.system(size: 12, weight: .semibold)).foregroundStyle(Palette.textMid)
            }
            if let bot = item.bot { BotChip(bot: bot).font(.system(size: 12)) }
            Text("· \(item.origin)")
                .font(.system(size: 12))
                .foregroundStyle(Palette.textSecondary)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let due = item.due {
                let today = NextModel.today()
                Text("due \(DueLabel.short(due, today: today))")
                    .font(.system(size: 12))
                    .foregroundStyle(due < today ? Palette.late : Palette.textSecondary)
            }
        }
        .accessibilityElement(children: .combine)
    }

    /// The Task behind the item, while its agent runs: opens it in Focus.
    private func taskLine(_ t: MuxaV2.Terminal) -> some View {
        HStack(spacing: 10) {
            Text("Task")
            Button { nav.open(t) } label: {
                HStack(spacing: 6) {
                    StateDot(color: t.state.color, size: 7)
                    Text(t.title).foregroundStyle(Palette.text)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(Palette.cardRaised, in: RoundedRectangle(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Palette.borderStrong))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("detail.task")
            if let place = t.placeText { Text(place) }
        }
        .font(.system(size: 12.5))
        .foregroundStyle(Palette.textSecondary)
    }

    @ViewBuilder
    private func noteView(_ note: String?) -> some View {
        if let note {
            MarkdownText(text: note, size: 14, color: Palette.terminalText)
                .accessibilityIdentifier("detail.note")
        } else {
            Text("No note.").font(.system(size: 14)).foregroundStyle(Palette.textTertiary)
        }
    }

    private var liveTerminal: MuxaV2.Terminal? {
        guard let id = item.terminal else { return nil }
        return model.terminals.first { $0.id == id && $0.live }
    }
}
