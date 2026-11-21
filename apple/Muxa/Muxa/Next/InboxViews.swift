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

    /// The action `⌘⏎` takes on an item of this kind.
    var mainAction: String {
        switch self {
        case .review: "Approve"
        default: "Done"
        }
    }
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

/// The wide pane: what the item asks, and its actions with their keys.
struct InboxItemPane: View {
    @Environment(NextModel.self) private var model
    let item: InboxItem
    /// Where the item stands in the user's order, from 1; nil when done.
    let position: Int?
    var onDone: () -> Void = {}

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                Text(item.title).font(.title2.weight(.semibold))
                    .accessibilityIdentifier("detail.title")
                if let note = item.note {
                    GroupBox("Summary") {
                        Text(note).frame(maxWidth: .infinity, alignment: .leading).textSelection(.enabled)
                    }
                }
                actions
                if let error = model.requestError {
                    Text(error).font(.callout).foregroundStyle(.red)
                }
            }
            .padding()
            .frame(maxWidth: 760, alignment: .leading)
        }
        .navigationTitle(item.kind.title)
        .navigationBarTitleDisplayMode(.inline)
    }

    private var header: some View {
        HStack(spacing: 12) {
            KindLabel(kind: item.kind)
            if let position { Text("#\(position)").font(.caption).foregroundStyle(.secondary) }
            Text(source).font(.caption).foregroundStyle(.secondary)
            if let w = item.workspace { Text(w).font(.caption).foregroundStyle(.secondary) }
            if let due = item.due { DueLabel(due: due) }
        }
    }

    private var source: String {
        switch item.source {
        case "task": "From a Task"
        case "routine": "From a Routine"
        default: "Captured"
        }
    }

    @ViewBuilder
    private var actions: some View {
        if position == nil {
            Button("Reopen") { Task { await model.inbox(.inboxReopen(item: item.id)) } }
                .buttonStyle(.bordered)
        } else {
            HStack {
                Button(item.kind.mainAction) {
                    Task {
                        await model.inbox(.inboxDone(item: item.id))
                        onDone()
                    }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.return, modifiers: .command)
                .accessibilityIdentifier("detail.main")
                Menu("Later") {
                    ForEach(LaterChoice.allCases) { choice in
                        Button(choice.title) {
                            Task {
                                await model.inbox(.inboxLater(item: item.id, until: choice.until()))
                                onDone()
                            }
                        }
                    }
                }
                .menuStyle(.button)
                .buttonStyle(.bordered)
                .accessibilityIdentifier("detail.later")
                // ⌘L: the usual choice, back tomorrow morning.
                Button("") {
                    Task {
                        await model.inbox(.inboxLater(item: item.id, until: LaterChoice.tomorrow.until()))
                        onDone()
                    }
                }
                .keyboardShortcut("l", modifiers: .command)
                .frame(width: 0, height: 0)
                .opacity(0)
                .accessibilityHidden(true)
                if item.kind.mainAction != "Done" {
                    Button("Done") {
                        Task {
                            await model.inbox(.inboxDone(item: item.id))
                            onDone()
                        }
                    }
                    .buttonStyle(.bordered)
                }
                Spacer()
                Menu {
                    Button("Delete", role: .destructive) {
                        Task {
                            await model.inbox(.inboxDelete(item: item.id))
                            onDone()
                        }
                    }
                } label: {
                    Image(systemName: "ellipsis.circle").accessibilityLabel("More")
                }
            }
            .background {
                Button("") {
                    Task {
                        await model.inbox(.inboxDone(item: item.id))
                        onDone()
                    }
                }
                .keyboardShortcut(.delete, modifiers: .command)
                .opacity(0)
                .accessibilityHidden(true)
            }
        }
    }
}
