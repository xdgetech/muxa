import MuxaV2
import SwiftUI

/// Focus's panes side by side: the open Task's agent, then its shells (ADR
/// 0079). Each has a 30 pt header with its state; the active one has the
/// keyboard and, beside other panes, an outline in its state color. The
/// agent's pane switches between its terminal and its Chat. The Pane menu's
/// keys, or Vim's window keys (ADR 0078), add, move between and close panes.
struct PaneSplit: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(MacChrome.self) private var chrome
    /// Opens the loop sheet of a Task.
    let onLoop: (MuxaV2.Terminal) -> Void

    /// The open Task's agent and its live shells, from the host's list. Drawn
    /// from here rather than `nav.panes`, so a Task opens with all its panes in
    /// one layout: laying out the agent alone first attached it at the full
    /// width and resized it again a moment later.
    private var shown: [MuxaV2.Terminal] {
        guard let task = model.terminals.first(where: { $0.id == nav.terminal }) else { return [] }
        return [task] + model.shells(of: task.id)
    }

    var body: some View {
        HSplitView {
            ForEach(Array(shown.enumerated()), id: \.element.id) { index, t in
                pane(t, index: index)
            }
        }
        // One split per Task: its dividers start even, every time it opens.
        .id(nav.terminal)
        .onAppear(perform: sync)
        .onChange(of: model.terminals) { sync() }
        .onChange(of: nav.terminal) { sync() }
    }

    /// The panes follow the host: a shell another device started shows here,
    /// and one that ended goes.
    private func sync() {
        guard let task = nav.terminal else { return }
        guard model.terminals.contains(where: { $0.id == task && $0.live }) else { return nav.closeTask() }
        nav.sync(shells: model.shells(of: task).map(\.id))
    }

    private func pane(_ t: MuxaV2.Terminal, index: Int) -> some View {
        let active = t.id == nav.activePane || (index == 0 && nav.activePane == nil)
        let chat = index == 0 && nav.showChat && t.isTaskAgent
        return VStack(spacing: 0) {
            header(t, index: index, active: active)
            if chat {
                ChatPanel(terminal: t).id("chat.\(t.id)")
                    .background(Palette.surface)
                    .onTapGesture { nav.activate(index) }
            } else {
                TerminalPane(terminal: t, active: active, focusRequest: nav.focusRequest) { nav.activate(index) }
                    .id(t.id)
            }
        }
        .frame(minWidth: 240)
        .background(Palette.ground)
        .overlay {
            if shown.count > 1, active {
                Rectangle().strokeBorder(t.state.color, lineWidth: 1).allowsHitTesting(false)
            }
        }
    }

    private func header(_ t: MuxaV2.Terminal, index: Int, active: Bool) -> some View {
        HStack(spacing: 8) {
            HStack(spacing: 8) {
                StateDot(color: t.state.color, size: 8)
                if let app = t.status?.app, !app.isEmpty {
                    Text(app).fontWeight(.semibold)
                }
                if let state = t.status?.state {
                    Text(TaskState(state).title.lowercased()).foregroundStyle(t.state.color)
                }
                if t.isTaskShell {
                    Text("shell \(index)").fontWeight(.semibold)
                } else {
                    Text(t.title).foregroundStyle(Palette.textSecondary).lineLimit(1)
                }
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("pane.\(index + 1)")
            Spacer(minLength: 8)
            if active, chrome.windowPrefixPending, let prefix = KeyBindings.shared.vimPrefix {
                KeyHint("\(prefix.display) …", color: Palette.accent)
            }
            if t.isTaskShell {
                Button("Close Shell", systemImage: "xmark") {
                    Task { await model.closeShell(t.id) }
                }
                .labelStyle(.iconOnly)
                .buttonStyle(PaneActionStyle(on: false))
                .help("Close this shell (\(Shortcut.closePane.display))")
                .accessibilityIdentifier("pane.close.\(index + 1)")
            } else if index == 0 {
                PaneActions(terminal: t, onLoop: onLoop)
            }
        }
        .font(.system(size: 12))
        .padding(.horizontal, 12)
        .frame(height: 30)
        .overlay(alignment: .bottom) { Rectangle().fill(Palette.border).frame(height: 1) }
    }
}

/// The active pane's Task actions: its loop, and the Chat and Reader panels.
private struct PaneActions: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    let terminal: MuxaV2.Terminal
    let onLoop: (MuxaV2.Terminal) -> Void

    var body: some View {
        HStack(spacing: 4) {
            Button(LoopText.title(model.loops[terminal.id]), systemImage: "arrow.triangle.2.circlepath") {
                onLoop(terminal)
            }
            .buttonStyle(PaneActionStyle(on: false))
            .disabled(!terminal.isTaskAgent)
            .accessibilityIdentifier("focus.loop")
            Button(nav.showChat ? "Terminal" : "Chat",
                   systemImage: nav.showChat ? "terminal" : "bubble.left.and.text.bubble.right") { nav.showChat.toggle() }
                .buttonStyle(PaneActionStyle(on: nav.showChat))
                .disabled(!terminal.isTaskAgent)
                .help("Switch between the terminal and Chat (\(Shortcut.chatPanel.display))")
                .accessibilityIdentifier("focus.chat")
            Button("Reader", systemImage: "doc.text.magnifyingglass") { nav.showReader.toggle() }
                .buttonStyle(PaneActionStyle(on: nav.showReader))
                .disabled(terminal.taskDir == nil)
                .help("Reader (\(Shortcut.readerPanel.display))")
                .accessibilityIdentifier("focus.reader")
        }
    }
}

/// A small icon-and-text button in a pane header; filled while its panel is open.
private struct PaneActionStyle: ButtonStyle {
    let on: Bool
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .labelStyle(PaneActionLabel())
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(on ? Palette.text : Palette.textSecondary)
            .padding(.horizontal, 7)
            .frame(height: 22)
            .background(on ? Palette.rowSelected : .clear, in: RoundedRectangle(cornerRadius: 6))
            .contentShape(Rectangle())
            .opacity(enabled ? (configuration.isPressed ? 0.6 : 1) : 0.35)
    }
}

private struct PaneActionLabel: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 5) {
            configuration.icon.font(.system(size: 11, weight: .semibold))
            configuration.title
        }
    }
}
