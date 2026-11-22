import SwiftUI

/// Every Mac shortcut, once. The menus take their keys from here and the
/// Keyboard Shortcuts sheet lists the table, so the two cannot drift apart.
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
    /// As the sheet prints it.
    let display: String
    /// Nil when the key is not a menu item (handled in a view).
    let key: KeyboardShortcut?

    static let home = Shortcut(id: "home", group: .go, title: "Home", display: "⌘⇧H", key: .init("h", modifiers: [.command, .shift]))
    static let focus = Shortcut(id: "focus", group: .go, title: "Focus", display: "⌘⇧F", key: .init("f", modifiers: [.command, .shift]))
    static let goToTask = Shortcut(id: "goto", group: .go, title: "Go to Task", display: "⌘P", key: .init("p", modifiers: .command))
    static let nextNeedsYou = Shortcut(id: "next-needs-you", group: .go, title: "Next Task that needs you", display: "F8",
                                       key: .init(KeyEquivalent(Character(UnicodeScalar(NSF8FunctionKey)!)), modifiers: []))
    static let nextTask = Shortcut(id: "next-task", group: .go, title: "Next Task", display: "⌃Tab", key: .init(.tab, modifiers: .control))
    static let sidebar = Shortcut(id: "sidebar", group: .go, title: "Toggle sidebar", display: "⌘B", key: .init("b", modifiers: .command))
    static let routines = Shortcut(id: "routines", group: .go, title: "Routines", display: "⌘⇧O", key: .init("o", modifiers: [.command, .shift]))
    static let capture = Shortcut(id: "capture", group: .capture, title: "Capture", display: "⌘N", key: .init("n", modifiers: .command))
    static let captureAnywhere = Shortcut(id: "capture-anywhere", group: .capture, title: "Capture from anywhere",
                                          display: "⌃⌥Space", key: .init(.space, modifiers: [.control, .option]))
    static let newTask = Shortcut(id: "new-task", group: .capture, title: "New Task", display: "⌘⇧N", key: .init("n", modifiers: [.command, .shift]))
    static let settings = Shortcut(id: "settings", group: .capture, title: "Settings", display: "⌘,", key: nil)
    static let shortcuts = Shortcut(id: "shortcuts", group: .capture, title: "Keyboard shortcuts", display: "⌘/", key: .init("/", modifiers: .command))
    static let open = Shortcut(id: "open", group: .inbox, title: "Approve or Done", display: "⌘⏎", key: nil)
    static let later = Shortcut(id: "later", group: .inbox, title: "Later", display: "⌘L", key: nil)
    static let done = Shortcut(id: "done", group: .inbox, title: "Done", display: "⌘⌫", key: nil)
    static let retry = Shortcut(id: "retry", group: .inbox, title: "Request changes or Retry", display: "⌘R", key: nil)
    static let move = Shortcut(id: "move", group: .inbox, title: "Move up / down", display: "⌥↑ ⌥↓", key: nil)
    static let split = Shortcut(id: "split", group: .panes, title: "Split pane", display: "⌘\\", key: .init("\\", modifiers: .command))
    static let pane = Shortcut(id: "pane", group: .panes, title: "Pane 1–9", display: "⌘1 … ⌘9", key: nil)
    static let inboxPanel = Shortcut(id: "inbox-panel", group: .panes, title: "Inbox panel", display: "⌘⇧I", key: .init("i", modifiers: [.command, .shift]))
    static let readerPanel = Shortcut(id: "reader-panel", group: .panes, title: "Reader panel", display: "⌘⇧R", key: .init("r", modifiers: [.command, .shift]))
    static let chatPanel = Shortcut(id: "chat-panel", group: .panes, title: "Chat panel", display: "⌘⇧C", key: .init("c", modifiers: [.command, .shift]))

    static let all: [Shortcut] = [
        home, focus, goToTask, nextNeedsYou, nextTask, sidebar, routines,
        capture, captureAnywhere, newTask, settings, shortcuts,
        open, later, done, retry, move,
        split, pane, inboxPanel, readerPanel, chatPanel,
    ]
}

extension View {
    /// The table's key on a menu item; nothing for a key a view handles.
    @ViewBuilder
    func shortcut(_ s: Shortcut) -> some View {
        if let key = s.key { keyboardShortcut(key) } else { self }
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
                Text("Also in each menu").foregroundStyle(Palette.textTertiary)
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
                                Text(s.display)
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
            Spacer(minLength: 0)
        }
        .font(.system(size: 13))
        .foregroundStyle(Palette.text)
        .frame(width: 700, height: 580)
        .background(Palette.surface)
        .accessibilityIdentifier("shortcuts.sheet")
    }
}
