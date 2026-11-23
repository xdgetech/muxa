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
    @State private var name = ""
    @State private var sentence = ""
    @State private var workspace = ""
    @State private var error: String?
    @State private var creating = false

    var body: some View {
        SheetScaffold(
            title: "New Bot",
            lead: "It starts with a chat. Tell it what to watch; it proposes Routines you apply.",
            primary: "Create and Chat",
            primaryDisabled: !valid || creating,
            primaryID: "newbot.create",
            cancel: { dismiss() },
            confirm: { Task { await create() } }
        ) {
            VStack(alignment: .leading, spacing: 10) {
                VStack(spacing: 0) {
                    FieldRow(label: "Name") {
                        TextField("Name", text: $name, prompt: Text("deps").foregroundStyle(Palette.textDim))
                            .font(.system(size: 14, design: .monospaced))
                            .autocorrectionDisabled()
                            .accessibilityIdentifier("newbot.name")
                    }
                    FieldRow(label: "What it does") {
                        TextField("What it does", text: $sentence,
                                  prompt: Text("Keeps the app's dependencies up to date").foregroundStyle(Palette.textDim))
                            .font(.system(size: 14))
                            .accessibilityIdentifier("newbot.description")
                    }
                    FieldRow(label: "Workspace", divider: false) {
                        Picker("Workspace", selection: $workspace) {
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
                if !workspace.isEmpty {
                    Text("It runs with \(workspace)'s access: its connections and MCP servers.")
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
        // A Bot's name is lowercase; typing capitals or spaces would only leave Create disabled.
        .onChange(of: name) { _, typed in
            let fixed = typed.lowercased().replacingOccurrences(of: " ", with: "-")
            if fixed != typed { name = fixed }
        }
    }

    /// The picked Bot's Workspace, else the first, once the settings are read.
    private func pickWorkspace() {
        guard !workspaces.contains(workspace) else { return }
        if case let .bot(w, _)? = chrome.botPick, workspaces.contains(w) {
            workspace = w
        } else {
            workspace = workspaces.first ?? ""
        }
    }

    private var workspaces: [String] { model.settings.settings?.workspaces.map(\.name) ?? [] }
    private var trimmedName: String { Lines.trimmed(name) }

    private var nameTaken: Bool {
        model.settings.workspace(workspace)?.bots.contains { $0.name == trimmedName } == true
    }

    private var valid: Bool {
        BotSpec.isValidName(trimmedName) && !nameTaken && !workspace.isEmpty && !model.settings.readOnly
    }

    /// Why Create is disabled, first reason first; else the naming rule.
    private var hint: (text: String, problem: Bool) {
        if model.settings.readOnly {
            return ("The host's settings are read only, so no Bot can be added from here.", true)
        }
        if workspaces.isEmpty {
            if let failure = model.settings.loadError { return ("The settings could not be read: \(failure)", true) }
            return ("Reading the host's Workspaces…", false)
        }
        if nameTaken { return ("\(workspace) has a Bot named \(trimmedName).", true) }
        if !trimmedName.isEmpty && !BotSpec.isValidName(trimmedName) {
            return ("A name is 1 to 64 lowercase letters, digits, - and _.", true)
        }
        return ("Lowercase letters, digits, - and _.", false)
    }

    /// `bot-add`, then the first talk; a talk that cannot start leaves the Bot added.
    private func create() async {
        creating = true
        defer { creating = false }
        let spec = BotSpec(name: trimmedName, description: Lines.optional(sentence))
        if let failure = await model.settings.save([.botAdd(workspace: workspace, bot: spec)]) {
            error = failure
            return
        }
        chrome.botPick = .bot(workspace: workspace, name: spec.name)
        let goal = spec.description.map { "You are \(spec.name): \($0). Ask me what to watch, then propose your Routines." }
        guard let t = await model.startTalk(workspace: workspace, bot: spec.name, title: "Talk to \(spec.name)", goal: goal)
        else {
            error = "\(spec.name) was added, but its chat did not start: \(model.requestError ?? "unknown error")"
            return
        }
        dismiss()
        nav.open(t)
    }
}
