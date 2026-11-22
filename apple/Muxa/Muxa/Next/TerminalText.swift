import Foundation
import MuxaV2

/// How a terminal reads in lists and headers, from its `muxa.*` labels.
extension Terminal {
    /// A Task's branch, a Routine run's Routine, the terminal's name, its
    /// folder or the program it reports; `Terminal t3` when it has none.
    var title: String {
        if let taskTitle, !taskTitle.isEmpty { return taskTitle }
        if let branch { return branch }
        if let routine = routineName { return "\(routine) run" }
        if let name, !name.isEmpty { return name }
        if let dir = taskDir { return Self.lastPart(dir) }
        if let app = status?.app, !app.isEmpty { return app }
        return "Terminal \(id)"
    }

    /// The Routine whose run this agent is (`muxa.kind = routine`).
    var routineName: String? {
        labels["muxa.kind"] == "routine" ? labels["muxa.routine"] : nil
    }

    var state: TaskState { TaskState(status?.state) }

    var workspace: String? { labels["muxa.workspace"] }

    /// The working place: `app branch`, `docs · read only`, `folder · reads 2 repos`.
    var placeText: String? {
        if let first = places.first {
            let others = places.count - 1
            let base = Self.describe(first)
            if first.kind == .folder, others > 0, places.dropFirst().allSatisfy({ $0.kind == .read }) {
                return "folder · reads \(others) \(others == 1 ? "repo" : "repos")"
            }
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
        case .checkout: "\(lastPart(p.path)) \(p.branch ?? "checkout")"
        }
    }

    private static func lastPart(_ path: String) -> String {
        (path as NSString).lastPathComponent
    }
}
