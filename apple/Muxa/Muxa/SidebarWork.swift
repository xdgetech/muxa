import MuxaProtocol
import MuxaSession
import SwiftUI

/// The Inbox's Work section (ADR 0046): open Work items in the host's order, the ones
/// moved down with Later last (ADR 0040). A swipe right completes, left is Later.
struct SidebarWorkSection: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let items = model.session.openWork
        if !items.isEmpty {
            Section {
                ForEach(items) { item in
                    Button { model.select(.work(item.id)) } label: {
                        WorkRow(item: item, request: model.latestRequest(of: item), resultReady: model.hasNewResult(item))
                    }
                        .foregroundStyle(.primary)
                        .sidebarHighlight(model.detail == .work(item.id))
                        .accessibilityIdentifier("work-row-\(item.id)")
                        .swipeActions(edge: .leading) {
                            Button("Complete", systemImage: Icon.done) { model.change(item, .complete) }.tint(Theme.done)
                        }
                        .swipeActions(edge: .trailing) {
                            Button("Later", systemImage: Icon.later) { model.change(item, .later) }.tint(.gray)
                        }
                }
            } header: {
                Text("Work").accessibilityIdentifier("inbox-section-work")
            }
        }
    }
}

/// The title, then its state, workspace and Later, and its assistant's delivery or a
/// Result not read yet. A Work item that waits on a decision has no answer of its own:
/// the decision is in Decisions.
struct WorkRow: View {
    let item: WorkItem
    /// The newest assistant request, if any.
    var request: AssistantRequest?
    /// The newest Result is not shown on this device yet.
    var resultReady = false

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(item.title).lineLimit(2)
            Text(details).font(.caption)
                .foregroundStyle(item.state == .waitingDecision ? Theme.attention : .secondary)
            if resultReady {
                Label("Result ready", systemImage: Icon.result).font(.caption.weight(.semibold))
                    .foregroundStyle(Theme.accent)
            } else if let request, request.delivery != .resulted {
                Text("Assistant: \(request.delivery.shown)").font(.caption)
                    .foregroundStyle(request.delivery == .failed || request.delivery == .timedOut ? Theme.attention : .secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    private var details: String {
        [item.state.shown, item.workspace, item.laterAt == nil ? nil : "later"].compactMap { $0 }.joined(separator: " · ")
    }
}

/// The Inbox's Unsorted section: Captures not sorted yet, newest first, and the ones
/// still on this device. Not counted in the badge.
struct SidebarUnsortedSection: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let captures = model.session.captures
        let unsent = model.unsentHere
        if !captures.isEmpty || !unsent.isEmpty {
            Section {
                UnsentRows()
                ForEach(captures) { c in
                    Button { model.select(.capture(c.id)) } label: { CaptureRow(capture: c) }
                        .foregroundStyle(.primary)
                        .sidebarHighlight(model.detail == .capture(c.id))
                        .accessibilityIdentifier("capture-row-\(c.id)")
                        .swipeActions(edge: .leading) {
                            Button("Work Item", systemImage: Icon.work) { model.sort(c, .work) }.tint(.accentColor)
                        }
                        .swipeActions(edge: .trailing) {
                            Button("Note", systemImage: Icon.note) { model.sort(c, .note) }.tint(.gray)
                        }
                }
            } header: {
                Text("Unsorted").accessibilityIdentifier("inbox-section-unsorted")
            }
        }
    }
}

/// Captures that wait on this device: sent again on connect, or now.
struct UnsentRows: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ForEach(model.unsentHere) { c in
            VStack(alignment: .leading, spacing: 2) {
                Text(CaptureDraft.firstLine(c.text, limit: 200)).lineLimit(2)
                Label(c.problem ?? "Not sent", systemImage: Icon.warning)
                    .font(.caption)
                    .foregroundStyle(c.problem == nil ? .secondary : Theme.failure)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("unsent-row-\(c.clientId)")
            .contextMenu {
                if model.isConnected {
                    Button("Send Now", systemImage: Icon.send) { Task { await model.resendUnsent() } }
                }
                Button("Delete", systemImage: Icon.delete, role: .destructive) { model.unsent.remove(c.clientId) }
            }
            .swipeActions(edge: .trailing) {
                Button("Delete", systemImage: Icon.delete, role: .destructive) { model.unsent.remove(c.clientId) }
            }
        }
    }
}

/// The first line of the text, then its age and workspace.
struct CaptureRow: View {
    let capture: Capture

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(CaptureDraft.firstLine(capture.text, limit: 200)).lineLimit(2)
            Text([capture.createdAt.shownDate, capture.workspace].compactMap { $0 }.joined(separator: " · "))
                .font(.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}
