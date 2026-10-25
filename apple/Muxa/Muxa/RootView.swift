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
            TrustHostSheet(key: key)
        }
        .sheet(isPresented: $model.showSwitcher) {
            SessionSwitcher()
        }
    }
}
