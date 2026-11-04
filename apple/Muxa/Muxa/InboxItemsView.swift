import MuxaProtocol
import MuxaSession
import SwiftUI

/// The Inbox as a list of questions (ADR 0033), like a mail list: the question, then
/// the source and the age, with a dot while unread. The host's order is kept. A
/// phone pushes the item; a tablet shows it beside the list and moves on after an
/// answer. The layout follows the window's size classes (ADR 0012), not the
/// sheet's: an iPad's sheet reports compact width.
struct InboxItemsView: View {
    @Environment(AppModel.self) private var model
    private var phoneLayout: Bool { model.phoneInbox }
    @Binding var focused: String?
    @Binding var path: [InboxRoute]
    /// The empty Inbox links to All Tasks.
    var showAllTasks: () -> Void
    @State private var later: InboxItem?

    var body: some View {
        let items = model.session.openItems
        Group {
            if phoneLayout {
                list(items)
            } else {
                HStack(spacing: 0) {
                    list(items).frame(width: 300)
                    Divider()
                    detail(items)
                }
            }
        }
        .onChange(of: items.map(\.id)) { old, ids in
            guard let focused, !ids.contains(focused) else { return }
            self.focused = Self.next(after: focused, in: old, remaining: ids)
        }
        .itemLater($later)
        .background { shortcuts(items) }
    }

    private func list(_ items: [InboxItem]) -> some View {
        List {
            if let error = model.session.workError {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
            }
            ForEach(items) { item in
                Button { open(item) } label: { InboxItemRow(item: item) }
                    .foregroundStyle(.primary)
                    .listRowBackground(!phoneLayout && item.id == current(items)?.id ? Color.accentColor.opacity(0.12) : nil)
                    .accessibilityIdentifier("item-row-\(item.id)")
                    .swipeActions(edge: .trailing) {
                        if item.answers.contains(.later) {
                            Button("Later", systemImage: Icon.later) { later = item }.tint(.indigo)
                        }
                    }
            }
        }
        .listStyle(.plain)
        .accessibilityIdentifier("inbox-list")
        .overlay {
            if items.isEmpty {
                ContentUnavailableView {
                    Label("Nothing needs you", systemImage: "checkmark.circle")
                } description: {
                    Text("All Tasks keeps every Task, running and closed.")
                } actions: {
                    Button("All Tasks", action: showAllTasks).accessibilityIdentifier("inbox-all-tasks")
                }
                .accessibilityIdentifier("inbox-empty")
            }
        }
    }

    @ViewBuilder
    private func detail(_ items: [InboxItem]) -> some View {
        if let item = current(items) {
            InboxItemScreen(id: item.id, pushed: false) { saved in focused = Self.next(after: saved.id, in: items, remaining: model.session.openItems.map(\.id)) }
                .id(item.id)
                .frame(maxWidth: .infinity)
        } else {
            Color.clear.frame(maxWidth: .infinity)
        }
    }

    /// The focused item, else the first.
    private func current(_ items: [InboxItem]) -> InboxItem? {
        items.first { $0.id == focused } ?? items.first
    }

    private func open(_ item: InboxItem) {
        focused = item.id
        if phoneLayout { path.append(.item(item.id)) }
    }

    /// After an answer, the item that took the answered one's place.
    static func next(after id: String, in old: [InboxItem], remaining: [String]) -> String? {
        next(after: id, in: old.map(\.id), remaining: remaining)
    }

    static func next(after id: String, in old: [String], remaining: [String]) -> String? {
        guard !remaining.isEmpty else { return nil }
        let index = old.firstIndex(of: id) ?? 0
        let later = old.dropFirst(index + 1).first { remaining.contains($0) }
        return later ?? remaining.last
    }

    /// Cmd+Up/Down move between items; Return opens one on a phone, where the list is alone.
    @ViewBuilder private func shortcuts(_ items: [InboxItem]) -> some View {
        Group {
            Button("Previous Item") { step(items, -1) }.keyboardShortcut(.upArrow)
            Button("Next Item") { step(items, 1) }.keyboardShortcut(.downArrow)
            if phoneLayout, path.isEmpty, let item = current(items) {
                Button("Open Item") { open(item) }.keyboardShortcut(.return, modifiers: [])
            }
        }
        .hidden()
    }

    private func step(_ items: [InboxItem], _ offset: Int) {
        guard !items.isEmpty else { return }
        let index = items.firstIndex { $0.id == focused } ?? 0
        let id = items[min(max(index + offset, 0), items.count - 1)].id
        focused = id
        // A pushed item follows, so the keys move through items on a phone too.
        if phoneLayout, !path.isEmpty { path = [.item(id)] }
    }
}

/// One row: the question, then the source and the age. The dot marks unread.
struct InboxItemRow: View {
    let item: InboxItem

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Circle()
                .fill(item.isUnread ? Color.accentColor : .clear)
                .frame(width: 8, height: 8)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.question).font(item.isUnread ? .headline : .body).lineLimit(2)
                Text(item.source()).font(.caption).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityValue(item.isUnread ? "Unread" : "")
    }
}
