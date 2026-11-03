import MuxaProtocol
import MuxaSession
import SwiftUI

/// A Routine's briefing or proposal: what it says, its sources, and its actions.
/// Source text is shown as data; it never acts by itself (ADR 0026 section 4).
struct InboxEntryDetail: View {
    @Environment(AppModel.self) private var model
    let id: String
    @Binding var path: [InboxRoute]
    @State private var prompt: InboxPrompt?
    @State private var failure: String?

    var body: some View {
        if let entry = model.entry(id) {
            form(entry)
        } else {
            ContentUnavailableView("Entry Not Found", systemImage: Icon.briefing,
                                   description: Text("The host no longer lists this entry."))
        }
    }

    private func form(_ entry: InboxEntry) -> some View {
        Form {
            Section {
                Text(entry.body).textSelection(.enabled).accessibilityIdentifier("entry-body")
                ForEach(entry.links ?? [], id: \.url) { link in
                    if let url = URL(string: link.url) {
                        Link(link.title ?? link.url, destination: url)
                    }
                }
            } header: {
                Text(entry.kindTitle)
            }
            if let p = entry.proposal {
                Section("Proposed action") {
                    LabeledContent("Action", value: p.action)
                    LabeledContent("Target", value: p.target)
                    Text(p.content).font(.callout.monospaced()).textSelection(.enabled)
                }
            }
            actions(entry)
            Section("From") {
                LabeledContent("Routine", value: "\(entry.workspace)/\(entry.routine)")
                LabeledContent("Published", value: entry.createdAt.shownDate)
                LabeledContent("Occurrence", value: entry.occurrence)
            }
            .font(.callout)
        }
        .navigationTitle(entry.title)
        .navigationBarTitleDisplayMode(.inline)
        .inboxActions($prompt) { task in path = [.task(task)] }
        .alert("Can't Update the Inbox", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
    }

    @ViewBuilder private func actions(_ entry: InboxEntry) -> some View {
        if let task = entry.task {
            Section {
                NavigationLink("Open the Task", value: InboxRoute.task(task))
                    .accessibilityIdentifier("entry-open-task")
            } footer: {
                Text("Approved. The Task performs the action.")
            }
        } else if entry.hasKnownActions {
            Section {
                if entry.isProposal {
                    Button("Approve…", systemImage: "checkmark.seal") { prompt = .approve(entry) }
                        .accessibilityIdentifier("entry-approve")
                    Button("Dismiss", systemImage: "xmark") { dismiss(entry) }
                        .accessibilityIdentifier("entry-dismiss")
                } else {
                    Button("Done", systemImage: "checkmark") { dismiss(entry) }
                        .accessibilityIdentifier("entry-done")
                }
                Button("Later…", systemImage: Icon.later) { prompt = .later(.entry(entry)) }
                    .accessibilityIdentifier("entry-later")
            }
        }
    }

    /// Done for a briefing, Dismiss for a proposal; back to the list after.
    private func dismiss(_ entry: InboxEntry) {
        Task {
            do {
                try await model.dismiss(entry)
                if path.last == .entry(entry.id) { path.removeLast() }
            } catch {
                failure = PaneSession.describe(error)
            }
        }
    }
}
