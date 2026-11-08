import MuxaSession
import MuxaUI
import SwiftUI

/// The shown pane's scrollback over the pane, newest line at the bottom. Reading it
/// changes nothing on the host: no copy mode, no scroll position (PROTOCOL.md "History").
struct HistoryScreen: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        NavigationStack {
            content
                .background(Theme.terminal)
                .navigationTitle("Scrollback")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { model.history = nil }
                            .keyboardShortcut(.cancelAction)
                            .accessibilityIdentifier("history-done")
                    }
                }
        }
    }

    @ViewBuilder private var content: some View {
        switch model.history {
        case .loaded(let history):
            VStack(spacing: 0) {
                if history.truncated {
                    Text("Older lines are not shown.").font(.caption).foregroundStyle(.secondary).padding(4)
                }
                HistoryHost(history: history)
            }
        case .failed(let reason):
            ContentUnavailableView("Can't Show the Scrollback", systemImage: Icon.warning, description: Text(reason))
        case .loading, nil:
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

private struct HistoryHost: UIViewRepresentable {
    let history: PaneHistory

    func makeUIView(context: Context) -> HistoryView {
        let view = HistoryView(history: history)
        view.accessibilityIdentifier = "history"
        return view
    }

    func updateUIView(_ view: HistoryView, context: Context) {}
}
