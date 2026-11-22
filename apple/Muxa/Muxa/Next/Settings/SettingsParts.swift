import MuxaV2
import SwiftUI

/// The banner above Settings: the host's lock, a conflict, or why a file
/// cannot be used as it is.
struct SettingsNotice: View {
    @Environment(NextModel.self) private var model

    var body: some View {
        let s = model.settings
        if let text = s.loadError ?? s.notice ?? (s.readOnly ? SettingsModel.lockedNotice : nil) {
            notice(text, color: Theme.attention)
        }
        if let problem = s.settings?.problem {
            notice("workspaces.toml cannot be used as it is: \(problem)", color: Theme.failure)
        }
    }

    private func notice(_ text: String, color: Color) -> some View {
        Label(text, systemImage: "exclamationmark.triangle")
            .font(.callout)
            .foregroundStyle(color)
            .accessibilityIdentifier("settings.notice")
    }
}

/// A save's error under a form, in the server's words.
struct SaveError: View {
    let text: String?
    var id = "editor.error"

    var body: some View {
        if let text {
            Text(text).font(.callout).foregroundStyle(Theme.failure).accessibilityIdentifier(id)
        }
    }
}

/// A sheet that edits one thing: Cancel, Save, the server's refusal inline,
/// and Remove for a thing that exists.
struct SettingsEditor<Content: View>: View {
    @Environment(NextModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let title: String
    var canSave = true
    /// The ops of Save; nil while the form is incomplete.
    let ops: () -> [SettingsOp]?
    /// The op of Remove, with what the confirmation says.
    var remove: (op: SettingsOp, warning: String)?
    /// After a save that applied: clears what must not stay on screen.
    var onSaved: () -> Void = {}
    let content: Content
    @State private var error: String?
    @State private var confirmRemove = false

    init(title: String, canSave: Bool = true, ops: @escaping () -> [SettingsOp]?,
         remove: (op: SettingsOp, warning: String)? = nil, onSaved: @escaping () -> Void = {},
         @ViewBuilder content: () -> Content) {
        self.title = title
        self.canSave = canSave
        self.ops = ops
        self.remove = remove
        self.onSaved = onSaved
        self.content = content()
    }

    var body: some View {
        NavigationStack {
            PaletteForm {
                content
                if let remove {
                    Section {
                        Button("Remove", role: .destructive) { confirmRemove = true }
                            .accessibilityIdentifier("editor.remove")
                    } footer: {
                        Text(remove.warning)
                    }
                }
                SaveError(text: error)
            }
            .formStyle(.grouped)
            .disabled(model.settings.readOnly)
            .navigationTitle(title)
            .inlineTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { Task { await save(ops()) } }
                        .disabled(!canSave || ops() == nil || model.settings.saving || model.settings.readOnly)
                        .accessibilityIdentifier("editor.save")
                }
            }
            .confirmationDialog("Remove \(title)?", isPresented: $confirmRemove, titleVisibility: .visible) {
                Button("Remove", role: .destructive) { Task { await save(remove.map { [$0.op] }) } }
                    .accessibilityIdentifier("editor.confirmremove")
            } message: {
                Text(remove?.warning ?? "")
            }
        }
        .presentationBackground(Palette.ground)
        #if os(macOS)
        .frame(minWidth: 480, minHeight: 520)
        #endif
    }

    private func save(_ ops: [SettingsOp]?) async {
        guard let ops else { return }
        error = await model.settings.save(ops)
        if error == nil {
            onSaved()
            dismiss()
        }
    }
}

/// Who gets a connection or MCP server now, and when an agent last started with it.
struct UsageLine: View {
    let usedBy: UsedBy?
    let lastUsed: Int64?

    var body: some View {
        Text(Self.text(usedBy: usedBy, lastUsed: lastUsed))
            .font(.caption)
            .foregroundStyle(Palette.textTertiary)
    }

    static func text(usedBy: UsedBy?, lastUsed: Int64?) -> String {
        var who: [String] = []
        if let r = usedBy?.routines, !r.isEmpty { who.append(r.joined(separator: ", ")) }
        if let t = usedBy?.tasks.count, t > 0 { who.append(t == 1 ? "1 Task" : "\(t) Tasks") }
        let used = who.isEmpty ? "Used by no Routine or Task now" : "Used by " + who.joined(separator: " · ")
        guard let lastUsed else { return used + " · never used" }
        let when = RelativeDateTimeFormatter().localizedString(
            for: Date(timeIntervalSince1970: TimeInterval(lastUsed)), relativeTo: .now)
        return used + " · last used \(when)"
    }
}

/// A short colored word: a secret's state, an access label.
struct StateTag: View {
    let text: String
    var color = Palette.textSecondary

    var body: some View {
        Text(text)
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(color)
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(Palette.raised, in: RoundedRectangle(cornerRadius: 5))
    }
}

extension SecretState {
    var color: Color {
        switch self {
        case .set: Theme.connected
        case .missing, .unknown: Theme.failure
        case .locked, .unavailable: Theme.attention
        }
    }
}

/// Text of one entry per line, as forms edit lists and maps.
enum Lines {
    static func split(_ text: String) -> [String] {
        text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }

    static func join(_ list: [String]?) -> String { (list ?? []).joined(separator: "\n") }

    /// `KEY<separator>value` per line; a line without the separator is dropped.
    static func pairs(_ text: String, separator: Character) -> [String: String] {
        var out: [String: String] = [:]
        for line in split(text) {
            guard let at = line.firstIndex(of: separator) else { continue }
            let key = line[..<at].trimmingCharacters(in: .whitespaces)
            if !key.isEmpty { out[key] = line[line.index(after: at)...].trimmingCharacters(in: .whitespaces) }
        }
        return out
    }

    static func join(_ map: [String: String], separator: String) -> String {
        map.keys.sorted().map { "\($0)\(separator)\(map[$0] ?? "")" }.joined(separator: "\n")
    }

    static func trimmed(_ s: String) -> String { s.trimmingCharacters(in: .whitespacesAndNewlines) }

    /// Nil for an empty field.
    static func optional(_ s: String) -> String? {
        let t = trimmed(s)
        return t.isEmpty ? nil : t
    }
}
