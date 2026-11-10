import MuxaProtocol
import MuxaSession
import SwiftUI

/// A `write` item's write (ADR 0052 §2): the action, the target and the content whole, as
/// plain text in a fixed-width font. `Text(verbatim:)` renders no Markdown and opens no
/// link, so the text the user approves is the text the user saw.
struct WriteProposalView: View {
    @Environment(AppModel.self) private var model
    let item: InboxItem
    let write: WriteProposal

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            LabeledContent("Action") { Text(verbatim: write.action).textSelection(.enabled) }
                .accessibilityIdentifier("item-write-action")
            LabeledContent("Target") { Text(verbatim: write.target).font(.body.monospaced()).textSelection(.enabled) }
                .accessibilityIdentifier("item-write-target")
            if let worker = write.worker {
                LabeledContent("Proposed By", value: worker)
            }
            Text("Content").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            Text(verbatim: write.content)
                .font(.callout.monospaced())
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .background(.background, in: RoundedRectangle(cornerRadius: 8))
                .accessibilityIdentifier("item-write-content")
            if let reason = item.summary, !reason.isEmpty {
                Text(verbatim: reason).font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                    .accessibilityIdentifier("item-write-reason")
            }
            if model.isOlderWrite(write) {
                Label("From an older result (version \(write.version))", systemImage: Icon.warning)
                    .font(.callout).foregroundStyle(Theme.attention)
                    .accessibilityIdentifier("item-write-older")
            }
            if let task = item.task {
                Button("Task started", systemImage: Icon.newTask) { model.select(.task(task)) }
                    .accessibilityIdentifier("item-write-task")
            } else if model.showsWrite(item) {
                Text("Approve starts one Task of the workspace's writer with this content, unchanged. "
                     + "muxa does not check what the writer does; its Task reports it.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

/// The writes a request's Results propose, each with its state (ADR 0052 §7): proposed,
/// waiting in the Inbox, rejected, or the writer's Task, which opens with its state.
struct RequestWritesSection: View {
    @Environment(AppModel.self) private var model
    let request: AssistantRequest

    /// Newest result first; a result names its step's member when it has one.
    private var writes: [(key: String, result: AssistResult, write: ResultWrite)] {
        request.results.sorted { $0.at > $1.at }.flatMap { result in
            result.writes.enumerated().map { ("\(result.version)-\(result.step ?? 1)-\($0.offset)", result, $0.element) }
        }
    }

    var body: some View {
        let writes = writes
        if !writes.isEmpty {
            Section {
                ForEach(writes, id: \.key) { entry in
                    row(entry.result, entry.write)
                }
            } header: {
                Label("Writes", systemImage: Icon.write)
            } footer: {
                Text("Each write waits for your approval in the Inbox. The writer performs an approved one in a Task.")
            }
        }
    }

    @ViewBuilder
    private func row(_ result: AssistResult, _ write: ResultWrite) -> some View {
        let status = model.session.writeStatus(write, of: request)
        VStack(alignment: .leading, spacing: 4) {
            Text(verbatim: write.action).font(.body.weight(.semibold))
            Text(verbatim: write.target).font(.caption.monospaced()).foregroundStyle(.secondary)
            if let worker = result.worker {
                Text("From \(worker)").font(.caption).foregroundStyle(.secondary)
            }
            if result.version < request.version {
                Text("From an older result (version \(result.version))").font(.caption).foregroundStyle(Theme.attention)
            }
            switch status {
            case .waiting(let item):
                Button(model.shown(status)) { model.select(.item(item)) }
                    .accessibilityIdentifier("work-write-item-\(item)")
            case .taskStarted(let task):
                Button(model.shown(status)) { model.select(.task(task)) }
                    .accessibilityIdentifier("work-write-task-\(task)")
            default:
                Text(model.shown(status)).font(.callout).foregroundStyle(.secondary)
                    .accessibilityIdentifier("work-write-status")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("work-write")
    }
}

/// The workspace's writer under its staff (ADR 0052 §7): marked "Writes", with its
/// description, and whether the workspace takes writes. It is never asked directly.
struct WriterSection: View {
    @Environment(AppModel.self) private var model
    let workspace: String?

    var body: some View {
        if let writer = model.session.writer(of: workspace) {
            Section {
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        Text(writer.worker)
                        Spacer(minLength: 8)
                        Label("Writes", systemImage: Icon.write).font(.caption).foregroundStyle(Theme.attention)
                    }
                    if !writer.description.isEmpty {
                        Text(writer.description).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                    }
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("staff-writer")
            } header: {
                Text("Writer")
            } footer: {
                Text(footer).accessibilityIdentifier("staff-writes")
            }
        }
    }

    private var footer: String {
        switch model.session.writesMode(of: workspace) {
        case .review?: "Staff members only propose writes. You approve each one in the Inbox, then the writer performs it."
        case Writes.none?: "This workspace takes no writes. Staff members suggest changes in a Result's Next."
        case .other(let raw)?: "Writes: \(raw)"
        case nil: ""
        }
    }
}
