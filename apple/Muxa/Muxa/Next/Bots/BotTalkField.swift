import MuxaV2
import SwiftUI

/// "Talk to <bot>…" at the top of a Bot's Home (ADR 0086): Return or Send
/// starts a talk with the text as its first message, and the talk opens on
/// the Bot's page. Nothing starts until the user sends.
struct BotTalkField: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let entry: BotEntry
    @State private var text = ""
    @State private var starting = false

    var body: some View {
        HStack(spacing: 8) {
            TextField("Talk to \(entry.name)…", text: $text, axis: .vertical)
                .lineLimit(1...8)
                .textFieldStyle(.plain)
                .submitLabel(.send)
                .onSubmit(send)
                .newlineOnShiftReturn()
                .padding(.horizontal, 4)
                .fieldWell(radius: 10)
                .accessibilityIdentifier("bot.home.talk")
            SendButton(title: "Send", disabled: line.isEmpty || starting || !model.isConnected, action: send)
                .accessibilityIdentifier("bot.home.send")
        }
    }

    private var line: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }

    private func send() {
        let goal = line
        guard !goal.isEmpty, !starting else { return }
        starting = true
        // Cleared at once, as Chat's field is, so the field shows the send happened.
        text = ""
        Task {
            defer { starting = false }
            // The talk opens where a Bot's terminals open: its page, on Chat.
            if let t = await model.startChat(with: entry, goal: goal) {
                nav.open(t)
            } else {
                text = goal
            }
        }
    }
}
