import SwiftUI

/// One line of an editable list; its id keeps the row while the text changes.
struct EditableLine: Identifiable, Equatable {
    let id = UUID()
    var text: String

    init(_ text: String) { self.text = text }
}

/// A list of single-line entries to edit, remove and add: Bot rules, a
/// Bot's commands. `problem` says why a line cannot be saved.
struct EditableLineList: View {
    var title: String?
    @Binding var lines: [EditableLine]
    let placeholder: String
    let addTitle: String
    /// The identifiers' prefix: `<id>.0`, `<id>.remove.0`, `<id>.add`.
    let id: String
    var problem: (String) -> String? = { _ in nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let title { Text(title).font(.subheadline.weight(.semibold)) }
            if lines.isEmpty {
                Text("none").font(.callout).foregroundStyle(Palette.textTertiary)
            }
            ForEach($lines) { $line in
                let n = lines.firstIndex { $0.id == line.id } ?? 0
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 8) {
                        TextField(placeholder, text: $line.text)
                            .monospaced()
                            .plainEntry()
                            .accessibilityIdentifier("\(id).\(n)")
                        Button("Remove", systemImage: "minus.circle") {
                            lines.removeAll { $0.id == line.id }
                        }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                        .accessibilityIdentifier("\(id).remove.\(n)")
                    }
                    if let why = problem(line.text) {
                        Text(why).font(.caption).foregroundStyle(Theme.failure).accessibilityIdentifier("\(id).problem.\(n)")
                    }
                }
            }
            Button(addTitle, systemImage: "plus") { lines.append(EditableLine("")) }
                .buttonStyle(.borderless)
                .accessibilityIdentifier("\(id).add")
        }
    }
}
