import MuxaProtocol
import MuxaSession
import SwiftUI

/// Cmd+K: every session with its agent's state. Return opens the first match.
struct SessionSwitcher: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    var body: some View {
        NavigationStack {
            List {
                ForEach(matches.groupedByRepo(), id: \.repo) { group in
                    Section {
                        ForEach(group.sessions) { s in
                            Button {
                                open(s)
                            } label: {
                                SessionRow(session: s, current: s.id == model.currentSession?.id)
                            }
                        }
                    } header: {
                        RepoHeader(repo: group.repo)
                    }
                }
            }
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always))
            .onSubmit(of: .search) {
                if let first = matches.groupedByRepo().first?.sessions.first { open(first) }
            }
            .navigationTitle("Sessions")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }.keyboardShortcut(.cancelAction)
                }
            }
            // The list can be stale: tmux reports changes only for the bridge's session.
            .task { try? await model.session.refresh() }
        }
    }

    private var matches: [Session] {
        let all = model.visibleSessions
        guard !query.isEmpty else { return all }
        return all.filter {
            $0.name.localizedCaseInsensitiveContains(query)
                || ($0.repo ?? "").localizedCaseInsensitiveContains(query)
                || ($0.branch ?? "").localizedCaseInsensitiveContains(query)
        }
    }

    private func open(_ s: Session) {
        dismiss()
        Task { await model.open(s) }
    }
}

struct SessionRow: View {
    let session: Session
    let current: Bool
    /// Shown as its Cmd+number shortcut when it has one.
    var number: Int?

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(session.name).font(.headline)
                // The repo is the section header; the branch tells sessions apart.
                if let detail = [session.branch, session.task].compactMap({ $0 }).joined(separator: " · ").nonEmpty {
                    Text(detail).font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer()
            if let pane = session.defaultPane {
                AgentBadge(pane: pane)
            }
            if current {
                Image(systemName: "checkmark").foregroundStyle(.tint)
            }
            if let number, number <= 9 {
                Text("⌘\(number)").font(.caption.monospaced()).foregroundStyle(.tertiary)
            }
        }
    }
}

/// The agent's state and its age, only when an adapter reported it.
struct AgentBadge: View {
    let pane: Pane

    var body: some View {
        if let agent = pane.agent {
            if let state = agent.shown(shellForeground: pane.shellForeground) {
                HStack(spacing: 4) {
                    Circle().fill(color(state)).frame(width: 8, height: 8)
                    Text(state.rawValue)
                    if let since = agent.since {
                        Text(Self.age(since)).foregroundStyle(.secondary)
                    }
                }
                .font(.caption)
            } else {
                Text("unknown (stale)").font(.caption).foregroundStyle(.secondary)
            }
        } else {
            Text(pane.command).font(.caption.monospaced()).foregroundStyle(.secondary)
        }
    }

    /// One line for a title: "waiting 1m · 100×30", or the command when no adapter reports.
    static func summary(_ pane: Pane) -> String {
        let size = "\(pane.cols)×\(pane.rows)"
        guard let agent = pane.agent else { return "\(pane.command) · \(size)" }
        guard let state = agent.shown(shellForeground: pane.shellForeground) else { return "unknown (stale) · \(size)" }
        let age = agent.since.map { " " + Self.age($0) } ?? ""
        return "\(state.rawValue)\(age) · \(size)"
    }

    private func color(_ s: AgentState) -> Color {
        switch s {
        case .running: .blue
        case .waiting: .orange
        case .idle: .gray
        case .done: .green
        case .error: .red
        }
    }

    private static func age(_ since: UInt64) -> String {
        let seconds = max(0, Int(Date().timeIntervalSince1970) - Int(since))
        switch seconds {
        case ..<60: return "\(seconds)s"
        case ..<3600: return "\(seconds / 60)m"
        case ..<86400: return "\(seconds / 3600)h"
        default: return "\(seconds / 86400)d"
        }
    }
}

extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
