import Foundation
import MuxaV2

/// What opens the Start Task sheet: empty, or prefilled from an Inbox item or a capture.
struct TaskSeed: Identifiable, Equatable {
    let id = UUID()
    var title = ""
    var goal: String?
    var workspace: String?
}

/// One `start-task` request (protocol v2, Task actions).
struct TaskRequest: Equatable {
    var workspace: String
    var title: String
    var goal: String?
    var places: [PlaceSpec]
    var connections: [String]
    var mcp: [String]
    var plan: Bool
    var loop: LoopSpec?
}

/// The Start Task form's choices for one Workspace.
struct TaskDraft {
    /// How the Task uses one of the Workspace's repos (ADR 0068).
    enum RepoUse: String, CaseIterable, Identifiable {
        case none = "Not used", branch = "Branch", read = "Read only"
        var id: Self { self }
    }

    var title: String
    var goal: String
    var workspace = ""
    var folder = true
    var repos: [String: RepoUse] = [:]
    var branches: [String: String] = [:]
    /// The connections and MCP servers named besides the default ones.
    var connections: Set<String> = []
    var mcp: Set<String> = []
    var plan = false
    var loop = false
    var check = ""
    var attempts = 3

    init(seed: TaskSeed) {
        title = seed.title
        goal = seed.goal ?? ""
    }

    /// A new Workspace: its repos, connections and checks differ.
    mutating func choose(_ ws: Workspace) {
        workspace = ws.name
        folder = true
        repos = [:]
        branches = [:]
        connections = []
        mcp = []
        loop = false
        if case .list(let checks)? = ws.loop?.checks { check = checks.first ?? "" } else { check = "" }
    }

    func use(of repo: String) -> RepoUse { repos[repo] ?? .none }

    /// A branch worktree usually is where the agent works: the folder is then not needed.
    mutating func setUse(_ use: RepoUse, of repo: String) {
        repos[repo] = use
        if use == .branch { folder = false }
    }

    /// What still keeps Start off, or nil.
    func missing(in ws: Workspace) -> String? {
        if trimmed(title).isEmpty { return "Name the Task." }
        if let repo = ws.repos.first(where: { use(of: $0) == .branch && trimmed(branches[$0] ?? "").isEmpty }) {
            return "Name the branch for \(repo)."
        }
        if loop, ws.loop?.checks == .any, trimmed(check).isEmpty { return "Type the loop's check." }
        return nil
    }

    func request(in ws: Workspace) -> TaskRequest {
        var places: [PlaceSpec] = folder ? [.folder] : []
        for repo in ws.repos {
            switch use(of: repo) {
            case .none: break
            case .branch: places.append(.branch(repo: repo, branch: trimmed(branches[repo] ?? "")))
            case .read: places.append(.read(repo: repo))
            }
        }
        let goal = trimmed(goal)
        return TaskRequest(
            workspace: ws.name, title: trimmed(title), goal: goal.isEmpty ? nil : goal, places: places,
            connections: ws.connections.filter { !$0.isDefault && connections.contains($0.name) }.map(\.name),
            mcp: ws.mcp.filter { !$0.isDefault && mcp.contains($0.name) }.map(\.name),
            plan: plan,
            loop: loop && ws.loop != nil
                ? LoopSpec(check: trimmed(check).isEmpty ? nil : trimmed(check), maxAttempts: UInt32(attempts)) : nil)
    }

    private func trimmed(_ s: String) -> String { s.trimmingCharacters(in: .whitespacesAndNewlines) }
}
