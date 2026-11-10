import MuxaProtocol
import MuxaSession
import SwiftUI

/// `+` on a host with `work` level 9 (ADR 0048): save a thought on the host, with no
/// Task and no agent. Ask Assistant also makes a Work item and asks the workspace's
/// assistant. "Start a Task instead…" opens the New Task form with the text.
struct CaptureSheet: View {
    @Environment(AppModel.self) private var model
    @State var draft: CaptureDraft
    @State private var saving = false
    @State private var error: String?
    /// The host saved the Capture but refused its assistant: only Done is left.
    @State private var kept = false
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
                if !model.session.pickableWorkspaces.isEmpty {
                    Section {
                        Picker("Workspace", selection: $draft.workspace) {
                            Text("None").tag(String?.none)
                            // The overall assistant's workspace takes no Captures (ADR 0049 §2).
                            ForEach(model.session.pickableWorkspaces) { w in
                                Text(w.name).tag(String?.some(w.name))
                            }
                        }
                        .accessibilityIdentifier("capture-workspace")
                    }
                }
                if model.canCreateTasks && !kept {
                    Section {
                        Button("Start a Task instead…", systemImage: Icon.newTask) {
                            Task { await model.startTaskInstead(of: draft) }
                        }
                        .disabled(saving)
                        .accessibilityIdentifier("capture-start-task")
                    }
                }
                // Below "Start a Task instead…", so a phone's keyboard leaves both in view.
                if model.isConnected && !kept {
                    assistant
                }
                if let message = error ?? visibleProblem {
                    Section {
                        Label(message, systemImage: Icon.warning)
                            .foregroundStyle(Theme.failure)
                            .accessibilityIdentifier("capture-error")
                    } footer: {
                        if kept { Text("Your capture is saved in the Inbox under Unsorted.") }
                    }
                }
            }
            .navigationTitle("Capture")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    if !kept {
                        Button("Cancel") { model.capture = nil }
                            .keyboardShortcut(.cancelAction)
                            .disabled(saving)
                    }
                }
                ToolbarItem(placement: .primaryAction) {
                    if model.isConnected && !kept && !saving {
                        Button("Ask") { save(assist: draft.assist) }
                            .disabled(draft.assistProblem != nil)
                            .accessibilityLabel("Ask Assistant")
                            .accessibilityIdentifier("capture-ask")
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if kept {
                        Button("Done") { model.capture = nil }
                            .accessibilityIdentifier("capture-done")
                    } else if saving {
                        ProgressView()
                    } else {
                        Button("Save") { save() }
                            .disabled(draft.problem != nil)
                            .keyboardShortcut(.return, modifiers: .command)
                            .accessibilityIdentifier("capture-save")
                    }
                }
            }
            .interactiveDismissDisabled(saving || (!draft.trimmed.isEmpty && !kept))
            .onAppear { textFocused = true }
            .onChange(of: draft) { if !kept { error = nil } }
        }
    }

    /// Only a text over the limit; an empty one just keeps Save disabled.
    private var visibleProblem: String? {
        draft.trimmed.isEmpty ? nil : draft.problem
    }

    /// What Ask asks for: the scope and an optional purpose. The host needs a workspace.
    private var assistant: some View {
        Section {
            Picker("Scope", selection: $draft.scope) {
                ForEach(AssistScope.choices, id: \.self) { Text($0.shown.capitalized).tag($0) }
            }
            .pickerStyle(.segmented)
            .accessibilityIdentifier("capture-scope")
            TextField("What should it look at? (optional)", text: $draft.purpose, axis: .vertical)
                .lineLimit(1...4)
                .accessibilityIdentifier("capture-purpose")
        } header: {
            Label("Assistant", systemImage: Icon.assistant)
        } footer: {
            Text(draft.workspace == nil
                 ? "Ask needs a workspace: its assistant works on it on the host."
                 : "Ask makes a Work item and asks the workspace's assistant. It runs on the host.")
        }
    }

    private func save(assist: AssistSpec? = nil) {
        guard draft.problem == nil, !saving else { return }
        saving = true
        Task {
            defer { saving = false }
            do {
                try await model.save(draft, assist: assist)
            } catch let refused as PaneSession.AssistNotStarted {
                kept = true
                self.error = refused.reason.message
            } catch {
                self.error = PaneSession.describe(error)
            }
        }
    }
}
