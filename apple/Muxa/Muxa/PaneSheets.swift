import SwiftUI

/// What opens over a pane from its More menu: Task Info (ADR 0044).
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
    }
}

extension View {
    func paneSheets() -> some View { modifier(PaneSheets()) }
}
