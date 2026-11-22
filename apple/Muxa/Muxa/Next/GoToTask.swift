import MuxaV2
import SwiftUI

/// `⌘P`: type part of a Task's branch or a terminal's name, `⏎` opens the
/// first match in Focus. Tasks that need the user come first.
struct GoToTask: View {
    @Environment(NextModel.self) private var model
    @Environment(NextNavigation.self) private var nav
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @FocusState private var focused: Bool

    private var matches: [MuxaV2.Terminal] {
        let live = model.needsYou + model.terminals.filter { $0.live && !model.needsYou.contains($0) }
        let query = text.trimmingCharacters(in: .whitespaces).lowercased()
        guard !query.isEmpty else { return live }
        return live.filter { title($0).lowercased().contains(query) }
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
                        Text(title(t))
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

    private func title(_ t: MuxaV2.Terminal) -> String { t.branch ?? t.name ?? t.id }

    private func pick(_ t: MuxaV2.Terminal) {
        nav.open(t)
        dismiss()
    }
}
