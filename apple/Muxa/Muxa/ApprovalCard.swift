import MuxaProtocol
import SwiftUI

/// What the agent waits on, with the choices its adapter knows. Choices the
/// adapter cannot name stay in the terminal, one tap away.
struct ApprovalCard: View {
    @Environment(AppModel.self) private var model
    let prompt: ChatPrompt
    /// The tool call the agent waits on, when it is in the log.
    let tool: ChatTool?
    /// The prompt last answered here, so one tap sends its keys once.
    @State private var answered: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(title, systemImage: Icon.waiting)
                .font(.callout.weight(.semibold))
                .foregroundStyle(Theme.attention)
            if let detail {
                Text(detail)
                    .font(.callout.monospaced())
                    .lineLimit(4)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
                    .background(Theme.toolCard, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    if prompt.kind == .plan {
                        Button("Read Plan") { model.openPlan() }
                            .buttonStyle(.borderedProminent)
                            .accessibilityIdentifier("approval-read-plan")
                    }
                    ForEach(Array(prompt.options.enumerated()), id: \.offset) { _, option in
                        choice(option)
                    }
                    Button(terminalTitle) { model.setPretty(false) }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("approval-terminal")
                }
            }
        }
        .padding(14)
        .frame(maxWidth: 720)
        .background(Theme.attention.opacity(0.12), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("approval")
    }

    @ViewBuilder
    private func choice(_ option: ChatPrompt.Option) -> some View {
        let button = Button(option.label) {
            answered = prompt.entry
            model.session.send(keys: option.keys)
        }
        .disabled(answered == prompt.entry)
        .accessibilityIdentifier("approval-\(option.label)")
        if option.cancel {
            button.buttonStyle(.bordered).tint(Theme.failure)
        } else {
            button.buttonStyle(.borderedProminent)
        }
    }

    private var title: String {
        switch prompt.kind {
        case .permission: "Allow \(tool?.name ?? "this tool")?"
        case .question: prompt.question ?? "The agent asks a question"
        case .plan: "The plan is ready"
        case .other: "Waiting for your answer"
        }
    }

    private var detail: String? {
        switch prompt.kind {
        case .permission: tool.map(\.summary).flatMap { $0.isEmpty ? nil : $0 }
        case .question, .plan, .other: nil
        }
    }

    /// The menu has more than the card offers; it is in the terminal.
    private var terminalTitle: String {
        switch prompt.kind {
        case .plan: "Approve…"
        case .question: "Other Answer…"
        case .permission, .other: "More…"
        }
    }
}
