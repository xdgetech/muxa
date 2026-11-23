import MuxaSession
import SwiftUI

/// Every Mac shortcut, once. The menus take their keys from here and the
/// Keyboard Shortcuts sheet lists the table, so the two cannot drift apart.
/// A rebindable one takes the user's key from `KeyBindings` (ADR 0078).
struct Shortcut: Identifiable {
    enum Group: String, CaseIterable {
        case go = "Go"
        case capture = "Capture and Tasks"
        case inbox = "Inbox"
        case panes = "Focus panes"
    }

    let id: String
    let group: Group
    let title: String
    /// The key a menu item starts with; nil for none.
    let defaultChord: KeyChord?
    /// A key a view handles, as the sheet prints it; such a key is not rebindable.
    let fixed: String?

    private init(_ id: String, _ group: Group, _ title: String, _ chord: KeyChord?) {
        self.id = id
        self.group = group
        self.title = title
        defaultChord = chord
        fixed = nil
    }

    private init(_ id: String, _ group: Group, _ title: String, fixed: String) {
        self.id = id
        self.group = group
        self.title = title
        defaultChord = nil
        self.fixed = fixed
    }

    var rebindable: Bool { fixed == nil }

    /// The key now: the user's, else the default.
    @MainActor var chord: KeyChord? { rebindable ? KeyBindings.shared.chord(for: self) : nil }

    /// As the sheet and the hints print it; empty when it has no key.
    @MainActor var display: String { fixed ?? chord?.display ?? "" }

    static let home = Shortcut("home", .go, "Home", KeyChord("h", [.command, .shift]))
    static let focus = Shortcut("focus", .go, "Focus", KeyChord("f", [.command, .shift]))
    static let goToTask = Shortcut("goto", .go, "Go to Task", KeyChord("p", .command))
    // No function keys by default: on a laptop they need Fn (ADR 0078).
    static let nextNeedsYou = Shortcut("next-needs-you", .go, "Next Task that needs you", KeyChord("j", .command))
    static let nextTask = Shortcut("next-task", .go, "Next Task", KeyChord(.tab, .control))
    static let previousNeedsYou = Shortcut("previous-needs-you", .go, "Previous Task that needs you", KeyChord("j", [.command, .shift]))
    static let previousTask = Shortcut("previous-task", .go, "Previous Task", KeyChord(.tab, [.control, .shift]))
    static let sidebar = Shortcut("sidebar", .go, "Toggle sidebar", KeyChord("b", .command))
    static let routines = Shortcut("routines", .go, "Routines", KeyChord("o", [.command, .shift]))
    static let capture = Shortcut("capture", .capture, "Capture", KeyChord("n", .command))
    static let captureAnywhere = Shortcut("capture-anywhere", .capture, "Capture from anywhere", KeyChord(.space, [.control, .option]))
    static let newTask = Shortcut("new-task", .capture, "New Task", KeyChord("n", [.command, .shift]))
    static let settings = Shortcut("settings", .capture, "Settings", fixed: "⌘,")
    static let shortcuts = Shortcut("shortcuts", .capture, "Keyboard shortcuts", KeyChord("/", .command))
    static let open = Shortcut("open", .inbox, "Approve or Done", fixed: "⌘⏎")
    static let later = Shortcut("later", .inbox, "Later", fixed: "⌘L")
    static let done = Shortcut("done", .inbox, "Done", fixed: "⌘⌫")
    static let retry = Shortcut("retry", .inbox, "Request changes or Retry", fixed: "⌘R")
    static let move = Shortcut("move", .inbox, "Move up / down", fixed: "⌥↑ ⌥↓")
    // iTerm2's pane keys; VS Code's `⌘\` clashes with 1Password and is hard to type on a JIS keyboard.
    static let split = Shortcut("split", .panes, "Split pane", KeyChord("d", .command))
    static let nextPane = Shortcut("next-pane", .panes, "Next pane", KeyChord("]", .command))
    static let previousPane = Shortcut("previous-pane", .panes, "Previous pane", KeyChord("[", .command))
    // In Focus ⌘W closes the active shell, as in iTerm2; the agent's pane stays, and so does the window.
    static let closePane = Shortcut("close-pane", .panes, "Close shell, or the window outside Focus", KeyChord("w", .command))
    static let closeTask = Shortcut("close-task", .panes, "Close Task…", nil)
    static let closeOtherPanes = Shortcut("close-other-panes", .panes, "Close other shells", nil)
    static let pane = Shortcut("pane", .panes, "Pane 1–9", fixed: "⌘1 … ⌘9")
    static let inboxPanel = Shortcut("inbox-panel", .panes, "Inbox panel", KeyChord("i", [.command, .shift]))
    static let readerPanel = Shortcut("reader-panel", .panes, "Reader panel", KeyChord("r", [.command, .shift]))
    static let chatPanel = Shortcut("chat-panel", .panes, "Chat panel", KeyChord("c", [.command, .shift]))

    static let all: [Shortcut] = [
        home, focus, goToTask, nextNeedsYou, previousNeedsYou, nextTask, previousTask, sidebar, routines,
        capture, captureAnywhere, newTask, settings, shortcuts,
        open, later, done, retry, move,
        split, nextPane, previousPane, closePane, closeOtherPanes, closeTask, pane, inboxPanel, readerPanel, chatPanel,
    ]
}

extension View {
    /// The table's key on a menu item, as the user set it; nothing for a key a view handles.
    @MainActor @ViewBuilder
    func shortcut(_ s: Shortcut) -> some View {
        if let chord = s.chord { keyboardShortcut(chord.keyboardShortcut) } else { self }
    }
}

/// Help → Keyboard Shortcuts (`⌘/`): the table in its groups.
struct ShortcutsSheet: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Keyboard shortcuts").font(.system(size: 17, weight: .bold))
                Spacer()
                Text("Change them in Settings › Keyboard").foregroundStyle(Palette.textTertiary)
                Button("Close") { dismiss() }.keyboardShortcut(.cancelAction)
                    .buttonStyle(ActionButtonStyle(kind: .secondary, compact: true))
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 16)
            .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1) }
            LazyVGrid(columns: [GridItem(.flexible(), spacing: 28, alignment: .topLeading),
                                GridItem(.flexible(), alignment: .topLeading)], alignment: .leading, spacing: 18) {
                ForEach(Shortcut.Group.allCases, id: \.self) { group in
                    VStack(alignment: .leading, spacing: 2) {
                        SectionLabel(title: group.rawValue).padding(.bottom, 4)
                        ForEach(Shortcut.all.filter { $0.group == group }) { s in
                            HStack {
                                Text(s.title).foregroundStyle(Palette.textMid)
                                Spacer()
                                Text(s.display.isEmpty ? "—" : s.display)
                                    .font(.system(size: 12, design: .monospaced))
                                    .padding(.horizontal, 7)
                                    .padding(.vertical, 1)
                                    .background(Palette.ground, in: RoundedRectangle(cornerRadius: 5))
                                    .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(Palette.borderStrong))
                            }
                            .frame(minHeight: 28)
                            .accessibilityElement(children: .combine)
                        }
                    }
                }
            }
            .padding(20)
            if let prefix = KeyBindings.shared.vimPrefix {
                VStack(alignment: .leading, spacing: 6) {
                    SectionLabel(title: "Vim window keys")
                    Text(MacKeyboardPane.vimHelp(prefix.display)).foregroundStyle(Palette.textMid)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.horizontal, 20)
            }
            Spacer(minLength: 0)
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .frame(width: 700, height: 680)
        .background(Palette.surface)
        .accessibilityIdentifier("shortcuts.sheet")
    }
}
