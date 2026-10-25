import SwiftUI

struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        Group {
            if model.isConnected {
                PaneScreen()
            } else {
                ConnectView()
            }
        }
        .sheet(item: $model.untrustedKey) { key in
            // Only Trust or Cancel closes it, so a held setup password is always cleared.
            TrustHostSheet(key: key).interactiveDismissDisabled()
        }
        .sheet(isPresented: $model.showSwitcher) {
            SessionSwitcher()
        }
    }
}
