import Foundation
import MuxaProtocol

/// The fields of `create-task`, trimmed; optional ones are nil when empty.
public struct NewTaskRecord: Equatable, Sendable {
    public var workspace: String
    public var title: String
    public var goal: String
    public var project: String?
    public var repo: String?
    public var branch: String?
    public var base: String?
    /// Also start a Task Loop (ADR 0019).
    public var loop: LoopSpec?
    public var worker: String?

    public init(
        workspace: String, title: String, goal: String, project: String? = nil, repo: String? = nil,
        branch: String? = nil, base: String? = nil, loop: LoopSpec? = nil, worker: String? = nil
    ) {
        self.workspace = workspace
        self.title = title
        self.goal = goal
        self.project = project
        self.repo = repo
        self.branch = branch
        self.base = base
        self.loop = loop
        self.worker = worker
    }
}

/// What the New Task sheet for a task record edits. Checked here for quick
/// feedback; the bridge checks again and has the final say.
public struct TaskRecordDraft: Equatable, Sendable {
    public var workspace: String
    public var title: String
    public var goal: String
    public var project: String
    /// Empty: the task gets a directory in its workspace.
    public var repo: String
    public var branch: String
    /// Empty: the repo's default branch.
    public var base: String
    /// Also start a Task Loop; nil for none.
    public var loop: LoopDraft?
    public var worker: String?

    public init(
        workspace: String = "", title: String = "", goal: String = "", project: String = "", repo: String = "",
        branch: String = "", base: String = "", loop: LoopDraft? = nil, worker: String? = nil
    ) {
        self.workspace = workspace
        self.title = title
        self.goal = goal
        self.project = project
        self.repo = repo
        self.branch = branch
        self.base = base
        self.loop = loop
        self.worker = worker
    }

    /// A draft in `repo`, in the workspace whose `repos` lists it, else in the
    /// first workspace. An empty `repo` means a task directory.
    public init(repo: String, in workspaces: [WorkspaceInfo]) {
        let workspace = workspaces.first { $0.repos.contains(repo) } ?? workspaces.first
        self.init(workspace: workspace?.name ?? "", repo: repo)
    }

    /// What to fix before the task can be created; nil when it can.
    public var problem: String? {
        if workspace.trimmed.isEmpty { return "Pick a workspace." }
        if title.trimmed.isEmpty { return "Give the task a title." }
        if goal.trimmed.isEmpty { return "Describe the goal." }
        if let why = loop?.problem { return why }
        let repo = repo.trimmed
        if repo.isEmpty { return nil }
        if !repo.hasPrefix("/") { return "The repository path must start with /." }
        let branch = branch.trimmed
        if branch.isEmpty { return "Name the branch." }
        if let why = TaskDraft.refProblem(branch, what: "branch") { return why }
        let base = base.trimmed
        if !base.isEmpty, let why = TaskDraft.refProblem(base, what: "base") { return why }
        return nil
    }

    /// The `create-task` fields; nil while there is a problem. Branch and base go
    /// only with a repo.
    public var request: NewTaskRecord? {
        guard problem == nil else { return nil }
        let repo = repo.trimmed.nonEmpty
        return NewTaskRecord(
            workspace: workspace.trimmed, title: title.trimmed, goal: goal.trimmed, project: project.trimmed.nonEmpty,
            repo: repo, branch: repo == nil ? nil : branch.trimmed, base: repo == nil ? nil : base.trimmed.nonEmpty,
            loop: loop?.spec, worker: worker)
    }
}

private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
    var nonEmpty: String? { isEmpty ? nil : self }
}
