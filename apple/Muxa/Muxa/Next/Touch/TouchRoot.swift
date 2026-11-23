import SwiftUI

/// The iPhone and iPad app once connected. The layout follows the size
/// class, not the device (ADR 0012): compact is the phone's tabs, regular the
/// tablet's sidebar.
struct ConnectedRoot: View {
    @Environment(\.horizontalSizeClass) private var size

    @Environment(NextNavigation.self) private var nav

    var body: some View {
        Group {
            if size == .compact {
                PhoneRoot()
            } else {
                TabletRoot()
            }
        }
        .onAppear { nav.openElsewhere = { [weak nav] t in nav?.showOnBotPage(t) ?? false } }
    }
}

/// The phone's four tabs, and a Task's screen over them.
struct PhoneRoot: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav

    var body: some View {
        Group {
            if nav.focus, let t = model.terminals.first(where: { $0.id == nav.terminal }) {
                PhoneTaskView(terminal: t)
            } else {
                tabs
            }
        }
        .onChange(of: nav.focus) { _, focus in
            // ⌘⇧F with no Task open shows the Tasks instead.
            if focus, nav.terminal == nil {
                nav.focus = false
                nav.tab = .tasks
            }
        }
    }

    private var tabs: some View {
        TabView(selection: Bindable(nav).tab) {
            PhoneInbox()
                .tabItem { Label("Inbox", systemImage: "tray") }
                .tag(NextNavigation.Tab.inbox)
            PhoneTasks()
                .tabItem { Label("Workspaces", systemImage: "rectangle.stack") }
                .tag(NextNavigation.Tab.tasks)
            PhoneBots()
                .tabItem { Label("Bots", systemImage: "sparkles") }
                .tag(NextNavigation.Tab.bots)
            PhoneSettings()
                .tabItem { Label("Settings", systemImage: "gearshape") }
                .tag(NextNavigation.Tab.hosts)
        }
        .tint(Palette.text)
    }
}

/// A tab's large title, with room on the right for its buttons.
struct ScreenTitle<Trailing: View>: View {
    let title: String
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: 10) {
            Text(title)
                .font(.system(size: 30, weight: .bold))
                .foregroundStyle(Palette.text)
                .accessibilityAddTraits(.isHeader)
            Spacer()
            trailing
        }
    }
}
