import Foundation
import MuxaProtocol

/// The demo bridge's Captures and Work items (`work` level 8, ADR 0046), kept in memory,
/// with the host's checks: one Capture per `clientId`, a stale `rev` is refused, and
/// one Task per Work item `rev`. Its assistant requests are in `DemoAssistant+Requests.swift`.
struct DemoAssistant: Sendable {
    var captures: [Capture]
    var work: [WorkItem]
    /// The Task made for each `(work, rev)`, so asking twice makes one.
    var tasksByRev: [String: String] = [:]
    var requests: [AssistantRequest] = []
    /// The live run of each request, by request id; its delivery follows from its age.
    var runs: [String: DemoRun] = [:]
    /// The request made for each `(work, rev)`, so asking twice makes one version.
    var requestsByRev: [String: String] = [:]
    /// Runs started since launch, for their record ids.
    var runCount = 0

    typealias Refusal = DemoWork.Refusal

    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// Unsorted ones, newest first.
    var shownCaptures: [Capture] {
        captures.filter { $0.state == .received }.sorted { $0.createdAt > $1.createdAt }
    }

    /// Pinned open ones first, in the order pinned (`work` level 9, ADR 0047 §6); then the
    /// other open ones, newest first; then the moved-down ones in the order moved; then
    /// done and withdrawn, newest change first. `waiting` derives a linked Task's open item.
    /// A live run makes the item in progress, as a running Task does.
    func shownWork(waiting: (String) -> Bool) -> [WorkItem] {
        let derived = work.map { item -> WorkItem in
            let running = item.requests.contains { runs[$0] != nil }
            guard item.state == .open, !item.tasks.isEmpty || running else { return item }
            var shown = item
            shown.state = item.tasks.contains(where: waiting) ? .waitingDecision : .inProgress
            return shown
        }
        let pinned = derived.filter { $0.isListedOpen && $0.pinnedAt != nil }.sorted { ($0.pinnedAt ?? 0) < ($1.pinnedAt ?? 0) }
        let open = pinned + derived.filter { $0.isListedOpen && $0.laterAt == nil && $0.pinnedAt == nil }
            .sorted { $0.createdAt > $1.createdAt }
        let later = derived.filter { $0.isListedOpen && $0.laterAt != nil }.sorted { ($0.laterAt ?? 0) < ($1.laterAt ?? 0) }
        let closed = derived.filter { !$0.isListedOpen }.sorted { $0.updatedAt > $1.updatedAt }
        return open + later + closed.prefix(100)
    }

    mutating func capture(clientId: String, text: String, workspace: String?) throws(Refusal) -> Capture {
        if let stored = captures.first(where: { $0.clientId == clientId }) { return stored }
        guard !text.isEmpty, text.utf8.count <= 16 * 1024 else { throw Refusal(.badRequest, "the text must be 1 byte to 16 KiB") }
        let c = Capture(id: Self.id("c", captures.count + 0x100), rev: 1, clientId: clientId, text: text, source: .app,
                        state: .received, createdAt: Self.now, updatedAt: Self.now, workspace: workspace)
        captures.append(c)
        return c
    }

    mutating func sort(
        _ id: String, rev: UInt64, action: SortAction, into target: String?, title: String?, workspace: String? = nil
    ) throws(Refusal) -> (Capture, WorkItem?) {
        guard let i = captures.firstIndex(where: { $0.id == id }) else { throw Refusal(.badRequest, "\(id) is not a capture") }
        guard captures[i].rev == rev else { throw Refusal(.conflict, "\(id) changed since you read it") }
        guard captures[i].state == .received else { throw Refusal(.badRequest, "\(id) is sorted") }
        var item: WorkItem?
        switch action {
        case .work:
            let made = WorkItem(
                id: Self.id("w", work.count + 0x100), rev: 1,
                title: title ?? String(Self.firstLine(captures[i].text).prefix(200)), state: .open,
                createdAt: Self.now, updatedAt: Self.now, workspace: workspace ?? captures[i].workspace, captures: [id])
            work.append(made)
            item = made
        case .link:
            guard let target, let w = work.firstIndex(where: { $0.id == target && $0.isListedOpen }) else {
                throw Refusal(.badRequest, "\(target ?? "nothing") is not an open Work item")
            }
            work[w].captures.append(id)
            touch(w)
            item = work[w]
        case .note:
            break
        }
        captures[i].state = .sorted
        captures[i].rev += 1
        captures[i].updatedAt = Self.now
        if let item { captures[i].work = [item.id] }
        return (captures[i], item)
    }

