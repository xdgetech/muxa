import MuxaV2
import SwiftUI

/// The phone's Bots tab (ADR 0086): the Bots by Workspace and the Routines
/// without a Bot, New Bot behind "+", and a Bot's page pushed over them.
struct PhoneBots: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @State private var path: [BotsDestination] = []
    @State private var addingBot = false

    var body: some View {
        NavigationStack(path: $path) {
            VStack(spacing: 0) {
                ScreenTitle(title: "Bots") {
                    Button("New Bot", systemImage: "plus") { addingBot = true }
                        .labelStyle(.iconOnly)
                        .buttonStyle(RaisedIconStyle(round: true))
                        .disabled(model.settings.settings == nil || model.settings.readOnly)
                        .accessibilityIdentifier("bots.new")
                }
                .padding(.horizontal, 18)
                .padding(.top, 8)
                .padding(.bottom, 6)
                BotsList { path.append($0) }
            }
            .background(Palette.ground)
            .navigationTitle("Bots")
            .toolbar(.hidden, for: .navigationBar)
            .botsDestinations()
        }
        .tint(Palette.accent)
        .sheet(isPresented: $addingBot) { NewBotForm() }
        .onChange(of: nav.botRoute, initial: true) { _, route in follow(route) }
    }

    /// Another place asked for a Bot's page, or one of its terminals on it.
    private func follow(_ route: BotRoute?) {
        guard let route else { return }
        path = [.bot(workspace: route.workspace, name: route.bot)] + (route.terminal.map { [.conversation(terminal: $0)] } ?? [])
        nav.botRoute = nil
    }
}

/// New Bot on iOS: the same form and steps as the Mac's (`NewBotDraft`,
/// `createBot`). Create adds the Bot and opens its first chat on its page.
struct NewBotForm: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.dismiss) private var dismiss
    var preferredWorkspace: String?
    @State private var draft = NewBotDraft()
    @State private var error: String?
    @State private var creating = false

    var body: some View {
        let hint = draft.hint(in: model.settings)
        NavigationStack {
            PaletteForm {
                Section {
                    TextField("Name, e.g. deps", text: $draft.name)
                        .plainEntry()
                        .monospaced()
                        .accessibilityIdentifier("newbot.name")
                    TextField("What it does, in one sentence", text: $draft.sentence)
                        .accessibilityIdentifier("newbot.description")
                    Picker("Workspace", selection: $draft.workspace) {
                        ForEach(workspaces, id: \.self) { Text($0).tag($0) }
                    }
                    .accessibilityIdentifier("newbot.workspace")
                } footer: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(hint.text)
                            .foregroundStyle(hint.problem ? Theme.failure : Palette.textSecondary)
                            .accessibilityIdentifier("newbot.hint")
                        if !draft.workspace.isEmpty {
                            Text("It runs with \(draft.workspace)'s access: its connections and MCP servers.")
                        }
                        Text("It starts with a chat. Tell it what to watch; it proposes Routines you apply.")
                    }
                }
                SaveError(text: error, id: "newbot.error")
            }
            .navigationTitle("New Bot")
            .inlineTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create") { Task { await create() } }
                        .disabled(!draft.valid(in: model.settings) || creating)
                        .accessibilityIdentifier("newbot.create")
                }
            }
        }
        .onAppear {
            draft.pick(from: workspaces, preferring: preferredWorkspace)
            if model.settings.settings == nil { Task { await model.settings.load() } }
        }
        .onChange(of: workspaces) { draft.pick(from: workspaces, preferring: preferredWorkspace) }
        .onChange(of: draft.name) { _, typed in
            let fixed = NewBotDraft.typed(typed)
            if fixed != typed { draft.name = fixed }
        }
    }

    private var workspaces: [String] { model.settings.settings?.workspaces.map(\.name) ?? [] }

    private func create() async {
        creating = true
        defer { creating = false }
        let made = await model.createBot(draft.spec, in: draft.workspace)
        guard let talk = made.talk else {
            error = made.error
            return
        }
        dismiss()
        nav.open(talk)
    }
}
