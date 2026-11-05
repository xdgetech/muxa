import MuxaProtocol
import MuxaSession
import SwiftUI

/// One Inbox item (docs/inbox.md, "The item screen"): the question and its source,
/// the summary, the evidence, a reply box and the answers. Opening it marks it read;
/// it never answers it. `answered` gets the item as the host saved it.
struct InboxItemScreen: View {
    @Environment(AppModel.self) private var model
    let id: String
    var answered: (InboxItem) -> Void
    @State private var reply = ""
    @State private var approving: InboxItem?
    @State private var sending = false
    @State private var failure: String?
    @FocusState private var replying: Bool

    var body: some View {
        if let item = model.item(id) {
            content(item)
                .task(id: item.id) { await model.session.markRead(item) }
        } else {
            ContentUnavailableView("Item Not Found", systemImage: "tray",
                                   description: Text("Another device or the terminal answered it."))
        }
    }

    private func content(_ item: InboxItem) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(item.question).font(.title3.weight(.semibold))
                        .accessibilityIdentifier("item-question")
                    Text(source(item)).font(.subheadline).foregroundStyle(.secondary)
                }
                if let summary = item.summary, !summary.isEmpty {
                    ItemSummaryText(item: item, summary: summary)
                }
                if let question = item.prompt?.question {
                    Text(question).font(.headline)
                }
                if item.kind == .proposal, let p = item.proposal {
                    VStack(alignment: .leading, spacing: 6) {
                        LabeledContent("Action", value: p.action)
                        LabeledContent("Target", value: p.target)
                        Text(p.content).font(.callout.monospaced()).textSelection(.enabled)
                    }
                    .padding(12)
                    .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))
                }
                ItemEvidence(item: item)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding()
        }
        .scrollDismissesKeyboard(.interactively)
        // In the bottom inset, the reply and the answers stay above the keyboard.
        .safeAreaInset(edge: .bottom) { answerBar(item) }
        .navigationTitle(item.kind.title)
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $approving) { item in
            ApproveSheet(title: item.title, workspace: item.workspace, proposal: item.proposal) {
                approving = nil
                send(item, .approve)
            }
        }
        .alert("Can't Answer", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
        .background {
            let main = ItemAnswers(item, replying: hasReply).main
            Button("Send the Main Answer") { if let main { perform(item, main) } }
                .keyboardShortcut(.return, modifiers: .command)
                .disabled(main == nil || sending)
                .hidden()
        }
    }

    private func source(_ item: InboxItem) -> String {
        let from = item.routine.map { "\(item.workspace)/\($0)" } ?? "\(item.title) · \(item.workspace)"
        return "\(from) · \(item.age())"
    }

    // MARK: Answers

    private func answerBar(_ item: InboxItem) -> some View {
        let answers = ItemAnswers(item, replying: hasReply)
        return VStack(spacing: 8) {
            if item.answers.contains(where: \.needsText) {
                TextField(item.answers.contains(where: { $0.needsText && $0 != .askAgent }) ? "Reply…" : "Ask the workspace's agent…",
                          text: $reply, axis: .vertical)
                    .lineLimit(1...4)
                    .textFieldStyle(.roundedBorder)
                    .focused($replying)
                    .accessibilityIdentifier("item-reply")
            }
            if answers.chooses, let options = item.prompt?.options {
                ViewThatFits {
                    HStack { optionButtons(item, options) }
                    VStack(alignment: .leading) { optionButtons(item, options) }
                }
            }
            // A phone puts the ways into the session on a row of their own.
            ViewThatFits(in: .horizontal) {
                HStack {
                    answerButtons(item, answers)
                    Spacer(minLength: 0)
                    sessionButtons(item)
                }
                VStack(alignment: .leading, spacing: 8) {
                    HStack { answerButtons(item, answers) }
                    HStack {
                        Spacer(minLength: 0)
                        sessionButtons(item)
                    }
                }
            }
            .disabled(sending)
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
        .background(.bar)
    }

    /// The text buttons, More when two or more are hidden, then ✓ and Later as icons
    /// (ADR 0040).
    @ViewBuilder
    private func answerButtons(_ item: InboxItem, _ answers: ItemAnswers) -> some View {
        ForEach(answers.shown, id: \.self) { action in
            answerButton(item, action, prominent: action == answers.main)
        }
        if !answers.more.isEmpty {
            Menu {
                ForEach(answers.more, id: \.self) { action in
                    Button(action.title(for: item.kind)) { perform(item, action) }
                        .disabled(action.needsText && !hasReply)
                        .accessibilityIdentifier("item-answer-\(action.rawValue)")
                }
            } label: {
                Label("More", systemImage: "ellipsis.circle")
            }
            .accessibilityIdentifier("item-more")
        }
        if let done = answers.done {
            iconButton(item, done, systemImage: Icon.done, prominent: done == answers.main)
        }
        if answers.later {
            iconButton(item, .later, systemImage: Icon.later, prominent: false)
        }
    }

    /// Chat and Terminal show the item's session (ADR 0035 §3): an `ask` is answered
    /// in Chat, a `failure` read in the terminal. Chat only where the agent has one.
    @ViewBuilder
    private func sessionButtons(_ item: InboxItem) -> some View {
        if model.itemSession(item) != nil {
            if model.hasChat(item) {
                Button("Chat", systemImage: Icon.chat) { model.open(item, as: .chat) }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("item-open-chat")
            }
            Button("Terminal", systemImage: Icon.agent) { model.open(item, as: .terminal) }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("item-open-terminal")
        }
    }

    @ViewBuilder
    private func optionButtons(_ item: InboxItem, _ options: [ChatPrompt.Option]) -> some View {
        ForEach(Array(options.enumerated()), id: \.offset) { index, option in
            Button(option.label) { send(item, .choose, option: UInt32(index)) }
                .buttonStyle(.bordered)
                .tint(option.cancel ? .secondary : .accentColor)
                .disabled(sending)
                .accessibilityIdentifier("item-option-\(index)")
        }
    }

    @ViewBuilder
    private func answerButton(_ item: InboxItem, _ action: InboxAction, prominent: Bool) -> some View {
        styled(Button(action.title(for: item.kind)) { perform(item, action) }, prominent: prominent)
            .disabled(action.needsText && !hasReply)
            .accessibilityIdentifier("item-answer-\(action.rawValue)")
    }

    private func iconButton(_ item: InboxItem, _ action: InboxAction, systemImage: String, prominent: Bool) -> some View {
        styled(Button(action.title(for: item.kind), systemImage: systemImage) { perform(item, action) }, prominent: prominent)
            .labelStyle(.iconOnly)
            .accessibilityIdentifier("item-answer-\(action.rawValue)")
    }

    @ViewBuilder
    private func styled(_ button: some View, prominent: Bool) -> some View {
        if prominent {
            button.buttonStyle(.borderedProminent)
        } else {
            button.buttonStyle(.bordered)
        }
    }

    private var hasReply: Bool { !reply.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    /// Approve on a proposal shows what the Task will get first.
    private func perform(_ item: InboxItem, _ action: InboxAction) {
        switch action {
        case .approve where item.kind == .proposal: approving = item
        case _ where action.needsText && !hasReply: replying = true
        default: send(item, action)
        }
    }

    private func send(_ item: InboxItem, _ action: InboxAction, option: UInt32? = nil) {
        let text = action.needsText ? reply.trimmingCharacters(in: .whitespacesAndNewlines) : nil
        sending = true
        Task {
            defer { sending = false }
            do {
                // Ask Agent leaves the item open: its session's Chat shows instead.
                if action == .askAgent, let text {
                    try await model.askAgent(item, text: text)
                    reply = ""
                    replying = false
                    return
                }
                let saved = try await model.session.answer(item, action, text: text, option: option)
                reply = ""
                replying = false
                answered(saved)
            } catch {
                failure = PaneSession.describe(error)
            }
        }
    }
}
