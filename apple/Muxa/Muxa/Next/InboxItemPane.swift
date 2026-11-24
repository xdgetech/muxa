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
                    .foregroundStyle(Palette.text)
                    .accessibilityIdentifier("detail.title")
                if let note = item.note {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Summary").font(.system(size: 13)).foregroundStyle(Palette.textSecondary)
                        MarkdownText(text: note).accessibilityIdentifier("detail.note")
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .card(radius: 12, border: true)
                }
                ItemSources(item: item)
                ItemAssist(item: item, live: position != nil, onDone: onDone)
                actions
                if position != nil { ItemTaskActions(item: item, onDone: onDone) }
                if let error = model.requestError {
                    Text(error).font(.callout).foregroundStyle(Palette.late)
                        .accessibilityIdentifier("detail.error")
                }
            }
            .padding()
            .frame(maxWidth: 760, alignment: .leading)
        }
        .background(Palette.ground)
        .navigationTitle(item.kind.title)
        .inlineTitle()
        .dialog("Remove the worktree anyway?", isPresented: losingWork, message: wouldLoseWork,
                actions: [DialogAction("Remove Anyway", role: .destructive, id: "detail.force") {
                    Task { await approve(force: true) }
                }])
    }

    private var losingWork: Binding<Bool> {
        Binding(get: { wouldLoseWork != nil }, set: { if !$0 { wouldLoseWork = nil } })
    }

    private var header: some View {
        HStack(spacing: 10) {
            KindChip(kind: item.kind)
            if let position { Text("#\(position)") }
            if let bot = item.bot { BotChip(bot: bot) }
            Text(item.origin).lineLimit(1)
            if let due = item.due { DueLabel(due: due) }
        }
        .font(.system(size: 12.5))
        .foregroundStyle(Palette.textSecondary)
    }

    @ViewBuilder
    private var actions: some View {
        if position == nil {
            Button("Reopen", systemImage: "arrow.uturn.backward") { Task { await model.inbox(.inboxReopen(item: item.id)) } }
                .buttonStyle(QuietButtonStyle())
        } else {
            HStack {
                Button(item.closesTask ? "Approve" : "Done") {
                    Task {
                        if item.closesTask { await approve(force: false) } else { await done() }
                    }
                }
                .buttonStyle(FilledButtonStyle())
                .keyboardShortcut(.return, modifiers: .command)
                .accessibilityIdentifier("detail.main")
                later
                if item.closesTask {
                    Button("Done", systemImage: "checkmark") { Task { await done() } }
                        .buttonStyle(QuietButtonStyle())
                        .accessibilityIdentifier("detail.done")
                }
                Spacer()
                Menu {
                    Button("Delete", role: .destructive) { Task { await act(.inboxDelete(item: item.id)) } }
                } label: {
                    Image(systemName: "ellipsis").accessibilityLabel("More")
                }
                .buttonStyle(RaisedIconStyle(round: true))
            }
            .background { hiddenKeys }
        }
    }

    /// Moves the item to the bottom of the Inbox (ADR 0096).
    private var later: some View {
        Button("Later", systemImage: "text.line.last.and.arrowtriangle.forward") {
            Task { await act(.inboxLater(item: item.id)) }
        }
        .buttonStyle(QuietButtonStyle())
        .accessibilityIdentifier("detail.later")
    }

    /// `⌘L`: Later. `⌘⌫`: Done.
    private var hiddenKeys: some View {
        Group {
            Button("") { Task { await act(.inboxLater(item: item.id)) } }
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
