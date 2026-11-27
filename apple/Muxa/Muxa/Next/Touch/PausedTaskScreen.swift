import MuxaV2
import SwiftUI

/// A paused Task in place of its Task screen (ADR 0104): back on a phone, the
/// Paused view, and Close Task's confirm, then the host's reason when its
/// worktree holds work, as the Mac asks.
struct PausedTaskScreen: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let paused: Paused
    var showsBack = false
    @State private var closing = false
    @State private var wouldLoseWork: String?

    var body: some View {
        VStack(spacing: 0) {
            if showsBack {
                HStack {
                    Button { nav.focus = false } label: { Image(systemName: "chevron.left") }
                        .buttonStyle(BareIconStyle())
                        .accessibilityLabel("Back")
                        .accessibilityIdentifier("mode.home")
                    Spacer()
                }
                .padding(.horizontal, 8)
                .padding(.top, 4)
            }
            PausedTaskView(paused: paused) { closing = true }
            if let error = model.requestError {
                Text(error)
                    .font(.footnote)
                    .foregroundStyle(Palette.late)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 6)
                    .accessibilityIdentifier("paused.error")
            }
        }
        .background(Palette.ground)
        .dialog(
            "Close “\(paused.terminal.title)”?",
            isPresented: $closing,
            message: "Its branch worktrees are removed when they are clean and merged. Its Inbox items are marked Done.",
            actions: [DialogAction("Close Task", role: .destructive, id: "closetask.confirm") {
                Task { await close(force: false) }
            }]
        )
        .dialog(
            "Remove the worktree anyway?",
            isPresented: Binding(get: { wouldLoseWork != nil }, set: { if !$0 { wouldLoseWork = nil } }),
            message: wouldLoseWork,
            actions: [DialogAction("Remove Anyway", role: .destructive, id: "closetask.force") {
                Task { await close(force: true) }
            }]
        )
    }

    private func close(force: Bool) async {
        if let why = await model.closeTask(paused.id, force: force) { wouldLoseWork = why }
    }
}
