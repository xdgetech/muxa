import MuxaV2
import SwiftUI

extension ItemKind {
    var title: String {
        switch self {
        case .review: "Review"
        case .reply: "Reply"
        case .decide: "Decide"
        case .read: "Read"
        case .todo: "To do"
        case .failed: "Failed"
        }
    }
}

extension InboxItem {
    /// A Task's Review: Approve closes the Task (ADR 0072).
    var closesTask: Bool { kind == .review && terminal != nil }

    /// A Failed item that knows what made it can run that again.
    var canRetry: Bool { kind == .failed && (terminal != nil || routine != nil) }

    /// Work the user may hand to a new Task. A Task of muxa v1 resumes instead.
    var canStartTask: Bool { [.reply, .decide, .todo].contains(kind) && place == nil }

    /// A Task of muxa v1: its agent starts again in the place it worked.
    var canResumeTask: Bool { place != nil }

    /// The resumed Task's title: the item's, without `Resume: `.
    var resumeTitle: String {
        let prefix = "Resume: "
        return title.hasPrefix(prefix) ? String(title.dropFirst(prefix.count)) : title
    }

    /// Something to read the user may turn into work (docs/next.md, Make it a Task).
    var canMakeTask: Bool { kind == .read }

    /// Where the item came from: `pr-watch · Routine · scan`, `Personal · captured`.
    var origin: String {
        if let routine { return [bot, "Routine · \(routine.name)"].compactMap { $0 }.joined(separator: " · ") }
        let from = switch source {
        case "task": "Task"
        case "routine": "Routine"
        case "v1": "muxa v1"
        default: "captured"
        }
        return [workspace, from].compactMap { $0 }.joined(separator: " · ")
    }
}

/// A due date: `today`, a weekday within the week, or the date. Red once it has passed.
struct DueLabel: View {
    let due: String

    var body: some View {
        let today = NextModel.today()
        let overdue = due < today
        Text(Self.short(due, today: today))
            .foregroundStyle(overdue ? Palette.late : Palette.textSecondary)
            .accessibilityLabel(overdue ? "Overdue \(due)" : "Due \(due)")
    }

    static func short(_ due: String, today: String, calendar: Calendar = .current) -> String {
        if due == today { return "today" }
        let parse = DateFormatter()
        parse.calendar = calendar
        parse.dateFormat = "yyyy-MM-dd"
        guard let date = parse.date(from: due), let now = parse.date(from: today) else { return due }
        let days = calendar.dateComponents([.day], from: now, to: date).day ?? 0
        let format = DateFormatter()
        format.calendar = calendar
        format.setLocalizedDateFormatFromTemplate(days > 0 && days < 7 ? "EEE" : "MMMd")
        return format.string(from: date)
    }
}

/// An Inbox item as the mocks show it: the kind's chip, the title, where it
/// came from and when it is due. The list around it draws the card.
struct ItemRow: View {
    let item: InboxItem
    let now: Int64

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            KindChip(kind: item.kind).padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                Text(item.title)
                    .font(.system(size: 15))
                    .foregroundStyle(Palette.text)
                    .lineLimit(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
                HStack(spacing: 6) {
                    Text(item.origin).lineLimit(1)
                    Spacer(minLength: 0)
                    if item.isLater(now: now) {
                        Image(systemName: "clock").accessibilityLabel("Later")
                    }
                    if let due = item.due { DueLabel(due: due) }
                }
                .font(.system(size: 12.5))
                .foregroundStyle(Palette.textSecondary)
            }
        }
        .frame(minHeight: minTouch)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("item.\(item.id)")
    }
}

/// An item row in its card; picked rows are lighter.
struct ItemCard: View {
    let item: InboxItem
    let now: Int64
    var picked = false

    var body: some View {
        ItemRow(item: item, now: now)
            .padding(12)
            .background(picked ? Palette.rowSelected : Palette.card, in: RoundedRectangle(cornerRadius: 14))
            .contentShape(RoundedRectangle(cornerRadius: 14))
    }
}

/// When a Later item comes back.
enum LaterChoice: CaseIterable, Identifiable {
    case hour, tomorrow, nextWeek

    var id: Self { self }

    var title: String {
        switch self {
        case .hour: "In 1 Hour"
        case .tomorrow: "Tomorrow 9:00"
        case .nextWeek: "Next Monday 9:00"
        }
    }

    func until(from now: Date = .now, calendar: Calendar = .current) -> Int64 {
        let date: Date
        switch self {
        case .hour:
            date = now.addingTimeInterval(3600)
        case .tomorrow:
            let tomorrow = calendar.date(byAdding: .day, value: 1, to: now) ?? now
            date = calendar.date(bySettingHour: 9, minute: 0, second: 0, of: tomorrow) ?? tomorrow
        case .nextWeek:
            let monday = calendar.nextDate(after: now, matching: DateComponents(hour: 9, weekday: 2), matchingPolicy: .nextTime)
            date = monday ?? now.addingTimeInterval(7 * 86400)
        }
        return Int64(date.timeIntervalSince1970)
    }
}

/// Done and Later on an item's swipe, as the mocks color them.
struct ItemSwipeActions: View {
    @Environment(NextModel.self) private var model
    let item: InboxItem

    var body: some View {
        Button("Done", systemImage: "checkmark") { Task { await model.inbox(.inboxDone(item: item.id)) } }
            .tint(Palette.swipeDone)
        Button("Later", systemImage: "clock") {
            Task { await model.inbox(.inboxLater(item: item.id, until: LaterChoice.tomorrow.until())) }
        }
        .tint(Palette.swipeLater)
    }
}
