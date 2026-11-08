import SwiftUI

/// What opens over a pane from its More menu: Task Info (ADR 0044) and the scrollback.
private struct PaneSheets: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        content
            .sheet(
                isPresented: Binding(get: { model.taskInfo != nil }, set: { if !$0 { model.taskInfo = nil } }),
                onDismiss: model.refocusPane
            ) {
                if let id = model.taskInfo { TaskInfoSheet(id: id) }
            }
            .fullScreenCover(
                isPresented: Binding(get: { model.history != nil }, set: { if !$0 { model.history = nil } }),
                onDismiss: model.refocusPane
            ) {
                HistoryScreen()
            }
    }
}

extension View {
    func paneSheets() -> some View { modifier(PaneSheets()) }
}
