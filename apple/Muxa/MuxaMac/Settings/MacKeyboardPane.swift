import AppKit
import MuxaSession
import SwiftUI

/// Settings › This Mac › Keyboard (ADR 0078): every rebindable shortcut with a
/// key recorder, and Vim's window keys with their prefix.
struct MacKeyboardPane: View {
    private let keys = KeyBindings.shared

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                MacPaneTitle(title: "Keyboard",
                             subtitle: "The keys start as in iTerm2 and VS Code. Click a key to record a new one.")
                VStack(alignment: .leading, spacing: 22) {
                    vimSection
                    ForEach(Shortcut.Group.allCases, id: \.self) { group in
                        section(group)
                    }
                    Button("Reset All Keys") { keys.resetAll() }
                        .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
                        .accessibilityIdentifier("keys.resetall")
                }
                .padding(.horizontal, 28)
                .padding(.vertical, 18)
                .frame(maxWidth: MacSettingsLayout.formWidth, alignment: .leading)
            }
        }
    }

    private var vimSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel(title: "Vim window keys")
            Toggle("Use a prefix for pane commands, as Vim's CTRL-W", isOn: Binding(
                get: { keys.vimPrefix != nil },
                set: { keys.setVimPrefix($0 ? KeyBindings.defaultPrefix : nil) }
            ))
            .toggleStyle(.switch)
            .accessibilityIdentifier("keys.vim")
            if let prefix = keys.vimPrefix {
                KeyRow(title: "Prefix", id: KeyBindings.prefixID, chord: prefix, custom: false,
                       record: { keys.setVimPrefix($0 ?? KeyBindings.defaultPrefix) }, reset: nil)
                Text(Self.vimHelp(prefix.display))
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    static func vimHelp(_ prefix: String) -> String {
        "In a terminal in Focus, press \(prefix), then: v or s split, h or ← previous, l, w or → next, "
            + "c or q close, o close the others, 1–9 a pane. \(prefix) twice sends \(prefix) to the terminal."
    }

    private func section(_ group: Shortcut.Group) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            SectionLabel(title: group.rawValue).padding(.bottom, 4)
            ForEach(Shortcut.all.filter { $0.group == group }) { s in
                if s.rebindable {
                    KeyRow(title: s.title, id: s.id, chord: keys.chord(for: s), custom: keys.isCustom(s),
                           record: { keys.set($0, for: s) }, reset: { keys.reset(s) })
                } else {
                    HStack {
                        Text(s.title).foregroundStyle(Palette.textMid)
                        Spacer()
                        KeyHint(s.display)
                    }
                    .frame(minHeight: 30)
                    .help("This key is fixed.")
                }
            }
        }
    }
}

/// One shortcut: its name, a button that records the next key press, and Reset.
private struct KeyRow: View {
    let title: String
    let id: String
    let chord: KeyChord?
    let custom: Bool
    /// Nil clears the key.
    let record: (KeyChord?) -> Void
    let reset: (() -> Void)?
    @State private var recording = false
    @State private var monitor: Any?

    var body: some View {
        VStack(alignment: .trailing, spacing: 2) {
            HStack(spacing: 8) {
                Text(title).foregroundStyle(Palette.textMid)
                Spacer()
                if custom, let reset {
                    Button("Reset", action: reset)
                        .buttonStyle(.plain)
                        .foregroundStyle(Palette.accent)
                        .accessibilityIdentifier("keys.reset.\(id)")
                }
                Button(action: toggle) {
                    Text(recording ? "Type a key…" : chord?.display ?? "None")
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(recording ? Palette.accent : chord == nil ? Palette.textTertiary : Palette.text)
                        .frame(minWidth: 90)
                        .padding(.horizontal, 8)
                        .frame(height: 24)
                        .background(Palette.ground, in: RoundedRectangle(cornerRadius: 5))
                        .overlay(RoundedRectangle(cornerRadius: 5)
                            .strokeBorder(recording ? Palette.accent : Palette.borderStrong))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("\(title) key")
                .accessibilityValue(chord?.display ?? "None")
                .accessibilityIdentifier("keys.key.\(id)")
            }
            if recording {
                Text("With ⌘, ⌃ or ⌥. Esc cancels; ⌫ removes the key.")
                    .font(.system(size: 11.5))
                    .foregroundStyle(Palette.textTertiary)
            } else if let chord, !others(chord).isEmpty {
                Text("Also used by \(others(chord).joined(separator: ", "))")
                    .font(.system(size: 11.5))
                    .foregroundStyle(Palette.late)
            }
        }
        .frame(minHeight: 30)
        .onDisappear(perform: stop)
    }

    private func others(_ chord: KeyChord) -> [String] {
        KeyBindings.shared.users(of: chord, except: id)
    }

    private func toggle() {
        if recording { stop() } else { start() }
    }

    /// The next key press becomes the key: Esc cancels, ⌫ alone clears it.
    private func start() {
        recording = true
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            guard let chord = KeyChord(event) else { return event }
            MainActor.assumeIsolated { take(chord) }
            return nil
        }
    }

    private func take(_ chord: KeyChord) {
        switch chord {
        case KeyChord(.escape): break
        case KeyChord(.delete): record(nil)
        case _ where Self.usable(chord): record(chord)
        // A plain letter would fire while the user types; wait for one with a modifier.
        default: return
        }
        stop()
    }

    /// F-keys alone, or any key with ⌘, ⌃ or ⌥.
    private static func usable(_ chord: KeyChord) -> Bool {
        if case .named(let n) = chord.key, n.rawValue.hasPrefix("f") { return true }
        return !chord.modifiers.subtracting(.shift).isEmpty
    }

    private func stop() {
        recording = false
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }
}
