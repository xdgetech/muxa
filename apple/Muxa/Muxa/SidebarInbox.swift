import MuxaProtocol
import MuxaSession
import SwiftUI

/// Inbox mode (ADR 0033, ADR 0035): the host's full list of items in the host's order,
/// like a mail list, with no fold. A row shows its item in the detail; a phone pushes it.
struct SidebarInbox: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let items = model.session.openItems
        if !model.usesItems {
            // A row, not a full-screen notice: the list keeps its place when the mode changes.
            Section {
                Text("This host needs a newer muxa-bridge for the Inbox. Update muxa on the host.")
                    .foregroundStyle(.secondary)
                Button("Tasks") { model.setMode(.tasks) }.accessibilityIdentifier("sidebar-inbox-tasks")
                Button("Sessions") { model.setMode(.sessions) }.accessibilityIdentifier("sidebar-inbox-sessions")
            } header: {
                Text("Update muxa-bridge")
            }
        } else {
            if let error = model.session.workError {
                Label(error, systemImage: Icon.warning).foregroundStyle(Theme.failure)
            }
            ForEach(items) { item in
                Button { model.select(.item(item.id)) } label: { InboxItemRow(item: item) }
                    .foregroundStyle(.primary)
                    .sidebarHighlight(model.highlights(item))
                    .accessibilityIdentifier("item-row-\(item.id)")
                    .swipeActions(edge: .trailing) {
                        if item.answers.contains(.later) {
                            Button("Later", systemImage: Icon.later) { model.later(item) }.tint(.indigo)
                        }
                    }
            }
            if items.isEmpty {
                Text("Nothing needs you").foregroundStyle(.secondary)
                    .accessibilityIdentifier("inbox-empty")
            }
        }
    }
}

/// Return opens the selected item on a phone, where the list is alone.
struct ReturnOpensItem: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if model.phoneLayout, let item = model.shownItem ?? model.session.openItems.first {
            Button("Open Item") { model.select(.item(item.id)) }
                .keyboardShortcut(.return, modifiers: [])
                .hidden()
        }
    }
}

/// One row: the question, then the source and the age. The dot marks unread.
struct InboxItemRow: View {
    let item: InboxItem

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Circle()
                .fill(item.isUnread ? Color.accentColor : .clear)
                .frame(width: 8, height: 8)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.question).font(item.isUnread ? .headline : .body).lineLimit(2)
                Text(item.source()).font(.caption).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityValue(item.isUnread ? "Unread" : "")
    }
}

/// The item in the detail column, with its keys: Cmd+Up/Down move between items,
/// Cmd+O opens its terminal, Cmd+Shift+L moves it down.
struct ItemDetail: View {
    @Environment(AppModel.self) private var model
    let id: String

    var body: some View {
        let before = model.session.openItems
        InboxItemScreen(id: id) { saved in model.itemAnswered(saved, before: before) }
            .id(id)
            .background {
                Group {
                    Button("Previous Item") { model.stepItem(-1) }.keyboardShortcut(.upArrow)
                    Button("Next Item") { model.stepItem(1) }.keyboardShortcut(.downArrow)
                    if let item = model.item(id) {
                        Button("Open the Item's Terminal") { model.open(item, as: .terminal) }
                            .keyboardShortcut("o")
                            .disabled(model.itemSession(item) == nil)
                        Button("Later") { model.later(item) }
                            .keyboardShortcut("l", modifiers: [.command, .shift])
                            .disabled(!item.answers.contains(.later))
                    }
                }
                .hidden()
            }
    }
}

/// The detail of Inbox mode with nothing selected.
struct InboxEmptyDetail: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if !model.usesItems {
            InboxNeedsNewerBridge(showAllTasks: { model.setMode(.tasks) }, showSessions: { model.setMode(.sessions) })
        } else if model.session.openItems.isEmpty {
            ContentUnavailableView {
                Label("Nothing needs you", systemImage: "checkmark.circle")
            } description: {
                Text("Tasks keeps every Task, running and closed. Sessions has your tmux sessions.")
            } actions: {
                Button("Tasks") { model.showMode(.tasks) }.accessibilityIdentifier("inbox-show-tasks")
                Button("Sessions") { model.showMode(.sessions) }.accessibilityIdentifier("inbox-show-sessions")
            }
        } else {
            ContentUnavailableView("Pick an Item", systemImage: "tray",
                                   description: Text("The item shows here, with its evidence and its answers."))
        }
    }
}

/// A host below `work` level 3 has no Inbox items, and the app has no other Inbox
/// (ADR 0033 §5). Tasks and Sessions still work there.
struct InboxNeedsNewerBridge: View {
    var showAllTasks: () -> Void
    var showSessions: () -> Void

    var body: some View {
        ContentUnavailableView {
            Label("Update muxa-bridge", systemImage: "arrow.down.circle")
        } description: {
            Text("This host needs a newer muxa-bridge for the Inbox. Update muxa on the host.")
        } actions: {
            Button("Tasks", action: showAllTasks).accessibilityIdentifier("inbox-all-tasks")
            Button("Sessions", action: showSessions).accessibilityIdentifier("inbox-sessions")
        }
    }
}
