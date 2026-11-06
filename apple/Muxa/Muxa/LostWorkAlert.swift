import MuxaSession
import SwiftUI

/// Done or Drop that the host refused with `would-lose-work` (ADR 0043): its reason,
/// and `remove`, which sends the same end again with `force`.
struct LostWork {
    let reason: String
    let remove: () -> Void
}

extension View {
    /// Remove sends the end again; Cancel changes nothing, so the Task stays as it was.
    func lostWorkAlert(_ lost: Binding<LostWork?>) -> some View {
        alert(
            "Remove the Worktree?",
            isPresented: Binding(get: { lost.wrappedValue != nil }, set: { if !$0 { lost.wrappedValue = nil } }),
            presenting: lost.wrappedValue
        ) { work in
            Button("Remove", role: .destructive) { work.remove() }
            Button("Cancel", role: .cancel) {}
        } message: { work in
            Text(PaneSession.removeAnyway(work.reason))
        }
    }
}
