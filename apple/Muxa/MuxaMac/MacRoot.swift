import SwiftUI

/// The Mac once connected: Home or Focus, as the desktop mocks (ADR 0070).
struct ConnectedRoot: View {
    @Environment(NextNavigation.self) private var nav

    var body: some View {
        if nav.focus {
            FocusView(selected: Bindable(nav).terminal) { nav.focus = false }
        } else {
            HomeView(openTerminal: nav.open, showFocus: { nav.focus = true })
        }
    }
}
