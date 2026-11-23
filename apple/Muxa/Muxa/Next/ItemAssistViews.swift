import MuxaV2
import SwiftUI

/// What a Bot proposes to change in the settings (ADR 0083) and what the
/// Capture assistant suggests (ADR 0084), with their actions, and Ask a Bot.
/// muxa applies a proposal or a suggestion only when the user says so.
struct ItemAssist: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let item: InboxItem
    /// False for a Done item: nothing to act on.
    let live: Bool
    var onDone: () -> Void

    var body: some View {
        Group {
            if let p = item.settingsProposal { proposalCard(p) }
            if live, let s = item.suggestion { suggestionCard(s) }
            if live, !askable.isEmpty { askMenu }
        }
        // Ask a Bot lists the Workspace's Bots from the settings.
        .task(id: item.workspace) {
            if item.workspace != nil, model.settings.settings == nil { await model.settings.load() }
        }
    }

    private func proposalCard(_ p: SettingsProposal) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Proposed settings").font(.system(size: 13)).foregroundStyle(Palette.textSecondary)
            ForEach(Array(p.ops.enumerated()), id: \.offset) { n, op in
                Text(op.summary)
                    .font(.system(.callout, design: .monospaced))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("detail.op.\(n)")
            }
            Text("Apply changes the settings only if nobody changed them since. Else ask the Bot again.")
                .font(.caption)
                .foregroundStyle(Palette.textSecondary)
            if live {
                Button("Apply", systemImage: "checkmark.seal") {
                    Task {
                        await model.applyProposal(item)
                        if model.requestError == nil { onDone() }
                    }
                }
                .buttonStyle(FilledButtonStyle())
                .accessibilityIdentifier("detail.apply")
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(radius: 12, border: true)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("detail.settingsproposal")
    }

    private func suggestionCard(_ s: ItemSuggestion) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("Suggested", systemImage: "sparkles").font(.system(size: 13)).foregroundStyle(Palette.textSecondary)
            if let kind = s.kind { LabeledContent("Kind", value: kind.title) }
            if let workspace = s.workspace { LabeledContent("Workspace", value: workspace) }
            if let due = s.due { LabeledContent("Due", value: due) }
            if let note = s.note { Text(note).frame(maxWidth: .infinity, alignment: .leading) }
            if !s.split.isEmpty {
                Text("Split into").font(.system(size: 13)).foregroundStyle(Palette.textSecondary)
                ForEach(Array(s.split.enumerated()), id: \.offset) { _, title in
                    Label(title, systemImage: "smallcircle.filled.circle").labelStyle(.titleAndIcon)
                }
            }
            if let goal = s.taskGoal {
                LabeledContent("Task", value: goal)
            }
            ViewThatFits(in: .horizontal) {
                HStack { suggestionButtons(s) }
                VStack(alignment: .leading) { suggestionButtons(s) }
            }
            .buttonStyle(QuietButtonStyle())
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .card(radius: 12, border: true)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("detail.suggestion")
    }

    @ViewBuilder
    private func suggestionButtons(_ s: ItemSuggestion) -> some View {
        Button("Accept", systemImage: "checkmark") { Task { await answer(accept: true) } }
            .accessibilityIdentifier("suggestion.accept")
        Button("Dismiss", systemImage: "xmark") { Task { await answer(accept: false) } }
            .accessibilityIdentifier("suggestion.dismiss")
        if let goal = s.taskGoal {
            Button("Start Task", systemImage: "play") {
                nav.newTask = TaskSeed(title: item.title, goal: goal, workspace: s.workspace ?? item.workspace)
            }
            .accessibilityIdentifier("suggestion.starttask")
        }
        if let bot = s.bot, let workspace = s.workspace ?? item.workspace {
            Button("Ask \(bot)", systemImage: "sparkles") { Task { await ask(bot, in: workspace) } }
                .accessibilityIdentifier("suggestion.ask")
        }
    }

    /// The Bots of the item's Workspace.
    private var askable: [String] {
        guard let workspace = item.workspace else { return [] }
        return model.settings.workspace(workspace)?.bots.map(\.name) ?? []
    }

    private var askMenu: some View {
        Menu {
            ForEach(askable, id: \.self) { bot in
                Button(bot) { Task { await ask(bot, in: item.workspace ?? "") } }
            }
        } label: {
            Label("Ask a Bot", systemImage: "sparkles")
        }
        .menuStyle(.button)
        .buttonStyle(QuietButtonStyle())
        .fixedSize()
        .accessibilityIdentifier("detail.askbot")
    }

    private func answer(accept: Bool) async {
        await model.inbox(.inboxSuggestion(item: item.id, accept: accept))
    }

    /// A talk with the Bot whose goal is the item; it opens like any started Task.
    private func ask(_ bot: String, in workspace: String) async {
        guard let t = await model.startTalk(workspace: workspace, bot: bot, title: "Ask \(bot): \(item.title)",
                                            goal: item.askGoal) else { return }
        nav.open(t)
    }
}

