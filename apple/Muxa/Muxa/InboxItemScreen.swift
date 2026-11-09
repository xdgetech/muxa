import MuxaProtocol
import MuxaSession
import SwiftUI

/// One Inbox item (docs/inbox.md, "The item screen"): the question and its source,
/// the summary and the evidence; ✓, Later and the … menu in the top bar; the answers
/// to the agent and the reply box at the bottom. Opening it marks it read; it never
/// answers it. `answered` gets the item as the host saved it.
struct InboxItemScreen: View {
    @Environment(AppModel.self) private var model
    let id: String
    var answered: (InboxItem) -> Void
    @State private var reply = ""
    @State private var approving: InboxItem?
    @State private var sending = false
    @State private var failure: String?
    @State private var lostWork: LostWork?
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
                    sourceLine(item)
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
                if item.delegate != nil || item.sort != nil {
                    OverallProposal(item: item)
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
        .toolbar { ToolbarItemGroup(placement: .topBarTrailing) { topBar(item) } }
        .sheet(item: $approving) { item in
            ApproveSheet(title: item.title, workspace: item.workspace, proposal: item.proposal) {
                approving = nil
                send(item, .approve)
            }
        }
        .lostWorkAlert($lostWork)
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

    /// The source opens the item's session (ADR 0035 §3): its Chat where the agent has
    /// one, else the agent's pane. The pane switches between Chat, Agent and Shell.
    @ViewBuilder
    private func sourceLine(_ item: InboxItem) -> some View {
        if model.itemSession(item) != nil {
            Button {
                openSession(item)
            } label: {
                HStack(spacing: 4) {
                    Text(source(item))
                    Image(systemName: "chevron.right").font(.caption)
                }
            }
            .font(.subheadline)
            .accessibilityIdentifier("item-open-session")
        } else {
            Text(source(item)).font(.subheadline).foregroundStyle(.secondary)
        }
    }

    // MARK: Answers

    /// Later, ✓ and the … menu, the same on every item (ADR 0040): what the developer
    /// does with the item, apart from what the agent asked.
    @ViewBuilder
    private func topBar(_ item: InboxItem) -> some View {
        let answers = ItemAnswers(item)
        if answers.later {
            Button(InboxAction.later.title(for: item.kind), systemImage: Icon.later) { perform(item, .later) }
                .tint(Theme.later)
                .disabled(sending)
                .accessibilityIdentifier("item-answer-later")
        }
        if let done = answers.done {
            Button(done.title(for: item.kind), systemImage: Icon.done) { perform(item, done) }
                .tint(Theme.done)
                .disabled(sending)
                .accessibilityIdentifier("item-answer-\(done.rawValue)")
        }
        let session = model.itemSession(item) != nil
        if !answers.menu.isEmpty || session {
            Menu {
                ForEach(answers.menu, id: \.self) { action in
                    Button(action.title(for: item.kind), role: action == .drop ? .destructive : nil) { perform(item, action) }
                        .accessibilityIdentifier("item-answer-\(action.rawValue)")
                }
                if session {
                    Button("Open Session", systemImage: Icon.openSession) { openSession(item) }
                        .accessibilityIdentifier("item-open-session-menu")
                }
            } label: {
                Label("More", systemImage: "ellipsis.circle")
            }
            .disabled(sending)
            .accessibilityIdentifier("item-more")
        }
    }

    /// The answers to what the agent asked, then the reply box.
    @ViewBuilder
    private func answerBar(_ item: InboxItem) -> some View {
        let answers = ItemAnswers(item, replying: hasReply)
        let options = answers.chooses ? item.prompt?.options ?? [] : []
        if !options.isEmpty || !answers.decisions.isEmpty || answers.text != nil {
            VStack(spacing: 8) {
                if !options.isEmpty {
                    ViewThatFits {
                        HStack { optionButtons(item, options) }
                        VStack(alignment: .leading) { optionButtons(item, options) }
                    }
                }
                if !answers.decisions.isEmpty {
                    HStack(spacing: 10) {
                        ForEach(answers.decisions, id: \.self) { action in
                            answerButton(item, action, prominent: action == answers.main)
                        }
                    }
                    .controlSize(.large)
                    .disabled(sending)
                }
                if let action = answers.text {
                    ItemComposer(item: item, action: action, text: $reply, focused: $replying,
                                 sending: sending) { send(item, action) }
                }
            }
            .padding(.horizontal)
            .padding(.vertical, 10)
            .background(.bar)
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
        let button = Button { perform(item, action) } label: {
            Text(action.title(for: item.kind)).frame(maxWidth: .infinity)
        }
        .accessibilityIdentifier("item-answer-\(action.rawValue)")
        if prominent {
            button.buttonStyle(.borderedProminent)
        } else {
            button.buttonStyle(.bordered)
        }
    }

    private var hasReply: Bool { !reply.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    /// Its Chat where the agent has one, else its pane; the pane switches views.
    private func openSession(_ item: InboxItem) {
        model.open(item, as: model.hasChat(item) ? .chat : .terminal)
    }

    /// Approve on a proposal shows what the Task will get first.
    private func perform(_ item: InboxItem, _ action: InboxAction) {
        switch action {
        case .approve where item.kind == .proposal: approving = item
        case _ where action.needsText && !hasReply: replying = true
        default: send(item, action)
        }
    }

    /// Complete Task and Drop Task remove the worktree; when that could lose work, the user decides.
    private func send(_ item: InboxItem, _ action: InboxAction, option: UInt32? = nil, worktree: WorktreeEnd = .remove) {
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
                let saved = try await model.session.answer(item, action, text: text, option: option, worktree: worktree)
                reply = ""
                replying = false
                answered(saved)
            } catch {
                if let reason = PaneSession.lostWork(error) {
                    lostWork = LostWork(reason: reason) { send(item, action, option: option, worktree: .force) }
                } else {
                    failure = PaneSession.describe(error)
                }
            }
        }
    }
}

/// What approving the overall assistant's proposal does (ADR 0047 §4): ask a
/// workspace's delegate, or sort a Capture. The delegate's name is its workspace's.
private struct OverallProposal: View {
    @Environment(AppModel.self) private var model
    let item: InboxItem

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let d = item.delegate {
                LabeledContent("Asks", value: "\(d.workspace)'s assistant")
                    .accessibilityIdentifier("item-delegate-workspace")
                LabeledContent("Scope", value: d.scope.shown.capitalized)
                LabeledContent("Work Item", value: model.workItem(d.work)?.title ?? d.work)
                Text(d.purpose).textSelection(.enabled)
                    .accessibilityIdentifier("item-delegate-purpose")
                Text(d.reason).font(.callout).foregroundStyle(.secondary)
            }
            if let s = item.sort {
                LabeledContent("Action", value: s.action.shown)
                    .accessibilityIdentifier("item-sort-action")
                if let workspace = s.workspace { LabeledContent("Workspace", value: workspace) }
                if let title = s.title { LabeledContent("Title", value: title) }
                if let work = s.work { LabeledContent("Into", value: model.workItem(work)?.title ?? work) }
                if let text = model.session.knownCaptures[s.capture]?.text {
                    Text(text).font(.callout).textSelection(.enabled)
                        .accessibilityIdentifier("item-sort-capture")
                }
            }
        }
    }
}
