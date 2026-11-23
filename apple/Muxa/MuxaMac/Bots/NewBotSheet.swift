import MuxaV2
import SwiftUI

/// New Bot (ADR 0086 decision 3): a name, one sentence, the Workspace and
/// its worker. Create adds the Bot, starts its first chat and opens it in
/// Focus; its Routines come from that chat as proposals.
struct NewBotSheet: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(MacChrome.self) private var chrome
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var sentence = ""
    @State private var workspace = ""
    @State private var worker = ""
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
                    FieldRow(label: "Workspace") {
                        Picker("Workspace", selection: $workspace) {
                            ForEach(workspaces, id: \.self) { Text($0).tag($0) }
                        }
                        .labelsHidden()
                        .fixedSize()
                        .accessibilityIdentifier("newbot.workspace")
                    }
                    FieldRow(label: "Runs with", divider: false) {
                        Picker("Runs with", selection: $worker) {
                            Text("The Workspace's agent").tag("")
                            ForEach(model.settings.settings?.workers ?? []) { Text($0.name).tag($0.name) }
                        }
                        .labelsHidden()
                        .fixedSize()
                        .accessibilityIdentifier("newbot.worker")
                    }
                }
                .raisedCard()
                Text(nameHint).font(.system(size: 12)).foregroundStyle(nameTaken ? Palette.late : Palette.textTertiary)
                SaveError(text: error, id: "newbot.error")
            }
        }
        .onAppear(perform: pickWorkspace)
        .onChange(of: workspaces) { pickWorkspace() }
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

    private var nameHint: String {
        nameTaken ? "\(workspace) has a Bot named \(trimmedName)." : "Lowercase letters, digits, - and _."
    }

    /// `bot-add`, then the first talk; a talk that cannot start leaves the Bot added.
    private func create() async {
        creating = true
        defer { creating = false }
        let spec = BotSpec(name: trimmedName, description: Lines.optional(sentence), worker: Lines.optional(worker))
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
