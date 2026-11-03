import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

struct InboxTests {
    static func record(_ id: String, _ status: TaskStatus = .open, updated: UInt64 = 1) -> TaskRecord {
        TaskRecord(id: id, rev: 1, workspace: "work", title: id, goal: "g", status: status,
                   createdAt: 1, updatedAt: updated)
    }

    static func session(_ taskId: String?, _ state: AgentState?, shellForeground: Bool = false, role: Role = .agent) -> Session {
        Session(id: "$\(taskId ?? "x")", name: taskId ?? "x", taskId: taskId, panes: [
            Pane(id: "%\(taskId ?? "x")", window: "@1", index: 0, role: role, cols: 80, rows: 24, command: "claude",
                 path: "/w", dead: false, shellForeground: shellForeground,
                 agent: state.map { Agent(state: $0, since: 100) }),
        ])
    }

    func group(_ record: TaskRecord, _ sessions: [Session]) -> InboxGroup {
        InboxItem(record: record, sessions: sessions).group
    }

    /// `protocol/fixtures/inbox` pins the rule shared with `muxa-cli` and `Inbox.kt`.
    @Test func groupsMatchTheSharedFixture() throws {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("../../../../protocol/fixtures/inbox/groups.jsonl").standardized
        let names: [String: InboxGroup] = [
            "needs-attention": .needsAttention, "ready-for-review": .readyForReview, "running": .running,
            "no-agent": .noAgent, "closed": .closed,
        ]
        struct Case: Decodable {
            var name: String
            var record: TaskRecord
            var sessions: [Session]
            var group: String
            var need: String
        }
        let lines = try String(contentsOf: url, encoding: .utf8).split(separator: "\n")
        #expect(lines.count == 28)
        for line in lines {
            let c = try JSONDecoder().decode(Case.self, from: Data(line.utf8))
            #expect(InboxItem(record: c.record, sessions: c.sessions).need.rawValue == c.need, "\(c.name)")
            #expect(group(c.record, c.sessions) == names[c.group], "\(c.name)")
        }
    }

    /// `protocol/fixtures/inbox/queue.jsonl` pins Move down and Later for all three clients.
    @Test func theQueueMatchesTheSharedFixture() throws {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("../../../../protocol/fixtures/inbox/queue.jsonl").standardized
        struct Case: Decodable {
            var name: String
            var now: UInt64
            var records: [TaskRecord]
            var sessions: [Session]
            var queue: [String]
        }
        let lines = try String(contentsOf: url, encoding: .utf8).split(separator: "\n")
        #expect(lines.count == 16)
        for line in lines {
            let c = try JSONDecoder().decode(Case.self, from: Data(line.utf8))
            #expect(c.records.triageQueue(sessions: c.sessions, now: c.now).map(\.id) == c.queue, "\(c.name)")
        }
    }

    @Test func aLoopOfAnUnknownStateFollowsTheAgent() {
        var record = Self.record("a")
        record.loop = TaskLoop(
            check: "make check", maxAttempts: 3, timeout: 600, state: .other("resting"), attempts: 0,
            armedAt: 1, since: 1)
        #expect(group(record, [Self.session("a", .running)]) == .running)
        record.loop?.state = .stopped
        record.loop?.reason = .other("quota")
        #expect(group(record, [Self.session("a", .running)]) == .needsAttention)
    }

    @Test func theAgentStateDecidesTheGroupOfAnOpenTask() {
        #expect(group(Self.record("a"), [Self.session("a", .waiting)]) == .needsAttention)
        #expect(group(Self.record("a"), [Self.session("a", .error)]) == .needsAttention)
        #expect(group(Self.record("a"), [Self.session("a", .running)]) == .running)
        #expect(group(Self.record("a"), [Self.session("a", .idle)]) == .readyForReview)
        #expect(group(Self.record("a"), [Self.session("a", .done)]) == .readyForReview)
        #expect(group(Self.record("a", .review), [Self.session("a", .idle)]) == .readyForReview)
        #expect(group(Self.record("a", .other("later")), [Self.session("a", .done)]) == .readyForReview)
    }

    @Test func noStateIsNeverGuessed() {
        // No session, a session of another task, no adapter state, a shell in front.
        #expect(group(Self.record("a"), []) == .noAgent)
        #expect(group(Self.record("a"), [Self.session("b", .running)]) == .noAgent)
        #expect(group(Self.record("a"), [Self.session("a", nil)]) == .noAgent)
        let stale = InboxItem(record: Self.record("a"), sessions: [Self.session("a", .running, shellForeground: true)])
        #expect(stale.group == .noAgent)
        #expect(stale.agent == nil)
        #expect(stale.session != nil)
    }

    @Test func closedTasksAreClosedWhateverTheAgentDoes() {
        #expect(group(Self.record("a", .done), [Self.session("a", .waiting)]) == .closed)
        #expect(group(Self.record("a", .dropped), []) == .closed)
    }

    @Test func anUntaggedPaneWithAStateIsTheAgent() {
        let item = InboxItem(record: Self.record("a"), sessions: [Self.session("a", .waiting, role: .unknown)])
        #expect(item.group == .needsAttention)
        #expect(item.agent?.since == 100)
    }

    @Test func snoozeIdentityChangesWhenSessionAppearsOrAgentChanges() {
        let r = Self.record("a")
        let absent = InboxItem(record: r, sessions: [])
        let linked = InboxItem(record: r, sessions: [Self.session("a", nil)])
        let waiting = InboxItem(record: r, sessions: [Self.session("a", .waiting)])
        #expect(absent.attentionKey != linked.attentionKey)
        #expect(linked.attentionKey != waiting.attentionKey)
        #expect(waiting.attentionKey == InboxItem(record: r, sessions: [Self.session("a", .waiting)]).attentionKey)
    }

    @Test func queueExcludesRunningAndClosedAndBreaksTiesById() {
        let records = [Self.record("a"), Self.record("b"), Self.record("run"), Self.record("closed", .done)]
        let queue = records.triageQueue(sessions: [Self.session("run", .running)])
        #expect(queue.map(\.id) == ["b", "a"])
        #expect(queue.allSatisfy { $0.need == .noSession })
    }

    @Test func groupsComeInOrderNewestFirstAndEmptyOnesAreLeftOut() {
        let records = [
            Self.record("old", updated: 1), Self.record("new", updated: 5), Self.record("run", updated: 3),
            Self.record("gone", .done, updated: 9),
        ]
        let sessions = [Self.session("old", .waiting), Self.session("new", .waiting), Self.session("run", .running)]
        let inbox = records.inbox(sessions: sessions)
        #expect(inbox.map(\.group) == [.needsAttention, .running, .closed])
        #expect(inbox[0].items.map(\.id) == ["new", "old"])
    }
}
