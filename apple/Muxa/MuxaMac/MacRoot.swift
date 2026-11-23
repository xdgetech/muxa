import SwiftUI

/// The Mac once connected: Home, Bots or Focus, as the desktop mocks (ADR
/// 0070, ADR 0086).
struct ConnectedRoot: View {
    @Environment(NextNavigation.self) private var nav

    var body: some View {
        if nav.focus {
            FocusView(selected: Bindable(nav).terminal)
        } else if nav.tab == .bots || nav.tab == .routines {
            BotsView()
        } else {
            HomeView(openTerminal: nav.open, showFocus: { nav.focus = true })
        }
    }
}
