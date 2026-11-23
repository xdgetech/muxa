import MuxaV2
import SwiftUI

/// `⌘P`, as VS Code's quick open: type part of a Task's name, `↑`/`↓` pick a
/// row, `⏎` opens it in Focus, `esc` closes. Tasks that need the user come
/// first. A Task's shells (ADR 0079) and other terminals show only once the
/// text matches them, as the lists keep them closed; a shell opens its Task
/// with the shell's pane active. The Mac floats it over the window; iOS shows
/// it as a sheet.
struct GoToTask: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @State private var text = ""
    @State private var selected = 0
    @FocusState private var focused: Bool

    private var matches: [MuxaV2.Terminal] {
        let tasks = model.needsYou + model.liveTasks.filter { !model.needsYou.contains($0) }
        let query = text.trimmingCharacters(in: .whitespaces).lowercased()
        guard !query.isEmpty else { return tasks }
        let shells = tasks.flatMap { model.shells(of: $0.id) }
        return (tasks + shells + model.otherTerminals).filter { label($0).lowercased().contains(query) }
    }

    var body: some View {
        let rows = matches
        VStack(spacing: 0) {
            field(rows)
            Rectangle().fill(Palette.border).frame(height: 1)
            if rows.isEmpty {
                Text(text.isEmpty ? "No live Tasks" : "Nothing matches “\(text)”")
                    .font(.system(size: 13))
                    .foregroundStyle(Palette.textTertiary)
                    .frame(maxWidth: .infinity, minHeight: 64)
            } else {
                list(rows)
            }
            Rectangle().fill(Palette.border).frame(height: 1)
            footer
        }
        .foregroundStyle(Palette.text)
        .background(Palette.surface)
        .presentationBackground(Palette.surface)
        .onAppear(perform: takeKeyboard)
        .onChange(of: text) { selected = 0 }
        #if os(macOS)
        .onExitCommand(perform: close)
        #endif
    }

    private func field(_ rows: [MuxaV2.Terminal]) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Palette.textTertiary)
            TextField("Go to Task", text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 15))
                .focused($focused)
                .onSubmit { open(rows) }
                .onKeyPress(.downArrow) { move(1, in: rows) }
                .onKeyPress(.upArrow) { move(-1, in: rows) }
                .accessibilityIdentifier("goto.field")
        }
        .padding(.horizontal, 14)
        .frame(height: 48)
    }

    private func list(_ rows: [MuxaV2.Terminal]) -> some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 2) {
                    ForEach(Array(rows.enumerated()), id: \.element.id) { index, t in
                        row(t, on: index == min(selected, rows.count - 1))
                            .id(t.id)
                            .onTapGesture { pick(t) }
                    }
                }
                .padding(6)
            }
            // As tall as its rows, up to 360 pt: a short list leaves no empty space.
            .frame(height: min(CGFloat(rows.count) * Self.rowHeight + 12, 360))
            .onChange(of: selected) {
                if rows.indices.contains(selected) { proxy.scrollTo(rows[selected].id) }
            }
        }
    }

    private func row(_ t: MuxaV2.Terminal, on: Bool) -> some View {
        HStack(spacing: 10) {
            StateDot(color: t.isTaskShell ? Palette.textTertiary : t.state.color, size: 8)
            VStack(alignment: .leading, spacing: 1) {
                Text(label(t)).font(.system(size: 13.5, weight: on ? .semibold : .regular)).lineLimit(1)
                if let detail = detail(t) {
                    Text(detail).font(.system(size: 11.5)).foregroundStyle(Palette.textTertiary).lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            if let state = t.status?.state {
                Text(TaskState(state).title.lowercased())
                    .font(.system(size: 11.5))
                    .foregroundStyle(model.needsYou.contains(t) ? Theme.attention : t.state.color)
            }
        }
        .padding(.horizontal, 10)
        .frame(height: Self.rowHeight - 2)
        .background(on ? Palette.raised : .clear, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(on ? Palette.rowBorder : .clear))
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(on ? [.isButton, .isSelected] : .isButton)
        // Read as the value too: a combined element on iOS does not report the trait.
        .accessibilityValue(on ? "selected" : "")
        .accessibilityIdentifier("goto.\(t.id)")
    }

    /// A row and the 2 pt between rows.
    private static let rowHeight: CGFloat = 42

    private var footer: some View {
        HStack(spacing: 14) {
            hint("↑↓", "select")
            hint("⏎", "open")
            hint("esc", "close")
            Spacer()
        }
        .padding(.horizontal, 14)
        .frame(height: 30)
        .background(Palette.pane)
    }

    private func hint(_ key: String, _ what: String) -> some View {
        HStack(spacing: 5) {
            Text(key).font(.system(size: 11, design: .monospaced)).foregroundStyle(Palette.text)
            Text(what).font(.system(size: 11.5)).foregroundStyle(Palette.textSecondary)
        }
    }

    // MARK: Picking

    private func move(_ step: Int, in rows: [MuxaV2.Terminal]) -> KeyPress.Result {
        guard !rows.isEmpty else { return .handled }
        selected = (min(selected, rows.count - 1) + step + rows.count) % rows.count
        return .handled
    }

    private func open(_ rows: [MuxaV2.Terminal]) {
        guard !rows.isEmpty else { return }
        pick(rows[min(selected, rows.count - 1)])
    }

    private func pick(_ t: MuxaV2.Terminal) {
        if t.isTaskShell, let agent = agent(of: t) {
            nav.open(agent)
            nav.sync(shells: model.shells(of: agent.id).map(\.id), focus: t.id)
        } else {
            nav.open(t)
        }
        close()
    }

    /// On the Mac the terminal, an AppKit view, keeps the keyboard until it
    /// lets go; the field takes it a moment later.
    private func takeKeyboard() {
        #if os(macOS)
        NSApp.keyWindow?.makeFirstResponder(nil)
        #endif
        DispatchQueue.main.async { focused = true }
    }

    /// Closing without a pick gives the keyboard back to the terminal.
    private func close() {
        nav.showPalette = false
        nav.refocus()
    }

    // MARK: Names

    /// A shell is named after its Task, so "fix shell" finds fix-login's shells.
    private func label(_ t: MuxaV2.Terminal) -> String {
        guard t.isTaskShell, let agent = agent(of: t) else { return t.title }
        let n = (model.shells(of: agent.id).firstIndex(of: t) ?? 0) + 1
        return "\(agent.title) · shell \(n)"
    }

    /// Where it works: the Workspace and place of a Task, else what it is.
    private func detail(_ t: MuxaV2.Terminal) -> String? {
        if t.isTaskShell { return agent(of: t)?.pathText }
        if t.isTaskAgent { return t.pathText.isEmpty ? nil : t.pathText }
        return t.routineName.map { "Routine \($0)" } ?? "Other terminal"
    }

    private func agent(of shell: MuxaV2.Terminal) -> MuxaV2.Terminal? {
        model.liveTasks.first { $0.taskDir == shell.taskDir }
    }
}
