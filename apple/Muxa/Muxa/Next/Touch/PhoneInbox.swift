import MuxaV2
import SwiftUI

/// The phone's Inbox (PhoneInbox mock): the title and the host, who needs you,
/// Capture, and the items as cards. Swipe for Done and Later; tap for the item.
struct PhoneInbox: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @State private var path: [String] = []
    @State private var filter = InboxFilter()
    @State private var reordering = false

    var body: some View {
        NavigationStack(path: $path) {
            VStack(spacing: 0) {
                header
                list
            }
            .background(Palette.ground)
            .navigationTitle("Inbox")
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: String.self) { id in detail(id) }
            .safeAreaInset(edge: .bottom) { LinkBanner() }
        }
        .tint(Palette.accent)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 10) {
            ScreenTitle(title: "Inbox") {
                HostMenu()
                InboxFilterMenu(filter: $filter, items: model.inbox.items, reordering: $reordering)
            }
            NeedsYouBanner(open: nav.open) { nav.tab = .tasks }
            CaptureField()
        }
        .padding(.horizontal, 18)
        .padding(.top, 8)
        .padding(.bottom, 10)
    }

    private var visible: [InboxItem] { filter.apply(model.inbox.items) }

    private var list: some View {
        List {
            ForEach(visible) { item in
                Button { path.append(item.id) } label: { ItemCard(item: item) }
                    .buttonStyle(.plain)
                    .cardRow()
                    .swipeActions(edge: .trailing) { ItemSwipeActions(item: item) }
            }
            .onMove(perform: filter.isActive ? nil : { model.move(in: visible, from: $0, to: $1) })
            if visible.isEmpty {
                Text(model.inbox.items.isEmpty ? "Nothing to do." : "Nothing matches the filter.")
                    .foregroundStyle(Palette.textSecondary)
                    .cardRow(vertical: 12, horizontal: 18)
            }
            if filter.showDone, !model.inbox.done.isEmpty {
                Section {
                    ForEach(model.inbox.done) { item in
                        Button { path.append(item.id) } label: { ItemCard(item: item) }
                            .buttonStyle(.plain)
                            .cardRow()
                    }
                } header: {
                    GroupHeader(title: "Done", color: TaskState.done.color)
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .environment(\.editMode, .constant(reordering && !filter.isActive ? .active : .inactive))
        .accessibilityIdentifier("inbox.list")
    }

    @ViewBuilder
    private func detail(_ id: String) -> some View {
        if let item = (model.inbox.items + model.inbox.done).first(where: { $0.id == id }) {
            let position = model.inbox.items.firstIndex(where: { $0.id == id }).map { $0 + 1 }
            InboxItemPane(item: item, position: position) { path.removeAll() }
        } else {
            ContentUnavailableView("Not in the Inbox", systemImage: "tray")
        }
    }
}
