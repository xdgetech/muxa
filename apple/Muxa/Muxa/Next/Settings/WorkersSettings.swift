import MuxaV2
import SwiftUI

/// The host's workers (`[[worker]]`): agents a Routine can run as.
struct WorkersSettings: View {
    @Environment(NextModel.self) private var model
    @State private var editing: WorkerDraft?

    var body: some View {
        SettingsPage {
            Section {
                ForEach(model.settings.settings?.workers ?? []) { w in
                    Button { editing = WorkerDraft(w) } label: {
                        VStack(alignment: .leading, spacing: 3) {
                            HStack {
                                Text(w.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                                Spacer()
                                StateTag(text: w.agent)
                            }
                            if let d = w.description { Text(d).font(.caption).foregroundStyle(Palette.textSecondary) }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("worker.\(w.name)")
                }
                Button("Add Worker", systemImage: "plus") { editing = WorkerDraft() }
                    .accessibilityIdentifier("worker.add")
            } footer: {
                Text("A Routine names a worker to run as it. Without one, the Workspace's agent runs it.")
            }
        }
        .navigationTitle("Workers")
        .inlineTitle()
        .sheet(item: $editing) { WorkerEditor(draft: $0) }
    }
}

struct WorkerDraft: Identifiable {
    var existing: String?
    var name = ""
    var description = ""
    var agent = "claude-code"
    var args = ""
    var instructions = ""
    var workspaces = ""
    var check = ""
    var attempts = ""
    var id: String { existing ?? "+" }

    init() {}

    init(_ w: WorkerSpec) {
        existing = w.name
        name = w.name
        description = w.description ?? ""
        agent = w.agent
        args = Lines.join(w.args)
        instructions = w.instructions ?? ""
        workspaces = (w.workspaces ?? []).joined(separator: ", ")
        check = w.loop?.check ?? ""
        attempts = w.loop?.maxAttempts.map(String.init) ?? ""
    }

    func spec(keeping old: WorkerSpec?) -> WorkerSpec? {
        guard let name = Lines.optional(name) else { return nil }
        let list = workspaces.split(separator: ",").map { Lines.trimmed(String($0)) }.filter { !$0.isEmpty }
        let loop = Lines.optional(check).map {
            WorkerLoop(check: $0, maxAttempts: UInt32(Lines.trimmed(attempts)), timeout: old?.loop?.timeout)
        }
        return WorkerSpec(name: name, description: Lines.optional(description), agent: agent, args: Lines.split(args),
                          instructions: Lines.optional(instructions), workspaces: list.isEmpty ? nil : list, loop: loop)
    }
}

private struct WorkerEditor: View {
    @Environment(NextModel.self) private var model
    @State private var draft: WorkerDraft

    init(draft: WorkerDraft) {
        _draft = State(initialValue: draft)
    }

    var body: some View {
        SettingsEditor(
            title: draft.existing ?? "New Worker", ops: ops,
            remove: draft.existing.map { (.workerRemove(name: $0), "Routines that name it stop loading until they name another.") }
        ) {
            Section {
                if draft.existing == nil {
                    TextField("Name, e.g. reviewer", text: $draft.name)
                        .plainEntry()
                        .accessibilityIdentifier("worker.name")
                }
                TextField("Description", text: $draft.description)
                    .accessibilityIdentifier("worker.description")
                Picker("Agent", selection: $draft.agent) {
                    Text("Claude Code").tag("claude-code")
                    Text("Codex").tag("codex")
                }
                .accessibilityIdentifier("worker.agent")
                TextField("Arguments, one per line", text: $draft.args, axis: .vertical)
                    .lineLimit(1...6)
                    .monospaced()
                    .plainEntry()
                    .accessibilityIdentifier("worker.args")
                TextField("Instructions file (optional)", text: $draft.instructions)
                    .plainEntry()
                    .accessibilityIdentifier("worker.instructions")
                TextField("Workspaces, comma separated (empty: all)", text: $draft.workspaces)
                    .plainEntry()
                    .accessibilityIdentifier("worker.workspaces")
            }
            Section {
                TextField("Check (optional), e.g. make test", text: $draft.check)
                    .monospaced()
                    .plainEntry()
                    .accessibilityIdentifier("worker.check")
                TextField("Stop after this many failed checks", text: $draft.attempts)
                    .numberEntry()
                    .accessibilityIdentifier("worker.attempts")
            } header: {
                Text("Loop")
            }
        }
    }

    private func ops() -> [SettingsOp]? {
        let old = model.settings.settings?.workers.first { $0.name == draft.existing }
        guard let spec = draft.spec(keeping: old) else { return nil }
        return draft.existing == nil ? [.workerAdd(worker: spec)] : [.workerUpdate(worker: spec)]
    }
}
