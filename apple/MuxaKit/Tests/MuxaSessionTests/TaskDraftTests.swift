import MuxaProtocol
import Testing

@testable import MuxaSession

struct TaskDraftTests {
    @Test func aCompleteDraftBecomesARequest() throws {
        let draft = TaskDraft(repoDir: " /src/app ", branch: "feat/search ", base: "")
        #expect(draft.problem == nil)
        let req = try #require(draft.request)
        #expect(req.repo == "/src/app")
        #expect(req.branch == "feat/search")
        #expect(req.base == nil)
        #expect(TaskDraft(repoDir: "/src/app", branch: "x", base: "origin/main").request?.base == "origin/main")
    }

    @Test func mistakesAreNamedBeforeSending() {
        #expect(TaskDraft(branch: "x").problem == "Pick a repository.")
        #expect(TaskDraft(repoDir: "src/app", branch: "x").problem == "The repository path must start with /.")
        #expect(TaskDraft(repoDir: "/a", branch: " ").problem == "Name the branch.")
        for bad in ["-x", "a b", "a..b", "a:b"] {
            #expect(TaskDraft(repoDir: "/a", branch: bad).problem != nil, "\(bad)")
        }
        #expect(TaskDraft(repoDir: "/a", branch: "x", base: "--all").problem != nil)
        #expect(TaskDraft(repoDir: "/a", branch: "-x").request == nil)
    }

    @Test func knownReposAreListedOnceInOrder() {
        let sessions = [
            Session(id: "$1", name: "a", repo: "app", repoDir: "/src/app", panes: []),
            Session(id: "$2", name: "b", repo: "docs", repoDir: "/src/docs", panes: []),
            Session(id: "$3", name: "c", repo: "app", repoDir: "/src/app", panes: []),
            Session(id: "$4", name: "d", panes: []),
        ]
        #expect(sessions.knownRepos == [KnownRepo(name: "app", dir: "/src/app"), KnownRepo(name: "docs", dir: "/src/docs")])
    }
}
