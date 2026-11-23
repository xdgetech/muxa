import MuxaV2

/// New Bot's form, the same on every platform (ADR 0086 decision 3): a name,
/// one sentence and the Workspace whose access it runs with (ADR 0087).
struct NewBotDraft {
    var name = ""
    var sentence = ""
    var workspace = ""

    var trimmedName: String { Lines.trimmed(name) }

    var spec: BotSpec { BotSpec(name: trimmedName, description: Lines.optional(sentence)) }

    /// A Bot's name is lowercase; typing capitals or spaces would only keep Create disabled.
    static func typed(_ text: String) -> String {
        text.lowercased().replacingOccurrences(of: " ", with: "-")
    }

    @MainActor
    func nameTaken(in settings: SettingsModel) -> Bool {
        settings.workspace(workspace)?.bots.contains { $0.name == trimmedName } == true
    }

    @MainActor
    func valid(in settings: SettingsModel) -> Bool {
        BotSpec.isValidName(trimmedName) && !nameTaken(in: settings) && !workspace.isEmpty && !settings.readOnly
    }

    /// Why Create is disabled, first reason first; else the naming rule.
    @MainActor
    func hint(in settings: SettingsModel) -> (text: String, problem: Bool) {
        if settings.readOnly { return ("The host's settings are read only, so no Bot can be added from here.", true) }
        if settings.settings?.workspaces.isEmpty ?? true {
            if let failure = settings.loadError { return ("The settings could not be read: \(failure)", true) }
            return ("Reading the host's Workspaces…", false)
        }
        if nameTaken(in: settings) { return ("\(workspace) has a Bot named \(trimmedName).", true) }
        if !trimmedName.isEmpty && !BotSpec.isValidName(trimmedName) {
            return ("A name is 1 to 64 lowercase letters, digits, - and _.", true)
        }
        return ("Lowercase letters, digits, - and _.", false)
    }

    /// The preferred Workspace if there is one, else the first.
    mutating func pick(from workspaces: [String], preferring preferred: String?) {
        guard !workspaces.contains(workspace) else { return }
        workspace = preferred.flatMap { workspaces.contains($0) ? $0 : nil } ?? workspaces.first ?? ""
    }
}