    /// Later, complete, withdraw and reopen; `laterAt` is the next place at the bottom.
    /// At level 9 also pin, unpin, and edit with `due`; a pin of a pinned item, or an
    /// unpin of an unpinned one, keeps `rev`.
    mutating func update(
        _ id: String, rev: UInt64, action: WorkAction, laterAt: UInt64, title: String? = nil, workspace: String? = nil,
        due: Due? = nil
    ) throws(Refusal) -> WorkItem {
        guard let i = work.firstIndex(where: { $0.id == id }) else { throw Refusal(.badRequest, "\(id) is not a Work item") }
        guard work[i].rev == rev else { throw Refusal(.conflict, "\(id) changed since you read it") }
        switch action {
        case .later:
            work[i].laterAt = laterAt
            work[i].pinnedAt = nil
        case .complete, .withdraw:
            work[i].state = action == .complete ? .done : .withdrawn
            work[i].pinnedAt = nil
        case .reopen:
            work[i].state = .open
            work[i].laterAt = nil
        case .pin:
            guard work[i].isListedOpen else { throw Refusal(.badRequest, "\(id) is \(work[i].state.raw)") }
            guard work[i].pinnedAt == nil else { return work[i] }
            work[i].pinnedAt = max(Self.now, (work.compactMap(\.pinnedAt).max() ?? 0) + 1)
            work[i].laterAt = nil
        case .unpin:
            guard work[i].pinnedAt != nil else { return work[i] }
            work[i].pinnedAt = nil
        case .edit:
            if let due {
                guard due.at > 0, TimeZone(identifier: due.timezone) != nil else {
                    throw Refusal(.badRequest, "due needs a time and a known time zone")
                }
                work[i].due = due
            }
            if let title { work[i].title = String(title.prefix(200)) }
            if let workspace { work[i].workspace = workspace }
        case .merge:
            throw Refusal(.unsupported, "The demo bridge does not merge Work items.")
        }
        touch(i)
        return work[i]
    }

    /// The Task already made for this `rev`, if any.
    func task(for id: String, rev: UInt64) -> String? { tasksByRev["\(id) \(rev)"] }

    mutating func taskStarted(_ task: String, for id: String, rev: UInt64) {
        tasksByRev["\(id) \(rev)"] = task
        guard let i = work.firstIndex(where: { $0.id == id }) else { return }
        work[i].tasks.append(task)
        touch(i)
    }

    /// The latest `laterAt`, so the next one goes below it.
    var lastLaterAt: UInt64 { work.compactMap(\.laterAt).max() ?? 0 }

    private mutating func touch(_ i: Int) {
        work[i].rev += 1
        work[i].updatedAt = Self.now
    }

    private static func id(_ prefix: String, _ n: Int) -> String { String(format: "\(prefix)-%08x", n) }

    private static func firstLine(_ text: String) -> String {
        text.split(whereSeparator: \.isNewline).first.map(String.init) ?? text
    }

    // MARK: Fixtures

    /// `into` sorts it into that Work item: it is no longer listed, and only `get-work` shows it.
    private static func capture(
        _ id: String, _ text: String, ago: UInt64, source: CaptureSource = .app, workspace: String? = nil,
        into work: String? = nil
    ) -> Capture {
        Capture(id: id, rev: work == nil ? 1 : 2, clientId: "demo-\(id)", text: text, source: source,
                state: work == nil ? .received : .sorted, createdAt: now - ago, updatedAt: now - ago,
                workspace: workspace, work: work.map { [$0] } ?? [])
    }

    static func item(
        _ id: String, _ title: String, ago: UInt64, workspace: String?, captures: [String] = [], tasks: [String] = [],
        requests: [String] = [], pinnedAgo: UInt64? = nil, laterAgo: UInt64? = nil, dueIn: UInt64? = nil
    ) -> WorkItem {
        WorkItem(id: id, rev: 1 + UInt64(requests.count), title: title, state: .open, createdAt: now - ago,
                 updatedAt: now - ago, workspace: workspace, laterAt: laterAgo.map { now - $0 },
                 pinnedAt: pinnedAgo.map { now - $0 },
                 due: dueIn.map { Due(at: now + $0, timezone: TimeZone.current.identifier) },
                 captures: captures, requests: requests, tasks: tasks)
    }

    /// The UI tests' fixtures: one item waits on the passkey Task's question and holds a
    /// Capture sorted elsewhere, and the billing one has a Result of its second version.
    static let tests = DemoAssistant(
        captures: [
            capture("c-0000c001", "Look into the flaky CI job\nIt failed twice on main today.", ago: 300, source: .tmux),
            capture("c-0000c002", "Dark mode for the docs site?", ago: 900, workspace: "work"),
            capture("c-0000c003", "Passkeys: Safari needs the conditional UI flag", ago: 2000, source: .tmux,
                    workspace: "work", into: "w-0000b002"),
        ],
        work: [
            item("w-0000b001", "Plan the billing migration", ago: 600, workspace: "work", requests: ["q-0000d001"]),
            item("w-0000b002", "Ship passkey sign-in", ago: 1800, workspace: "work", captures: ["c-0000c003"],
                 tasks: ["t-auth01"]),
            item("w-0000b003", "Renew the domain", ago: 3600, workspace: "personal"),
        ],
        requests: [testsRequest])

    static let showcase = DemoAssistant(
        captures: [
            capture("c-5c00c001", "Ask Dana about the EU invoice format\nThey need it before the VAT run.", ago: 1500),
            capture("c-5c00c002", "Try the new Postgres 18 async I/O on staging", ago: 7200, source: .tmux, workspace: "work"),
        ],
        work: [
            item("w-5c00b001", "Stripe webhooks for invoices", ago: 5400, workspace: "work", tasks: ["t-a1b2c3"]),
            item("w-5c00b002", "Write the Q3 pricing memo", ago: 9000, workspace: "work", requests: ["q-5c00d001"]),
            item("w-5c00b003", "Book the team offsite venue", ago: 86_400, workspace: "personal"),
        ],
        requests: [showcaseRequest])
}

extension WorkItem {
    /// Listed with the open ones: neither done nor withdrawn.
    fileprivate var isListedOpen: Bool { state != .done && state != .withdrawn }
}
