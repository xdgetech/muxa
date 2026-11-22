import MuxaV2
import SwiftUI

/// The tablet (Tablet mock): a sidebar with Inbox | Tasks | Routines, and the
/// picked item, Task or Routine beside it.
struct TabletRoot: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @State private var selectedItem: String?
    @State private var selectedRoutine: String?
    @State private var filter = InboxFilter()
    @State private var now = Int64(Date().timeIntervalSince1970)

    /// The mock's sidebar width; a portrait iPad mini keeps more for the terminal.
    private static let sidebarWidth: CGFloat = 360

    var body: some View {
        GeometryReader { geo in
            HStack(spacing: 0) {
                TabletSidebar(
                    mode: mode, selectedItem: $selectedItem, selectedRoutine: $selectedRoutine,
                    filter: $filter, now: now)
                    .frame(width: min(Self.sidebarWidth, max(300, geo.size.width * 0.4)))
                Rectangle().fill(Palette.border).frame(width: 1)
                main.frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Palette.ground)
        .tint(Palette.accent)
        .task {
            // Later items come back on their own; the clock decides what shows.
            while !Task.isCancelled {
                now = Int64(Date().timeIntervalSince1970)
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    /// The sidebar's segment: Tasks while a Task is open, else the Inbox or Routines.
    private var mode: Binding<NextNavigation.Tab> {
        Binding(
            get: { nav.focus ? .tasks : (nav.tab == .routines ? .routines : .inbox) },
            set: { tab in
                if tab == .tasks {
                    nav.focus = true
                } else {
                    nav.focus = false
                    nav.tab = tab
                }
            })
    }

    @ViewBuilder
    private var main: some View {
        switch mode.wrappedValue {
        case .tasks:
            if let t = model.terminals.first(where: { $0.id == nav.terminal }) {
                TabletTaskView(terminal: t).id(t.id)
            } else {
                ContentUnavailableView("Select a Task", systemImage: "terminal")
            }
        case .routines:
            if let id = selectedRoutine, let r = model.routines.first(where: { $0.id == id }) {
                RoutinePane(routine: r)
            } else {
                ContentUnavailableView("Select a Routine", systemImage: "clock")
            }
        default:
            if let id = selectedItem, let item = (model.inbox.items + model.inbox.done).first(where: { $0.id == id }) {
                let position = model.inbox.items.firstIndex(where: { $0.id == id }).map { $0 + 1 }
                let visible = filter.apply(model.inbox.items, now: now)
                InboxItemPane(item: item, position: position) { selectedItem = filter.next(after: id, in: visible) }
                    .id(item.id)
            } else {
                ContentUnavailableView(model.inbox.items.isEmpty ? "Inbox zero" : "Select an item", systemImage: "tray")
            }
        }
    }
}
