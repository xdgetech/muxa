import MuxaV2
import SwiftUI

/// The tablet (Tablet mock): a sidebar with Inbox | Tasks | Bots, and the
/// picked item, Task or Bot beside it (ADR 0086). Settings has its index in the
/// sidebar and the picked page beside it.
struct TabletRoot: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @State private var selectedItem: String?
    @State private var selectedBot: BotsDestination?
    /// What a Bot's page pushed: its items and conversations.
    @State private var botPath: [BotsDestination] = []
    @State private var selectedSetting: SettingsPlace?
    @State private var filter = InboxFilter()

    /// The mock's sidebar width; a portrait iPad mini keeps more for the terminal.
    private static let sidebarWidth: CGFloat = 360

    var body: some View {
        GeometryReader { geo in
            HStack(spacing: 0) {
                TabletSidebar(
                    mode: mode, selectedItem: $selectedItem, selectedBot: $selectedBot, botPath: $botPath,
                    selectedSetting: $selectedSetting, filter: $filter)
                    .frame(width: min(Self.sidebarWidth, max(300, geo.size.width * 0.4)))
                Rectangle().fill(Palette.border).frame(width: 1)
                main.frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Palette.ground)
        .tint(Palette.accent)
        .onChange(of: nav.botRoute, initial: true) { _, route in
            // Another place asked for a Bot's page, or one of its terminals on it.
            guard let route else { return }
            selectedBot = .bot(workspace: route.workspace, name: route.bot)
            botPath = route.terminal.map { [.conversation(terminal: $0)] } ?? []
            nav.botRoute = nil
        }
    }

    /// The sidebar's segment: Tasks while a Task is open, else the Inbox, Bots or Settings.
    private var mode: Binding<NextNavigation.Tab> {
        Binding(
            get: { nav.focus ? .tasks : ([.bots, .hosts].contains(nav.tab) ? nav.tab : .inbox) },
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
            } else if let p = model.pausedRecord(nav.terminal) {
                PausedTaskScreen(paused: p).id(p.id)
            } else {
                ContentUnavailableView("Select a Task", systemImage: "terminal")
            }
        case .hosts:
            if let place = selectedSetting {
                NavigationStack { SettingsDetail(place: place) }.id(place)
            } else {
                ContentUnavailableView("Select a Workspace", systemImage: "gearshape")
            }
        case .bots:
            NavigationStack(path: $botPath) {
                Group {
                    if let selectedBot {
                        BotsDestinationView(destination: selectedBot)
                    } else {
                        ContentUnavailableView("Select a Bot", systemImage: "sparkles")
                    }
                }
                .botsDestinations()
            }
            .id(selectedBot)
        default:
            Group {
                if let id = selectedItem, let item = (model.inbox.items + model.inbox.done).first(where: { $0.id == id }) {
                    let position = model.inbox.items.firstIndex(where: { $0.id == id }).map { $0 + 1 }
                    let visible = filter.apply(model.inbox.items)
                    InboxItemPane(item: item, position: position) {
                        // The list may have dropped the item already; the onChange below picked the one below.
                        if visible.contains(where: { $0.id == id }) { selectedItem = filter.next(after: id, in: visible) }
                    }
                    .id(item.id)
                } else {
                    ContentUnavailableView(model.inbox.items.isEmpty ? "Inbox zero" : "Select an item", systemImage: "tray")
                }
            }
            .onChange(of: filter.apply(model.inbox.items).map(\.id)) { old, new in
                // Done or Later took the picked item away: pick the one that took its place.
                guard let id = selectedItem, let at = old.firstIndex(of: id), !new.contains(id), !new.isEmpty else { return }
                selectedItem = new[min(at, new.count - 1)]
            }
        }
    }
}
