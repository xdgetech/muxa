import Foundation
import MuxaProtocol

/// The Tasks of one workspace in Tasks mode (ADR 0063), grouped by repo.
public struct WorkspaceTasks: Equatable, Sendable, Identifiable {
    public var workspace: String
    public var repos: [RepoTasks]

    public var id: String { foldKey }
    /// Stable across refreshes; the client stores it to keep a group folded.
    public var foldKey: String { "w:\(workspace)" }
}

/// The Tasks of one repo, or of no repo, inside a workspace.
public struct RepoTasks: Equatable, Sendable, Identifiable {
    public static let noRepoLabel = "No repo"

    public var workspace: String
    /// The main checkout, absolute; nil for Tasks without a repo.
    public var repo: String?
    public var label: String
    /// False when the workspace has no real repo: its Tasks sit under the workspace alone.
    public var heading: Bool
    public var items: [TaskItem]

    public var id: String { foldKey }
    public var foldKey: String { "r:\(workspace):\(repo ?? "~")" }
}

extension Array where Element == InboxSection {
    /// Workspaces, then repos, both in byte order with "no repo" last. Items keep
    /// the Inbox order: what they need, then the newest change.
    public func groupedByWorkspace(workspaces: [WorkspaceInfo]) -> [WorkspaceTasks] {
        let items = flatMap(\.items)
        let names = Set(items.map(\.record.workspace)).sorted(by: byteOrder)
        return names.map { name in
            let root = workspaces.first { $0.name == name }?.root
            let members = items.filter { $0.record.workspace == name }
            let dirs = Set(members.compactMap(\.record.repo))
            let labelled = dirs.map { (dir: $0, label: RepoChoice.displayName($0, root: root)) }
                .sorted { byteOrder($0.label, $1.label) || ($0.label == $1.label && byteOrder($0.dir, $1.dir)) }
            var repos = labelled.map { repo in
                RepoTasks(workspace: name, repo: repo.dir, label: repo.label, heading: true,
                          items: members.filter { $0.record.repo == repo.dir })
            }
            let loose = members.filter { $0.record.repo == nil }
            if !loose.isEmpty {
                repos.append(RepoTasks(workspace: name, repo: nil, label: RepoTasks.noRepoLabel,
                                       heading: !repos.isEmpty, items: loose))
            }
            return WorkspaceTasks(workspace: name, repos: repos)
        }
    }
}

/// The desktop tree sorts with `LC_ALL=C`; byte order keeps both clients alike.
private func byteOrder(_ a: String, _ b: String) -> Bool {
    a.utf8.lexicographicallyPrecedes(b.utf8)
}
