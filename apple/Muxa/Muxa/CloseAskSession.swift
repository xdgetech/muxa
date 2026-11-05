import SwiftUI

/// Close Session for an ask session (ADR 0039): a confirmation, then why it failed.
private struct CloseAskSessionModifier: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        content
            .confirmationDialog(
                "Close \(model.closingAsk?.name ?? "Session")?",
                isPresented: Binding(get: { model.closingAsk != nil }, set: { if !$0 { model.closingAsk = nil } }),
                titleVisibility: .visible,
                presenting: model.closingAsk
            ) { ask in
                Button("Close Session", role: .destructive) { Task { await model.closeAsk(ask) } }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text("The agent stops. The main checkout and its files stay as they are.")
            }
            .alert(
                "Can't Close the Session",
                isPresented: Binding(get: { model.closeAskError != nil }, set: { if !$0 { model.closeAskError = nil } })
            ) {
                Button("OK", role: .cancel) { model.refocusPane() }
            } message: {
                Text(model.closeAskError ?? "")
            }
    }
}

extension View {
    func closeAskSession() -> some View { modifier(CloseAskSessionModifier()) }
}
