import MuxaV2
import SwiftUI

/// `⌘P`: type part of a Task's branch or a terminal's name, `⏎` opens the
/// first match in Focus. Tasks that need the user come first. A Task's shells
/// (ADR 0079) and other terminals show only once the text matches them, as the
/// lists keep them closed; a shell opens its Task with the shell's pane active.
struct GoToTask: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @FocusState private var focused: Bool

    private var matches: [MuxaV2.Terminal] {
        let tasks = model.needsYou + model.liveTasks.filter { !model.needsYou.contains($0) }
        let query = text.trimmingCharacters(in: .whitespaces).lowercased()
        guard !query.isEmpty else { return tasks }
        let shells = tasks.flatMap { model.shells(of: $0.id) }
        return (tasks + shells + model.otherTerminals).filter { label($0).lowercased().contains(query) }
    }

    /// A shell is named after its Task, so "fix shell" finds fix-login's shells.
    private func label(_ t: MuxaV2.Terminal) -> String {
        guard t.isTaskShell, let agent = agent(of: t) else { return t.title }
        let n = (model.shells(of: agent.id).firstIndex(of: t) ?? 0) + 1
        return "\(agent.title) · shell \(n)"
    }

    private func agent(of shell: MuxaV2.Terminal) -> MuxaV2.Terminal? {
        model.liveTasks.first { $0.taskDir == shell.taskDir }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            TextField("Go to Task", text: $text)
                .textFieldStyle(.plain)
                .font(.title3)
                .padding(12)
                .focused($focused)
                .onSubmit { if let first = matches.first { pick(first) } }
                .accessibilityIdentifier("goto.field")
            Divider().overlay(Palette.border)
            List(matches) { t in
                Button { pick(t) } label: {
                    HStack {
                        Text(label(t))
                        Spacer()
                        if let state = t.status?.state {
                            Text(state).font(.caption).foregroundStyle(model.needsYou.contains(t) ? Theme.attention : Palette.textSecondary)
                        }
                    }
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("goto.\(t.id)")
                .listRowBackground(Color.clear)
            }
            .listStyle(.plain)
            .groundList()
        }
        .foregroundStyle(Palette.text)
        .background(Palette.ground)
        .presentationBackground(Palette.ground)
        .frame(minWidth: 420, minHeight: 300)
        .onAppear { focused = true }
        #if os(macOS)
        .onExitCommand { dismiss() }
        #endif
    }

    private func pick(_ t: MuxaV2.Terminal) {
        if t.isTaskShell, let agent = agent(of: t) {
            nav.open(agent)
            nav.sync(shells: model.shells(of: agent.id).map(\.id), focus: t.id)
        } else {
            nav.open(t)
        }
        dismiss()
    }
}
