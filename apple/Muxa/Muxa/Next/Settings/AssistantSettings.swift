import MuxaV2
import SwiftUI

/// The Capture assistant (ADR 0084): `[assistant]` in `workspaces.toml`, its
/// Soul, and the Memory it writes. An older host sends none: the form starts
/// turned off.
struct AssistantSettings: View {
    @Environment(NextModel.self) private var model
    @State private var enabled = false
    @State private var worker = ""
    @State private var soul = ""
    /// The Soul as read; nil when it could not be read.
    @State private var readSoul: String?
    @State private var error: String?
    @State private var saved = false

    var body: some View {
        SettingsPage {
            Section {
                Toggle("Enabled", isOn: $enabled)
                    .accessibilityIdentifier("assistant.enabled")
                Picker("Worker", selection: $worker) {
                    Text("The default agent").tag("")
                    ForEach(model.settings.settings?.workers ?? []) { Text($0.name).tag($0.name) }
                }
                .accessibilityIdentifier("assistant.worker")
            } footer: {
                Text("Each capture gets one short run. It suggests a kind, a Workspace, a due date, a split, a Task or a Bot to ask. "
                    + "It has no Workspace and no connections.")
            }
            Section {
                TextEditor(text: $soul)
                    .font(.system(.callout, design: .monospaced))
                    .frame(minHeight: 140)
                    .scrollContentBackground(.hidden)
                    .accessibilityIdentifier("assistant.soul")
            } header: {
                Text("Soul")
            } footer: {
                Text("How it sorts what you capture. Every run starts from it.")
            }
            Section {
                Button("Save") { Task { await save() } }
                    .disabled(model.settings.saving || model.settings.settings == nil)
                    .accessibilityIdentifier("assistant.save")
                if saved { Text("Saved.").foregroundStyle(Palette.textSecondary).accessibilityIdentifier("assistant.saved") }
                SaveError(text: error, id: "assistant.error")
            }
            Section {
                Text(memory)
                    .font(.system(.callout, design: .monospaced))
                    .foregroundStyle(Palette.textSecondary)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("assistant.memory")
            } header: {
                Text("Memory")
            } footer: {
                Text("The assistant writes this from the suggestions you accept and dismiss.")
            }
        }
        .navigationTitle("Capture assistant")
        .inlineTitle()
        .task(id: model.settings.settings?.revision) { show(model.settings.settings?.assistant) }
    }

    private var memory: String {
        guard let a = model.settings.settings?.assistant else { return "Nothing yet." }
        return a.memoryText.map { $0.isEmpty ? "Nothing yet." : $0 } ?? "The memory cannot be read."
    }

    private func show(_ a: SettingsAssistant?) {
        enabled = a?.enabled ?? false
        worker = a?.worker ?? ""
        soul = a?.soulText ?? ""
        readSoul = a?.soulText
    }

    private func save() async {
        // An unchanged Soul is not sent, so a file the host could not read stays as it is.
        let text = soul == (readSoul ?? "") ? nil : soul
        error = await model.settings.save([.assistantUpdate(enabled: enabled, worker: Lines.optional(worker), soulText: text)])
        saved = error == nil
    }
}
