import MuxaV2
import SwiftUI

/// The Mac's Home (ADR 0066, ADR 0070) as the desktop mock: the header, the
/// Inbox in the user's order beside the picked item, and the keys at the foot.
/// Routines have their own page.
struct HomeView: View {
    @Environment(NextModel.self) private var model
    var openTerminal: (Terminal) -> Void = { _ in }
    var showFocus: () -> Void = {}

    @State private var selected: String?
    @State private var filter = InboxFilter()
    @State private var now = Int64(Date().timeIntervalSince1970)

    var body: some View {
        VStack(spacing: 0) {
            HomeHeader(openTerminal: openTerminal, showFocus: showFocus)
            HStack(spacing: 0) {
                HomeInboxColumn(selected: $selected, filter: $filter, now: now, open: openTask)
                    .column(width: 540, background: Palette.ground)
                detail
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Palette.pane)
            }
            LinkBanner()
            KeyLegend(items: [
                ("↑ ↓", "select"),
                (Shortcut.move.display, "move"),
                ("⏎", "open in Focus"),
                (Shortcut.open.display, "main action"),
                (Shortcut.later.display, "later"),
                (Shortcut.done.display, "done"),
                (Shortcut.capture.display, "capture"),
            ])
        }
        .background(Palette.ground)
        .onAppear(perform: keepSelection)
        .onChange(of: visible.map(\.id)) { keepSelection() }
        .task {
            // Later items come back on their own; the clock decides what shows.
            while !Task.isCancelled {
                now = Int64(Date().timeIntervalSince1970)
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    private var visible: [InboxItem] { filter.apply(model.inbox.items, now: now) }

    @ViewBuilder
    private var detail: some View {
        if let id = selected, let item = (model.inbox.items + model.inbox.done).first(where: { $0.id == id }) {
            let position = model.inbox.items.firstIndex(where: { $0.id == id }).map { $0 + 1 }
            HomeDetailPane(item: item, position: position) { selected = filter.next(after: id, in: visible) }
                .id(item.id)
        } else {
            ContentUnavailableView(
                model.inbox.items.isEmpty ? "Inbox zero" : "Select an item",
                systemImage: "tray")
        }
    }

    /// The mock always shows an item: the first, once the picked one is gone.
    private func keepSelection() {
        let all = model.inbox.items + (filter.showDone ? model.inbox.done : [])
        if let selected, all.contains(where: { $0.id == selected }) { return }
        selected = visible.first?.id
    }

    /// `⏎` on a row: the item's Task in Focus, while its agent runs.
    private func openTask(_ item: InboxItem) {
        guard let id = item.terminal, let t = model.terminals.first(where: { $0.id == id && $0.live }) else { return }
        openTerminal(t)
    }
}
