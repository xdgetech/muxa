import MuxaProtocol
import MuxaSession
import SwiftUI

/// `+` on a host with `work` level 8 (ADR 0046): save a thought on the host, with no
/// Task and no agent. "Start a Task instead…" opens the New Task form with the text.
struct CaptureSheet: View {
    @Environment(AppModel.self) private var model
    @State var draft: CaptureDraft
    @State private var saving = false
    @State private var error: String?
    @FocusState private var textFocused: Bool

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("What is on your mind?", text: $draft.text, axis: .vertical)
                        .lineLimit(4...12)
                        .focused($textFocused)
                        .accessibilityIdentifier("capture-text")
                } footer: {
                    Text(model.isConnected
                         ? "Saved on the host, in the Inbox under Unsorted. Nothing starts."
                         : "Not connected. It stays on this device and is sent when you connect.")
                }
                if !model.session.workspaces.isEmpty {
                    Section {
                        Picker("Workspace", selection: $draft.workspace) {
                            Text("None").tag(String?.none)
                            ForEach(model.session.workspaces) { w in
                                Text(w.name).tag(String?.some(w.name))
                            }
                        }
                        .accessibilityIdentifier("capture-workspace")
                    }
                }
                if model.canCreateTasks {
                    Section {
                        Button("Start a Task instead…", systemImage: Icon.newTask) {
                            Task { await model.startTaskInstead(of: draft) }
                        }
                        .disabled(saving)
                        .accessibilityIdentifier("capture-start-task")
                    }
                }
                if let message = error ?? visibleProblem {
                    Section {
                        Label(message, systemImage: Icon.warning)
                            .foregroundStyle(Theme.failure)
                            .accessibilityIdentifier("capture-error")
                    }
                }
            }
            .navigationTitle("Capture")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { model.capture = nil }
                        .keyboardShortcut(.cancelAction)
                        .disabled(saving)
                }
                ToolbarItem(placement: .confirmationAction) {
                    if saving {
                        ProgressView()
                    } else {
                        Button("Save", action: save)
                            .disabled(draft.problem != nil)
                            .keyboardShortcut(.return, modifiers: .command)
                            .accessibilityIdentifier("capture-save")
                    }
                }
            }
            .interactiveDismissDisabled(saving || !draft.trimmed.isEmpty)
            .onAppear { textFocused = true }
            .onChange(of: draft) { error = nil }
        }
    }

    /// Only a text over the limit; an empty one just keeps Save disabled.
    private var visibleProblem: String? {
        draft.trimmed.isEmpty ? nil : draft.problem
    }

    private func save() {
        guard draft.problem == nil, !saving else { return }
        saving = true
        Task {
            defer { saving = false }
            do {
                try await model.save(draft)
            } catch {
                self.error = PaneSession.describe(error)
            }
        }
    }
}
