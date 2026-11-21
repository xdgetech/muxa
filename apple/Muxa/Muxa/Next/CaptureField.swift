import MuxaV2
import SwiftUI

/// The field on top of the Inbox (ADR 0066 decision 6). The server reads the
/// line; the kind, Workspace and due date show as chips the user can change
/// before adding.
struct CaptureField: View {
    @Environment(NextModel.self) private var model
    /// After an item was added: the Mac's Capture window closes.
    var onAdded: () -> Void = {}
    /// Takes the keyboard when shown: the Mac's Capture window.
    var focusOnAppear = false
    @State private var text = ""
    @State private var parsed: Capture?
    @State private var parsing: Task<Void, Never>?
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Image(systemName: "plus.circle").foregroundStyle(.secondary)
                TextField("Capture: reply Anna #Personal due fri", text: $text)
                    .focused($focused)
                    .submitLabel(.done)
                    .onSubmit { Task { await add() } }
                    .accessibilityIdentifier("capture.field")
                if !text.isEmpty {
                    Button("Add") { Task { await add() } }
                        .disabled(parsed?.title.isEmpty ?? true)
                        .accessibilityIdentifier("capture.add")
                }
            }
            if let p = parsed, !text.isEmpty { chips(p) }
        }
        .onChange(of: text) { _, new in parse(new) }
        .onAppear { if focusOnAppear { focused = true } }
        .background {
            Button("") { focused = true }
                .keyboardShortcut("n", modifiers: .command)
                .opacity(0)
                .accessibilityHidden(true)
        }
    }

    private func chips(_ p: Capture) -> some View {
        HStack(spacing: 6) {
            Menu {
                ForEach(ItemKind.allCases.filter { $0 != .review && $0 != .failed }, id: \.self) { kind in
                    Button(kind.title) { parsed?.kind = kind }
                }
            } label: {
                KindLabel(kind: p.kind).padding(.horizontal, 8).padding(.vertical, 3)
                    .background(p.kind.tint.opacity(0.12), in: Capsule())
            }
            .accessibilityIdentifier("capture.kind")
            if let w = p.workspace {
                chip("#\(w)") { parsed?.workspace = nil }
            }
            if let d = p.due {
                chip("due \(d)") { parsed?.due = nil }
            }
        }
    }

    private func chip(_ text: String, remove: @escaping () -> Void) -> some View {
        Button(action: remove) {
            HStack(spacing: 4) {
                Text(text)
                Image(systemName: "xmark").font(.caption2)
            }
            .font(.caption)
            .padding(.horizontal, 8).padding(.vertical, 3)
            .background(.quaternary, in: Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(text), remove")
    }

    /// Asks the server once typing pauses.
    private func parse(_ line: String) {
        parsing?.cancel()
        guard !line.trimmingCharacters(in: .whitespaces).isEmpty else {
            parsed = nil
            return
        }
        parsing = Task {
            try? await Task.sleep(for: .milliseconds(250))
            guard !Task.isCancelled else { return }
            let p = await model.parseCapture(line)
            if !Task.isCancelled { parsed = p }
        }
    }

    private func add() async {
        parsing?.cancel()
        let capture: Capture?
        if let parsed { capture = parsed } else { capture = await model.parseCapture(text) }
        guard let capture, !capture.title.isEmpty else { return }
        await model.inbox(.inboxAdd(capture))
        if model.requestError == nil {
            text = ""
            parsed = nil
            onAdded()
        }
    }
}
