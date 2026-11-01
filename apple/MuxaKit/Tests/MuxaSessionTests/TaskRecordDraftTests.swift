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
}
