import Foundation
import MuxaV2

/// How a terminal reads in lists and headers, from its `muxa.*` labels.
extension Terminal {
    var title: String { branch ?? name ?? id }

    var state: TaskState { TaskState(status?.state) }

    var workspace: String? { labels["muxa.workspace"] }

    /// The working place: `app branch`, `docs · read only`, `folder + 2 repos`.
    var placeText: String? {
        if let first = places.first {
            let others = places.count - 1
            let base = Self.describe(first)
            return others > 0 ? "\(base) + \(others) \(others == 1 ? "repo" : "repos")" : base
        }
        guard let repo = labels["muxa.repo"] else { return taskDir.map(Self.lastPart) }
        return branch == nil ? Self.lastPart(repo) : "\(Self.lastPart(repo)) branch"
    }

    /// The Workspace and the working place, as a Task's header shows them.
    var pathText: String {
        let parts = [workspace, placeText].compactMap { $0 }
        return parts.isEmpty ? (isTaskAgent ? "Task" : "Terminal") : parts.joined(separator: " · ")
    }

    private static func describe(_ p: Place) -> String {
        switch p.kind {
        case .folder: "folder"
        case .branch: "\(lastPart(p.path)) branch"
        case .read: "\(lastPart(p.path)) · read only"
        }
    }

    private static func lastPart(_ path: String) -> String {
        (path as NSString).lastPathComponent
    }
}
