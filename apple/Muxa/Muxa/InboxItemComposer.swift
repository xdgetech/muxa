import MuxaProtocol
import MuxaSession
import SwiftUI

/// The reply box of an item (docs/inbox.md, "The item screen"): one text field and a
/// send arrow. `action` is Send to the item's agent, or Ask Agent where the item has no
/// session (ADR 0041); the placeholder says where the text goes.
struct ItemComposer: View {
    let item: InboxItem
    let action: InboxAction
    @Binding var text: String
    var focused: FocusState<Bool>.Binding
    let sending: Bool
    let send: () -> Void

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField(placeholder, text: $text, axis: .vertical)
                .lineLimit(1...4)
                .textFieldStyle(.roundedBorder)
                .focused(focused)
                .accessibilityIdentifier("item-reply")
            Button(action.title(for: item.kind), systemImage: "arrow.up.circle.fill", action: send)
                .labelStyle(.iconOnly)
                .font(.title2)
                .disabled(isEmpty || sending)
                .accessibilityIdentifier("item-send")
        }
    }

    private var isEmpty: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    private var placeholder: String {
        switch action {
        case .askAgent: "Ask the workspace's agent…"
        case _ where item.kind == .ask: "Deny, then tell the agent…"
        default: "Reply to the agent…"
        }
    }
}
