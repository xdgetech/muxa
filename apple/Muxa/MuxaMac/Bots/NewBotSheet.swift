import MuxaV2
import SwiftUI

/// New Bot (ADR 0086 decision 3): a name, one sentence and the Workspace,
/// whose access it runs with (ADR 0087). Create adds the Bot and opens its
/// first chat on its page; its Routines come from that chat as proposals.
struct NewBotSheet: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(MacChrome.self) private var chrome
    @Environment(\.dismiss) private var dismiss
    @State private var draft = NewBotDraft()
    @State private var error: String?
    @State private var creating = false

    var body: some View {
        let hint = draft.hint(in: model.settings)
        SheetScaffold(
            title: "New Bot",
            lead: "It starts with a chat. Tell it what to watch; it proposes Routines you apply.",
            primary: "Create and Chat",
            primaryDisabled: !draft.valid(in: model.settings) || creating,
            primaryID: "newbot.create",
            cancel: { dismiss() },
            confirm: { Task { await create() } }
        ) {
            VStack(alignment: .leading, spacing: 10) {
                VStack(spacing: 0) {
                    FieldRow(label: "Name") {
                        TextField("Name", text: $draft.name, prompt: Text("deps").foregroundStyle(Palette.textDim))
                            .font(.system(size: 14, design: .monospaced))
                            .autocorrectionDisabled()
                            .accessibilityIdentifier("newbot.name")
                    }
                    FieldRow(label: "What it does") {
                        TextField("What it does", text: $draft.sentence,
                                  prompt: Text("Keeps the app's dependencies up to date").foregroundStyle(Palette.textDim))
                            .font(.system(size: 14))
                            .accessibilityIdentifier("newbot.description")
                    }
                    FieldRow(label: "Workspace", divider: false) {
                        Picker("Workspace", selection: $draft.workspace) {
                            ForEach(workspaces, id: \.self) { Text($0).tag($0) }
                        }
                        .labelsHidden()
                        .fixedSize()
                        .accessibilityIdentifier("newbot.workspace")
                    }
                }
                .raisedCard()
                Text(hint.text)
                    .font(.system(size: 12))
                    .foregroundStyle(hint.problem ? Palette.late : Palette.textTertiary)
                    .accessibilityIdentifier("newbot.hint")
                if !draft.workspace.isEmpty {
                    Text("It runs with \(draft.workspace)'s access: its connections and MCP servers.")
                        .font(.system(size: 12))
                        .foregroundStyle(Palette.textTertiary)
                }
                SaveError(text: error, id: "newbot.error")
            }
        }
        .onAppear {
            pickWorkspace()
            // The Bots page may show before the settings are read; the Workspace list needs them.
            if model.settings.settings == nil { Task { await model.settings.load() } }
        }
        .onChange(of: workspaces) { pickWorkspace() }
        .onChange(of: draft.name) { _, typed in
            let fixed = NewBotDraft.typed(typed)
            if fixed != typed { draft.name = fixed }
        }
    }

    /// The picked Bot's Workspace, else the first, once the settings are read.
    private func pickWorkspace() {
        var preferred: String?
        if case let .bot(w, _)? = chrome.botPick { preferred = w }
        draft.pick(from: workspaces, preferring: preferred)
    }

    private var workspaces: [String] { model.settings.settings?.workspaces.map(\.name) ?? [] }

    /// `bot-add`, then the first talk; a talk that cannot start leaves the Bot added.
    private func create() async {
        creating = true
        defer { creating = false }
        let spec = draft.spec
        let made = await model.createBot(spec, in: draft.workspace)
        if made.added { chrome.botPick = .bot(workspace: draft.workspace, name: spec.name) }
        guard let talk = made.talk else {
            error = made.error
            return
        }
        dismiss()
        nav.open(talk)
    }
}
