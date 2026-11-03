import MuxaProtocol
import Testing

@testable import MuxaSession

struct TaskRecordDraftTests {
    @Test func aTaskWithoutARepoNeedsAWorkspaceATitleAndAGoal() throws {
        #expect(TaskRecordDraft(title: "T", goal: "G").problem == "Pick a workspace.")
        #expect(TaskRecordDraft(workspace: "work", goal: "G").problem == "Give the task a title.")
        #expect(TaskRecordDraft(workspace: "work", title: "T", goal: " \n").problem == "Describe the goal.")
        let req = try #require(
            TaskRecordDraft(workspace: "work", title: " T ", goal: "G\n", project: " ", branch: "ignored").request)
        #expect(req == NewTaskRecord(workspace: "work", title: "T", goal: "G"))
    }

    @Test func aRepoNeedsAnAbsolutePathAndABranch() throws {
        var draft = TaskRecordDraft(workspace: "work", title: "T", goal: "G", project: "App", repo: "src/app")
        #expect(draft.problem == "The repository path must start with /.")
        draft.repo = "/src/app"
        #expect(draft.problem == "Name the branch.")
        draft.branch = "a b"
        #expect(draft.problem != nil)
        draft.branch = "feat/x"
        draft.base = "-x"
        #expect(draft.problem != nil)
        draft.base = ""
        let req = try #require(draft.request)
        #expect(req == NewTaskRecord(
            workspace: "work", title: "T", goal: "G", project: "App", repo: "/src/app", branch: "feat/x"))
    }

    @Test func aRepoPicksTheWorkspaceThatListsIt() {
        let workspaces = [
            WorkspaceInfo(name: "work", repos: ["/src/app"]),
            WorkspaceInfo(name: "docs", repos: ["/src/docs", "/src/site"]),
        ]
        let draft = TaskRecordDraft(repo: "/src/site", in: workspaces)
        #expect(draft.workspace == "docs")
        #expect(draft.repo == "/src/site")
        #expect(TaskRecordDraft(repo: "/src/other", in: workspaces).workspace == "work")
        #expect(TaskRecordDraft(repo: "", in: workspaces).workspace == "work")
        #expect(TaskRecordDraft(repo: "/src/app", in: []).workspace == "")
    }

    @Test func aRepoUnderARootPicksItsWorkspace() {
        let workspaces = [WorkspaceInfo(name: "work"), WorkspaceInfo(name: "lisk", root: "/lisk")]
        #expect(TaskRecordDraft(repo: "/lisk/lisk-app", in: workspaces).workspace == "lisk")
    }

    /// A `work` level 2 host names the branch after the title (ADR 0031).
    @Test func aHostThatNamesBranchesNeedsNone() throws {
        var draft = TaskRecordDraft(workspace: "work", title: "T", goal: "G", repo: "/src/app", branchFromTitle: true)
        #expect(draft.problem == nil)
        #expect(try #require(draft.request) == NewTaskRecord(workspace: "work", title: "T", goal: "G", repo: "/src/app"))
        draft.branch = " feat/x "
        #expect(try #require(draft.request).branch == "feat/x")
        draft.branch = "a b"
        #expect(draft.problem != nil, "a typed branch is still checked")
        #expect(TaskRecordDraft(repo: "/src/app", in: [WorkspaceInfo(name: "work")], branchFromTitle: true).branchFromTitle)
    }
}
