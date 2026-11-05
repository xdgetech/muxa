import CryptoKit
import Foundation
import MuxaProtocol

/// The demo bridge's Routines and stored Inbox entries (ADR 0026), kept in memory,
/// with the host's checks: one run per Routine, approve once, no action on a closed entry.
struct DemoWork: Sendable {
    var routines: [RoutineInfo]
    var entries: [InboxEntry]
    /// The sessions of runs that are open at the start.
    var sessions: [Session]
    /// A Task's Inbox items (`DemoItems.swift`); a Routine's items are `entries`.
    var taskItems: [InboxItem]
    /// When a Routine's item was first opened, by entry id.
    var entryReads: [String: UInt64] = [:]
    /// When Later moved each entry down (ADR 0040).
    var entryLaters: [String: UInt64] = [:]
    /// The host has `ask-agent` (`work` level 6, ADR 0041).
    var offersAskAgent = true

    struct Refusal: Error {
        var body: ErrorBody
        init(_ code: ErrorCode, _ message: String) { body = ErrorBody(code: code, message: message) }
    }

    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// The entries as `inbox-list` sends them: a Later whose time passed is open again.
    var shownEntries: [InboxEntry] {
        entries.map { e in
            guard e.disposition == "later", (e.until ?? 0) <= Self.now else { return e }
            var open = e
            open.disposition = "open"
            open.until = nil
            return open
        }
    }

    /// Starts a run now: its record id, and a session whose agent runs.
    mutating func run(workspace: String, routine: String, sessionNumber n: Int) throws(Refusal) -> (RoutineInfo, Session) {
        guard let i = routines.firstIndex(where: { $0.workspace == workspace && $0.name == routine }) else {
            throw Refusal(.badRequest, "\(workspace)/\(routine) is not a routine")
        }
        guard routines[i].enabled else { throw Refusal(.badRequest, "\(workspace)/\(routine) is disabled") }
        guard routines[i].running == nil else { throw Refusal(.conflict, "\(workspace)/\(routine) already runs") }
        let task = String(format: "t-run%03d", n)
        routines[i].running = RoutineRun(occurrence: "now-\(Self.now)", startedAt: Self.now, task: task)
        let session = Self.runSession(routine: routine, task: task, number: n, state: .running)
        return (routines[i], session)
    }

    /// Later moves an item below the others, in the order they were moved: two in one
    /// second still keep their order.
    var nextLaterAt: UInt64 {
        max(Self.now, (Array(entryLaters.values) + taskItems.compactMap(\.laterAt)).max().map { $0 + 1 } ?? 0)
    }

    /// Dismiss and Later change the entry. Approve of an open proposal returns the goal
    /// of the Task to create; `approved(_:task:)` then records it. Approving again
    /// returns the entry unchanged.
    mutating func update(_ id: String, action: InboxAction) throws(Refusal) -> (InboxEntry, goal: String?) {
        guard let i = entries.firstIndex(where: { $0.id == id }) else { throw Refusal(.badRequest, "\(id) is not an entry") }
        let entry = shownEntries[i]
        if action == .approve, entry.disposition == "approved" { return (entry, nil) }
        guard entry.disposition == "open" || entry.disposition == "later" else {
            throw Refusal(.badRequest, "\(id) is \(entry.disposition)")
        }
        switch action {
        case .approve:
            guard let p = entry.proposal, entry.kind == "proposal" else {
                throw Refusal(.badRequest, "a briefing has nothing to approve")
            }
            return (entry, "\(p.action)\nTarget: \(p.target)\n\n\(p.content)\n\nmuxa-proposal: \(entry.id) \(p.hash)")
        case .dismiss:
            entries[i].disposition = "dismissed"
            entries[i].until = nil
        case .later:
            entryLaters[id] = nextLaterAt
        default:
            throw Refusal(.badRequest, "\(action.rawValue) is not an entry's action")
        }
        return (entries[i], nil)
    }

    mutating func approved(_ id: String, task: String) -> InboxEntry {
        guard let i = entries.firstIndex(where: { $0.id == id }) else { preconditionFailure("approved an unknown entry") }
        entries[i].disposition = "approved"
        entries[i].task = task
        return entries[i]
    }

