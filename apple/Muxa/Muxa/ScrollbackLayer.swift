import MuxaSession
import MuxaUI
import SwiftUI

/// The shown pane's scrollback in the pane's place, opened at the newest line, so a
/// scroll up on the pane goes on into older lines. Reading it changes nothing on the
/// host: no copy mode, no scroll position (PROTOCOL.md "History").
struct ScrollbackLayer: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            Theme.terminal
            content
            Button("Live", systemImage: Icon.live) { model.closeHistory() }
                .buttonStyle(.borderedProminent)
                .padding(16)
                .accessibilityIdentifier("history-live")
        }
    }

    @ViewBuilder private var content: some View {
        switch model.history {
        case .loaded(let history):
            VStack(spacing: 0) {
                if history.truncated {
                    Text("Older lines are not shown.").font(.caption).foregroundStyle(.secondary).padding(4)
                }
                HistoryHost(history: history) { model.closeHistory() }
            }
        case .failed(let reason):
            ContentUnavailableView("Can't Show the Scrollback", systemImage: Icon.warning, description: Text(reason))
        case .loading, nil:
            ProgressView().tint(.white).frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

private struct HistoryHost: UIViewRepresentable {
    let history: PaneHistory
    let onLive: () -> Void

    func makeUIView(context: Context) -> HistoryView {
        let view = HistoryView(history: history)
        view.accessibilityIdentifier = "history"
        return view
    }

    func updateUIView(_ view: HistoryView, context: Context) {
        view.onLive = onLive
    }
}
