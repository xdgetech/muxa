import MuxaV2
import SwiftUI

/// "Delete older than 1, 3 or 5 days" (ADR 0092): the user's one step to
/// clear the Inbox, now that Bots have no daily limit (ADR 0091).
enum InboxDeleteOlder {
    static let choices = [1, 3, 5]

    static func title(_ days: Int) -> String { days == 1 ? "1 day" : "\(days) days" }

    /// The listed open and done items the host would remove; it may hold a
    /// few more done ones than it lists.
    static func count(_ inbox: Inbox, days: Int, now: Int64) -> Int {
        let cutoff = now - Int64(days) * 86_400
        return (inbox.items + inbox.done).filter { $0.created < cutoff }.count
    }
}

/// The choices, as a submenu of an Inbox menu. Picking one asks first.
struct InboxDeleteOlderMenu: View {
    @Binding var days: Int?

    var body: some View {
        Menu {
            ForEach(InboxDeleteOlder.choices, id: \.self) { d in
                Button(InboxDeleteOlder.title(d), role: .destructive) { days = d }
                    .accessibilityIdentifier("inbox.deleteOlder.\(d)")
            }
        } label: {
            Label("Delete Older Than", systemImage: "trash")
        }
        .accessibilityIdentifier("inbox.deleteOlder")
    }
}

private struct InboxDeleteOlderConfirm: ViewModifier {
    @Environment(NextModel.self) private var model
    @Binding var days: Int?

    func body(content: Content) -> some View {
        let d = days ?? 1
        let count = InboxDeleteOlder.count(model.inbox, days: d, now: Int64(Date.now.timeIntervalSince1970))
        let older = "older than \(InboxDeleteOlder.title(d))"
        content.alert(
            count == 0 ? "No items are \(older)." : "Delete \(count == 1 ? "1 item" : "\(count) items") \(older)?",
            isPresented: Binding(get: { days != nil }, set: { if !$0 { days = nil } })
        ) {
            if count > 0 {
                Button("Delete", role: .destructive) {
                    Task { await model.inbox(.inboxDeleteOlder(days: d)) }
                }
                .accessibilityIdentifier("inbox.deleteOlder.confirm")
                Button("Cancel", role: .cancel) {}
            } else {
                Button("OK", role: .cancel) {}
            }
        } message: {
            if count > 0 { Text("Open and Done items both go. This cannot be undone.") }
        }
    }
}

extension View {
    /// Asks before deleting the items older than `days`, then sends it.
    func inboxDeleteOlder(_ days: Binding<Int?>) -> some View {
        modifier(InboxDeleteOlderConfirm(days: days))
    }
}
