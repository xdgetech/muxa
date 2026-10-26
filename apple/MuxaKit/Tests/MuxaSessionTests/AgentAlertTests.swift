import MuxaProtocol
import Testing

@testable import MuxaSession

struct AgentAlertTests {
    func session(_ id: String, _ pane: String, _ state: AgentState?, shell: Bool = false) -> Session {
        Session(id: id, name: "s\(id)", panes: [
            Pane(id: pane, window: "@1", index: 0, role: .agent, cols: 80, rows: 24, command: "claude",
                 path: "/w", dead: false, shellForeground: shell, agent: state.map { Agent(state: $0) }),
        ])
    }

    @Test func attentionStatesAreTold() {
        let old = [session("$1", "%1", .running), session("$2", "%2", .running), session("$3", "%3", .idle)]
        let new = [session("$1", "%1", .waiting), session("$2", "%2", .error), session("$3", "%3", .done)]
        #expect(new.alerts(since: old, shown: nil).map(\.kind) == [.waiting, .failed, .done])
        #expect(new.alerts(since: old, shown: nil).first?.message == "needs your input")
    }

    @Test func aTurnEndingInIdleIsDone() {
        let alerts = [session("$1", "%1", .idle)].alerts(since: [session("$1", "%1", .running)], shown: nil)
        #expect(alerts.map(\.kind) == [.done])
        #expect([session("$1", "%1", .running)].alerts(since: [session("$1", "%1", .idle)], shown: nil).isEmpty)
    }

    @Test func quietCasesAreNotTold() {
        let waiting = [session("$1", "%1", .waiting)]
        // Unchanged, the shown session, a new pane (the first list), and a stale state.
        #expect(waiting.alerts(since: waiting, shown: nil).isEmpty)
        #expect(waiting.alerts(since: [session("$1", "%1", .running)], shown: "$1").isEmpty)
        #expect(waiting.alerts(since: [], shown: nil).isEmpty)
        #expect([session("$1", "%1", .waiting, shell: true)].alerts(since: [session("$1", "%1", .running)], shown: nil).isEmpty)
    }
}
