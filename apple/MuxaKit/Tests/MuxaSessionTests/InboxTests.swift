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
