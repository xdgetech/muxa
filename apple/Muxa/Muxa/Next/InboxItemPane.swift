import MuxaV2
import SwiftUI

/// The wide pane: what the item asks, and its actions with their keys.
struct InboxItemPane: View {
    @Environment(NextModel.self) private var model
    let item: InboxItem
    /// Where the item stands in the user's order, from 1; nil when done.
    let position: Int?
    var onDone: () -> Void = {}
    /// The server's reason not to remove a worktree, until the user decides.
    @State private var wouldLoseWork: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                Text(item.title).font(.title2.weight(.semibold))
                    .accessibilityIdentifier("detail.title")
                if let note = item.note {
                    GroupBox("Summary") {
                        Text(note).frame(maxWidth: .infinity, alignment: .leading).textSelection(.enabled)
                    }
                }
                actions
                if position != nil { ItemTaskActions(item: item, onDone: onDone) }
                if let error = model.requestError {
                    Text(error).font(.callout).foregroundStyle(.red)
                        .accessibilityIdentifier("detail.error")
                }
            }
            .padding()
            .frame(maxWidth: 760, alignment: .leading)
        }
        .navigationTitle(item.kind.title)
        .inlineTitle()
        .alert("Remove the worktree anyway?", isPresented: losingWork, presenting: wouldLoseWork) { _ in
            Button("Remove Anyway", role: .destructive) { Task { await approve(force: true) } }
                .accessibilityIdentifier("detail.force")
            Button("Cancel", role: .cancel) {}
        } message: { message in
            Text(message)
        }
    }

    private var losingWork: Binding<Bool> {
        Binding(get: { wouldLoseWork != nil }, set: { if !$0 { wouldLoseWork = nil } })
    }

    private var header: some View {
        HStack(spacing: 12) {
            KindLabel(kind: item.kind)
            if let position { Text("#\(position)").font(.caption).foregroundStyle(.secondary) }
            Text(source).font(.caption).foregroundStyle(.secondary)
            if let w = item.workspace { Text(w).font(.caption).foregroundStyle(.secondary) }
            if let due = item.due { DueLabel(due: due) }
        }
    }

    private var source: String {
        switch item.source {
        case "task": "From a Task"
        case "routine": "From a Routine"
        default: "Captured"
        }
    }

    @ViewBuilder
    private var actions: some View {
        if position == nil {
            Button("Reopen") { Task { await model.inbox(.inboxReopen(item: item.id)) } }
                .buttonStyle(.bordered)
        } else {
            HStack {
                Button(item.closesTask ? "Approve" : "Done") {
                    Task {
                        if item.closesTask { await approve(force: false) } else { await done() }
                    }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.return, modifiers: .command)
                .accessibilityIdentifier("detail.main")
                laterMenu
                if item.closesTask {
                    Button("Done") { Task { await done() } }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("detail.done")
                }
                Spacer()
                Menu {
                    Button("Delete", role: .destructive) { Task { await act(.inboxDelete(item: item.id)) } }
                } label: {
                    Image(systemName: "ellipsis.circle").accessibilityLabel("More")
                }
            }
            .background { hiddenKeys }
        }
    }

    private var laterMenu: some View {
        Menu("Later") {
            ForEach(LaterChoice.allCases) { choice in
                Button(choice.title) { Task { await act(.inboxLater(item: item.id, until: choice.until())) } }
            }
        }
        .menuStyle(.button)
        .buttonStyle(.bordered)
        .accessibilityIdentifier("detail.later")
    }

    /// `⌘L`: Later until tomorrow morning, the usual choice. `⌘⌫`: Done.
    private var hiddenKeys: some View {
        Group {
            Button("") { Task { await act(.inboxLater(item: item.id, until: LaterChoice.tomorrow.until())) } }
                .keyboardShortcut("l", modifiers: .command)
            Button("") { Task { await done() } }
                .keyboardShortcut(.delete, modifiers: .command)
        }
        .opacity(0)
        .accessibilityHidden(true)
    }

    private func done() async { await act(.inboxDone(item: item.id)) }

    private func act(_ r: V2Request) async {
        await model.inbox(r)
        if model.requestError == nil { onDone() }
    }

    private func approve(force: Bool) async {
        if let message = await model.approve(item.id, force: force) {
            wouldLoseWork = message
        } else if model.requestError == nil {
            onDone()
        }
    }
}
