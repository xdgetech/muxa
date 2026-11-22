import MuxaV2
import SwiftUI

/// Which Inbox items show: a kind, a Workspace, Later items and Done.
struct InboxFilter {
    var kind: ItemKind?
    var workspace: String?
    var showLater = false
    var showDone = false

    /// Dragging reorders only the whole list: a filtered one has gaps.
    var isActive: Bool { kind != nil || workspace != nil }

    var changesList: Bool { isActive || showLater || showDone }

    func apply(_ items: [InboxItem], now: Int64) -> [InboxItem] {
        items.filter { item in
            (kind == nil || item.kind == kind)
                && (workspace == nil || item.workspace == workspace)
                && (showLater || !item.isLater(now: now))
        }
    }

    /// The item to show after `id` leaves the list.
    func next(after id: String, in visible: [InboxItem]) -> String? {
        guard let at = visible.firstIndex(where: { $0.id == id }) else { return nil }
        return visible.indices.contains(at + 1) ? visible[at + 1].id : (at > 0 ? visible[at - 1].id : nil)
    }
}

/// The filter as an icon menu. `reordering`, where given, adds Reorder.
struct InboxFilterMenu: View {
    @Binding var filter: InboxFilter
    let items: [InboxItem]
    var reordering: Binding<Bool>?

    var body: some View {
        Menu {
            Picker("Kind", selection: $filter.kind) {
                Text("Every kind").tag(ItemKind?.none)
                ForEach(ItemKind.allCases, id: \.self) { Text($0.title).tag(Optional($0)) }
            }
            Picker("Workspace", selection: $filter.workspace) {
                Text("Every Workspace").tag(String?.none)
                ForEach(workspaces, id: \.self) { Text($0).tag(Optional($0)) }
            }
            Toggle("Show Later Items", isOn: $filter.showLater)
            Toggle("Show Done", isOn: $filter.showDone)
            if let reordering, !filter.isActive {
                Toggle("Reorder", isOn: reordering)
            }
        } label: {
            Image(systemName: filter.changesList
                ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
                .font(.system(size: 20))
                .foregroundStyle(filter.changesList ? Palette.accent : Palette.textSecondary)
                .frame(minWidth: minTouch, minHeight: minTouch)
                .contentShape(Rectangle())
                .accessibilityLabel("Filter")
        }
        .accessibilityIdentifier("inbox.filter")
    }

    private var workspaces: [String] {
        Array(Set(items.compactMap(\.workspace))).sorted()
    }
}

extension NextModel {
    /// A drag in the unfiltered list `visible`; the server keeps the order.
    func move(in visible: [InboxItem], from source: IndexSet, to destination: Int) {
        guard let from = source.first, visible.indices.contains(from) else { return }
        let id = visible[from].id
        // `destination` counts the moved row still in place.
        let index = destination > from ? destination - 1 : destination
        Task { await inbox(.inboxMove(item: id, index: index)) }
    }
}

/// `⌥↑` / `⌥↓` move the selected item one place.
struct ReorderKeys: View {
    @Environment(NextModel.self) private var model
    let selected: String?

    var body: some View {
        Group {
            Button("") { step(-1) }.keyboardShortcut(.upArrow, modifiers: .option)
            Button("") { step(1) }.keyboardShortcut(.downArrow, modifiers: .option)
        }
        .opacity(0)
        .accessibilityHidden(true)
    }

    private func step(_ by: Int) {
        guard let id = selected, let at = model.inbox.items.firstIndex(where: { $0.id == id }) else { return }
        let to = at + by
        guard to >= 0, to < model.inbox.items.count else { return }
        Task { await model.inbox(.inboxMove(item: id, index: to)) }
    }
}