    // MARK: Fixtures

    private static func runSession(routine: String, task: String, number n: Int, state: AgentState, ago: UInt64 = 0) -> Session {
        Session(id: "$\(n)", name: "\(routine)-\(task)", taskId: task, panes: [
            Pane(id: "%\(n)", window: "@\(n)", index: 0, role: .agent, cols: 100, rows: 30, command: "claude",
                 path: "/work/routines/\(routine)-\(task)", dead: false, shellForeground: false,
                 agent: Agent(state: state, since: now - ago)),
        ])
    }

    static func hash(action: String, target: String, content: String) -> String {
        let digest = SHA256.hash(data: Data((action + "\0" + target + "\0" + content).utf8))
        return "sha256:" + digest.map { String(format: "%02x", $0) }.joined()
    }

    private static func entry(
        _ id: String, _ routine: String, _ kind: String, _ title: String, _ body: String, ago: UInt64,
        links: [SourceLink]? = nil, proposal: (action: String, target: String, content: String)? = nil
    ) -> InboxEntry {
        InboxEntry(
            id: id, workspace: "work", routine: routine, key: "\(routine):\(id)", kind: kind, title: title, body: body,
            links: links, createdAt: now - ago, run: "t-r00001", occurrence: "2026-11-03T08:00", disposition: "open",
            until: nil,
            proposal: proposal.map {
                Proposal(action: $0.action, target: $0.target, content: $0.content,
                         hash: hash(action: $0.action, target: $0.target, content: $0.content))
            },
            task: nil)
    }

    private static func routines(running: String?) -> [RoutineInfo] {
        [
            RoutineInfo(
                workspace: "work", name: "slack-digest", trigger: RoutineTrigger(at: "08:00", timezone: "Europe/Berlin"),
                enabled: true, project: "App", nextDue: now + 20 * 3600,
                lastSuccess: RoutineSuccess(at: now - 4 * 3600, occurrence: "2026-11-03T08:00", task: "t-r00001", published: 3),
                lastFailure: RoutineFailure(at: now - 2 * 86_400, occurrence: "2026-11-01T08:00", task: "t-r00000",
                                            reason: "The run t-r00000 ended without .muxa/routine/output.json.")),
            RoutineInfo(
                workspace: "work", name: "linear-triage", trigger: RoutineTrigger(at: "09:30", timezone: "Asia/Tokyo"),
                enabled: true, project: "App", nextDue: now + 22 * 3600,
                running: running.map { RoutineRun(occurrence: "2026-11-03T09:30", startedAt: now - 300, task: $0) }),
            RoutineInfo(workspace: "personal", name: "weekly-memory", enabled: false),
        ]
    }

    /// Newest first, as the host lists them.
    private static let entries: [InboxEntry] = [
        entry("e-000000a1", "slack-digest", "briefing", "Release moved to Thursday",
              "The release thread agreed to ship on Thursday after the QA pass.", ago: 600,
              links: [SourceLink(url: "https://example.slack.com/archives/C01/p1762150000", title: "Release thread")]),
        entry("e-000000a2", "linear-triage", "proposal", "Close the stale issue ENG-12",
              "No activity for 30 days, and the fix shipped in 1.4.", ago: 900,
              proposal: ("Close the issue", "linear:ENG-12", "Closing: no activity for 30 days, fixed in 1.4.")),
        entry("e-000000a3", "slack-digest", "proposal", "Answer the question about the beta",
              "Dana asked when the beta opens.", ago: 1200,
              proposal: ("Reply in the thread", "slack:C01:1762150100", "The beta opens next Monday.")),
    ]

    /// The UI tests' Routines: one with a run that waits on the user, so its need shows in Inbox.
    static let tests = DemoWork(
        routines: routines(running: "t-r00002"), entries: entries,
        sessions: [runSession(routine: "linear-triage", task: "t-r00002", number: 90, state: .waiting, ago: 120)],
        taskItems: testItems)

    static let showcase = DemoWork(routines: routines(running: nil), entries: entries, sessions: [], taskItems: showcaseItems)
}
