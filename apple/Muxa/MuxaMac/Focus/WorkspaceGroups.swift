import Foundation
import MuxaV2

/// Focus's sidebar tree: each Workspace's repos with the Tasks that work in
/// them, then the Tasks without a repo. Pure, from the Workspaces and the live
/// Task agents.
struct WorkspaceGroup: Identifiable {
    struct Repo: Identifiable {
        let name: String
        /// The checked-out branch; nil on a detached HEAD.
        let branch: String?
        /// The agent in the repo's own checkout (ADR 0077), when one runs.
        let checkout: Terminal?
        let tasks: [Terminal]
        var id: String { name }
    }

    let workspace: Workspace
    let repos: [Repo]
    /// Tasks of the Workspace that work in none of its repos.
    let loose: [Terminal]
    /// Every live Task of the Workspace but the checkout agents.
    let count: Int
    var id: String { workspace.name }

    /// One group per Workspace, in the server's order. `hidden` Tasks (the
    /// ones Needs you lists) count but get no row. A Task whose Workspace is
    /// not in the list comes back in `unplaced`.
    static func make(_ workspaces: [Workspace], tasks: [Terminal], hidden: Set<String> = [])
        -> (groups: [WorkspaceGroup], unplaced: [Terminal])
    {
        let names = Set(workspaces.map(\.name))
        let groups = workspaces.map { ws in
            let own = tasks.filter { $0.workspace == ws.name }
            let checkouts = own.filter { $0.labels["muxa.checkout"] != nil }
            let work = own.filter { $0.labels["muxa.checkout"] == nil }
            let shown = work.filter { !hidden.contains($0.id) }
            let repos = ws.repos.map { repo in
                Repo(
                    name: repo, branch: ws.branches[repo],
                    checkout: checkouts.first { $0.labels["muxa.checkout"] == repo },
                    tasks: shown.filter { repoName(of: $0, in: ws.repos) == repo })
            }
            let loose = shown.filter { repoName(of: $0, in: ws.repos) == nil }
            return WorkspaceGroup(workspace: ws, repos: repos, loose: loose, count: work.count)
        }
        let unplaced = tasks.filter { t in
            !hidden.contains(t.id) && !(t.workspace.map(names.contains) ?? false)
        }
        return (groups, unplaced)
    }

    /// The repo a Task works in: its branch's main checkout, else a read-only
    /// or checkout working place. Repo names may be nested (`group/lib`), so
    /// a path matches by its suffix; the longest name wins.
    static func repoName(of t: Terminal, in repos: [String]) -> String? {
        var paths = [t.labels["muxa.repo"]].compactMap { $0 }
        if let first = t.places.first, first.kind == .read || first.kind == .checkout { paths.append(first.path) }
        for path in paths {
            let trimmed = path.hasSuffix("/") ? String(path.dropLast()) : path
            let match = repos
                .filter { trimmed == $0 || trimmed.hasSuffix("/" + $0) }
                .max { $0.count < $1.count }
            if let match { return match }
        }
        return nil
    }
}
