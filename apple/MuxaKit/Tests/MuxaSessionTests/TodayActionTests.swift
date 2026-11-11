import Foundation
import MuxaProtocol
import Testing

@testable import MuxaSession

/// Today rows that say what to do (`work` level 17, ADR 0059): the verb per target, who a
/// row's Send reaches, the runs a Re-rank has left, and a Work item's Send to the secretary.
@MainActor
struct TodayActionTests {
    static func item(_ id: String, _ kind: ItemKind, question: String = "Q", answers: [InboxAction] = []) -> InboxItem {
        InboxItem(id: id, kind: kind, state: .open, question: question, workspace: "work", title: "T", createdAt: 1,
                  answers: answers)
    }

    @Test func eachTargetKindHasItsVerb() {
        let verbs: [(ItemKind, String)] = [
            (.review, "Review changes"), (.ask, "Allow or deny"), (.result, "Read result"),
            (.write, "Check text to send"), (.failure, "Fix"), (.memory, "Check memory note"),
            (.proposal, "Approve or reject"), (.sort, "Sort capture"), (.delegate, "Approve request"),
            (.hypothesis, "Confirm or reject"), (.handoff, "Approve hand-off"), (.newTask, "Start Task"),
            (.other("later-kind"), "Open"),
        ]
        for (kind, verb) in verbs { #expect(Self.item("i-1", kind).todayVerb == verb) }
        #expect(Self.item("i-1", .briefing, question: "Pick a provider?").todayVerb == "Decide")
        #expect(Self.item("i-1", .briefing, question: "The week in review.").todayVerb == "Read")
    }

    @Test func sendNamesWhoGetsTheText() {
        var result = Self.item("i-1", .result, answers: [.reply])
        result.result = ResultReview(request: "q-1", version: 1, step: 1, work: "w-1", worker: "researcher")
        #expect(result.todaySend == TodaySend(route: .answer(.reply), label: "Send to researcher…"))
        #expect(Self.item("i-1", .review, answers: [.reply]).todaySend?.label == "Send to the Task's agent…")
        #expect(Self.item("i-1", .ask, answers: [.reply]).todaySend?.label == "Deny & send to the Task's agent…")
        #expect(Self.item("i-1", .newTask, answers: [.reply]).todaySend?.label == "Send to the secretary…")
        let briefing = Self.item("i-1", .briefing, answers: [.askAgent])
        #expect(briefing.todaySend == TodaySend(
            route: .answer(.askAgent), label: "Send to the work Ask session, with this briefing…"))
        #expect(Self.item("i-1", .memory, answers: [.approve]).todaySend == nil)
    }

    @Test func aWorkRowSendsToTheSecretaryAtLevel17() async throws {
        let bridge = AskRootBridge()
        bridge.assistant.overallRuns = RunBudget(left: 2, max: 3)
        let session = try await PlanTests().connected(bridge, level: "work:17")
        let row = try #require(session.currentPlan?.top.first)
        #expect(session.todayVerb(row) == "Start")
        #expect(session.todaySend(row) == TodaySend(
            route: .secretary(work: "w-00000001", workspace: "assistant"),
            label: "Send to the secretary (assistant), with this Work item…"))
        #expect(session.overallRuns?.shown == "2 of 3 runs left today")

        let started = try await session.sendToSecretary("Ask legal first.", work: "w-00000001")
        #expect(started.name == "ask-assistant")
        #expect(bridge.typed == ["w-00000001 Ask legal first."])
    }

    /// A level 16 host would open the session and drop the text: the client sends nothing.
    @Test func aLevel16HostTakesNoSendAndShowsNoRuns() async throws {
        let bridge = AskRootBridge()
        bridge.assistant.overallRuns = RunBudget(left: 2, max: 3)
        let session = try await PlanTests().connected(bridge, level: "work:16")
        let row = try #require(session.currentPlan?.top.first)
        #expect(session.todaySend(row) == nil)
        #expect(session.overallRuns == nil)
        let unsupported = PaneSession.Failure.bridge(
            ErrorBody(code: .unsupported, message: PaneSession.secretaryUnsupported))
        await #expect(throws: unsupported) { try await session.sendToSecretary("Hi", work: "w-00000001") }
        #expect(bridge.roots.isEmpty)
    }
}
