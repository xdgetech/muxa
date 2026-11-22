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

    var symbol: String {
        switch self {
        case .review: "checkmark.seal"
        case .reply: "arrowshape.turn.up.left"
        case .decide: "arrow.triangle.branch"
        case .read: "doc.text"
        case .todo: "circle"
        case .failed: "exclamationmark.triangle"
        }
    }

    var tint: Color {
        switch self {
        case .review: .blue
        case .reply: .indigo
        case .decide: .orange
        case .read: .teal
        case .todo: .secondary
        case .failed: .red
        }
    }
}

extension InboxItem {
    /// A Task's Review: Approve closes the Task (ADR 0072).
    var closesTask: Bool { kind == .review && terminal != nil }

    /// A Failed item that knows what made it can run that again.
    var canRetry: Bool { kind == .failed && (terminal != nil || routine != nil) }

    /// Work the user may hand to a new Task.
    var canStartTask: Bool { [.reply, .decide, .todo].contains(kind) }
}

struct KindLabel: View {
    let kind: ItemKind

    var body: some View {
        Label(kind.title, systemImage: kind.symbol)
            .font(.caption.weight(.semibold))
            .foregroundStyle(kind.tint)
    }
}

/// A due date; red once it has passed.
struct DueLabel: View {
    let due: String

    var body: some View {
        let overdue = due < NextModel.today()
        Label(due == NextModel.today() ? "Today" : due, systemImage: "calendar")
            .font(.caption)
            .foregroundStyle(overdue ? Color.red : Color.secondary)
            .accessibilityLabel(overdue ? "Overdue \(due)" : "Due \(due)")
    }
}

struct ItemRow: View {
    let item: InboxItem
    let now: Int64

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                KindLabel(kind: item.kind)
                if let w = item.workspace {
                    Text(w).font(.caption).foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                if item.isLater(now: now) {
                    Image(systemName: "clock").font(.caption).foregroundStyle(.secondary)
                        .accessibilityLabel("Later")
                }
                if let due = item.due { DueLabel(due: due) }
            }
            Text(item.title).lineLimit(2)
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("item.\(item.id)")
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
