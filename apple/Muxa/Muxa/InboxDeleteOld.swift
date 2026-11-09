import MuxaProtocol
import MuxaSession
import SwiftUI

/// Delete Old in the Inbox list's toolbar menu (docs/inbox.md, ADR 0046): it removes
/// the items older than 1, 3 or 5 days, open or closed, except protected ones. The
/// confirmation names the count. Only a host with `work` level 8 offers it.
struct DeleteOldItems: ViewModifier {
    @Environment(AppModel.self) private var model
    @State private var asking: Ask?
    @State private var failure: String?

    /// The confirmation for one age, with the count when it was asked.
    struct Ask {
        var days: UInt32
        var count: Int
        var message: String { PaneSession.deleteOldMessage(count: count, days: days) }
    }

    func body(content: Content) -> some View {
        content
            .toolbar {
                if model.mode == .inbox, model.usesItems, model.session.canDeleteInbox {
                    ToolbarItem(placement: .primaryAction) { menu }
                }
            }
            .alert(asking?.message ?? "", isPresented: present($asking), presenting: asking) { ask in
                if ask.count > 0 {
                    Button("Delete", role: .destructive) { delete(ask.days) }
                    Button("Cancel", role: .cancel) {}
                } else {
                    Button("OK", role: .cancel) {}
                }
            }
            .alert("Can't Delete", isPresented: present($failure)) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(failure ?? "")
            }
    }

    private var menu: some View {
        Menu {
            Menu("Delete Old", systemImage: Icon.delete) {
                ForEach(PaneSession.deleteOldDays, id: \.self) { days in
                    Button(days == 1 ? "Older than 1 day" : "Older than \(days) days") {
                        asking = Ask(days: days, count: model.session.deletableCount(olderThanDays: days))
                    }
                    .accessibilityIdentifier("inbox-delete-\(days)")
                }
            }
            .accessibilityIdentifier("inbox-delete-old")
        } label: {
            Label("More", systemImage: Icon.more)
        }
        .accessibilityIdentifier("inbox-more")
    }

    private func delete(_ days: UInt32) {
        Task {
            do {
                try await model.session.deleteOld(olderThanDays: days)
            } catch {
                failure = PaneSession.describe(error)
            }
        }
    }

    private func present<T>(_ value: Binding<T?>) -> Binding<Bool> {
        Binding(get: { value.wrappedValue != nil }, set: { if !$0 { value.wrappedValue = nil } })
    }
}
