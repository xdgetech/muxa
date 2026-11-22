import MuxaV2
import SwiftUI

/// `⌘P`: type part of a Task's branch or a terminal's name, `⏎` opens the
/// first match in Focus. Tasks that need the user come first. Other terminals
/// show only once the text matches them, as the lists keep them closed.
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
        return (tasks + model.otherTerminals).filter { $0.title.lowercased().contains(query) }
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
                        Text(t.title)
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
        nav.open(t)
        dismiss()
    }
}
