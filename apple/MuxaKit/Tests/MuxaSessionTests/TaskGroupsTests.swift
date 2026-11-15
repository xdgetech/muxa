import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

struct TaskGroupsTests {
    /// `protocol/fixtures/tasks/grouping.jsonl` pins the rule shared with `TaskGroups.kt`.
    @Test func groupsMatchTheSharedFixture() throws {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("../../../../protocol/fixtures/tasks/grouping.jsonl").standardized
        struct Repo: Decodable, Equatable {
            var repo: String?
            var label: String
            var heading: Bool
            var tasks: [String]
        }
        struct Group: Decodable, Equatable {
            var workspace: String
            var repos: [Repo]
        }
        struct Case: Decodable {
            var name: String
            var now: UInt64?
            var workspaces: [WorkspaceInfo]
            var records: [TaskRecord]
            var sessions: [Session]
            var groups: [Group]
        }
        let lines = try String(contentsOf: url, encoding: .utf8).split(separator: "\n")
        #expect(lines.count == 4)
        for line in lines {
            let c = try JSONDecoder().decode(Case.self, from: Data(line.utf8))
            let got = c.records.inbox(sessions: c.sessions, now: c.now).groupedByWorkspace(workspaces: c.workspaces)
                .map { w in
                    Group(workspace: w.workspace, repos: w.repos.map {
                        Repo(repo: $0.repo, label: $0.label, heading: $0.heading, tasks: $0.items.map(\.id))
                    })
                }
            #expect(got == c.groups, "\(c.name)")
        }
    }

    @Test func foldKeysDependOnlyOnWorkspaceAndRepo() {
        let early = [InboxTests.record("a", updated: 1)].map {
            var r = $0
            r.repo = "/src/app"
            return r
        }
        var later = early
        later[0].updatedAt = 9
        later[0].status = .done
        let keys = { (records: [TaskRecord]) in
            records.inbox(sessions: []).groupedByWorkspace(workspaces: []).flatMap { [$0.foldKey] + $0.repos.map(\.foldKey) }
        }
        #expect(keys(early) == ["w:work", "r:work:/src/app"])
        #expect(keys(later) == keys(early))
    }
}