extension InboxItem {
    /// What a Bot is asked: the title, the note and the links.
    var askGoal: String {
        let links = (links ?? []).map { [$0.title, $0.url].compactMap { $0 }.joined(separator: ": ") }
        return ([title, note] + links).compactMap { $0 }.joined(separator: "\n\n")
    }
}

extension SettingsOp {
    /// One line a person reads: `routine-add nightly-sweep (off)`.
    var summary: String {
        switch self {
        case let .routineAdd(_, r, _), let .routineUpdate(_, r, _):
            "\(name) \(r.name)" + (r.enabled ? "" : " (off)")
        case let .routineRemove(_, n), let .botRemove(_, n): "\(name) \(n)"
        case let .botAdd(_, b, _), let .botUpdate(_, b, _):
            "\(name) \(b.name)" + (b.members.isEmpty ? "" : " leads \(b.members.joined(separator: ", "))")
        default: name
        }
    }
}

/// Where a started Bot talk (ADR 0083) shows.
enum BotTalk {
    /// Shows the started talk: the Task screen on iOS, Focus in the Mac's main
    /// window, which Settings may hide.
    @MainActor
    static func show(_ t: MuxaV2.Terminal, nav: NextNavigation, openWindow: OpenWindowAction) {
        nav.open(t)
        #if os(macOS)
        openWindow(id: "main")
        #endif
    }
}

/// "Set up a Bot by talking": asks for the new Bot's name, then starts the talk.
struct NewBotTalk: ViewModifier {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.openWindow) private var openWindow
    let workspace: String
    @Binding var isPresented: Bool
    @State private var name = ""
    @State private var failure: String?

    func body(content: Content) -> some View {
        content
            .alert("The talk did not start", isPresented: failed, presenting: failure) { _ in
                Button("OK", role: .cancel) {}
            } message: { Text($0) }
            .alert("Set up a Bot by talking", isPresented: $isPresented) {
                TextField("Name, e.g. triage", text: $name)
                    .plainEntry()
                    .accessibilityIdentifier("bot.talkname")
                Button("Talk") { Task { await start() } }
                    .disabled(!BotSpec.isValidName(Lines.trimmed(name)))
                    .accessibilityIdentifier("bot.talkstart")
                Button("Cancel", role: .cancel) { name = "" }
            } message: {
                Text("Lowercase letters, digits, - and _. The Bot proposes its settings; you apply them from the Inbox.")
            }
    }

    private func start() async {
        let bot = Lines.trimmed(name)
        guard BotSpec.isValidName(bot) else { return }
        guard let t = await model.startTalk(workspace: workspace, bot: bot, title: "Talk to \(bot)") else {
            failure = model.requestError
            return
        }
        name = ""
        BotTalk.show(t, nav: nav, openWindow: openWindow)
    }

    private var failed: Binding<Bool> {
        Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })
    }
}

extension View {
    func newBotTalk(workspace: String, isPresented: Binding<Bool>) -> some View {
        modifier(NewBotTalk(workspace: workspace, isPresented: isPresented))
    }
}
