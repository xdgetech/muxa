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
    /// The Work item the Task is for, and its `rev` the client saw (`work` level 9, ADR 0048).
    public var work: String?
    public var workRev: UInt64?

    public init(
        workspace: String, title: String, goal: String, project: String? = nil, repo: String? = nil,
        branch: String? = nil, base: String? = nil, loop: LoopSpec? = nil, worker: String? = nil,
        work: String? = nil, workRev: UInt64? = nil
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
        self.work = work
        self.workRev = workRev
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
    /// The host names an empty branch after the title (`work` level 2, ADR 0031).
    public var branchFromTitle: Bool
    /// Started from a Work item: one Task per item `rev` (ADR 0048).
    public var work: String?
    public var workRev: UInt64?

    public init(
        workspace: String = "", title: String = "", goal: String = "", project: String = "", repo: String = "",
        branch: String = "", base: String = "", loop: LoopDraft? = nil, worker: String? = nil,
        branchFromTitle: Bool = false, work: String? = nil, workRev: UInt64? = nil
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
        self.branchFromTitle = branchFromTitle
        self.work = work
        self.workRev = workRev
    }

    /// A draft in `repo`, in the workspace it belongs to, else in the first
    /// workspace. An empty `repo` means a task directory.
    public init(repo: String, in workspaces: [WorkspaceInfo], branchFromTitle: Bool = false) {
        let workspace = (repo.isEmpty ? nil : workspaces.workspace(of: repo)) ?? workspaces.first
        self.init(workspace: workspace?.name ?? "", repo: repo, branchFromTitle: branchFromTitle)
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
        if branch.isEmpty { return branchFromTitle ? nil : "Name the branch." }
        if let why = TaskDraft.refProblem(branch, what: "branch") { return why }
        let base = base.trimmed
        if !base.isEmpty, let why = TaskDraft.refProblem(base, what: "base") { return why }
        return nil
    }

    /// The `create-task` fields; nil while there is a problem. Branch and base go
    /// only with a repo, and an empty branch not at all.
    public var request: NewTaskRecord? {
        guard problem == nil else { return nil }
        let repo = repo.trimmed.nonEmpty
        return NewTaskRecord(
            workspace: workspace.trimmed, title: title.trimmed, goal: goal.trimmed, project: project.trimmed.nonEmpty,
            repo: repo, branch: repo == nil ? nil : branch.trimmed.nonEmpty, base: repo == nil ? nil : base.trimmed.nonEmpty,
            loop: loop?.spec, worker: worker, work: work, workRev: work == nil ? nil : workRev)
    }
}

private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
    var nonEmpty: String? { isEmpty ? nil : self }
}
