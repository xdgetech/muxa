import MuxaV2
import SwiftUI

/// A Bot's settings on its page: the Bot editor's sections inline with Save,
/// then what it may use, then Remove. The Soul stays in Files.
struct BotSettingsTab: View {
    @Environment(NextModel.self) private var model
    @Environment(MacChrome.self) private var chrome
    let entry: BotEntry
    @State private var draft: BotDraft
    @State private var error: String?

    init(entry: BotEntry) {
        self.entry = entry
        _draft = State(initialValue: BotDraft(entry.bot))
    }

    var body: some View {
        PaletteForm {
            BotSettingsSections(draft: $draft, workspace: entry.workspace)
            Section {
                HStack {
                    Button("Save") { Task { await save() } }
                        .keyboardShortcut("s", modifiers: .command)
                        .disabled(draft.spec() == nil || !changed || model.settings.saving || model.settings.readOnly)
                        .accessibilityIdentifier("bot.settings.save")
                    if changed {
                        Button("Revert") { draft = BotDraft(entry.bot) }
                            .accessibilityIdentifier("bot.settings.revert")
                    }
                }
                SaveError(text: error, id: "bot.settings.error")
            }
            Section {
                BotAccessPanel(workspace: entry.workspace, commands: entry.bot.spec.commands)
            }
            BotRemovalSection(workspace: entry.workspace, bot: entry.bot) { chrome.botPick = nil }
        }
        .formStyle(.grouped)
        .disabled(model.settings.readOnly)
        .onChange(of: entry.bot.spec) { _, _ in draft = BotDraft(entry.bot) }
    }

    /// The form differs from the settings. A line marked in red keeps Save off.
    private var changed: Bool {
        guard let spec = draft.spec() else { return true }
        return spec != entry.bot.spec
    }

    private func save() async {
        guard let spec = draft.spec() else { return }
        error = await model.settings.save([.botUpdate(workspace: entry.workspace.name, bot: spec)])
    }
}
