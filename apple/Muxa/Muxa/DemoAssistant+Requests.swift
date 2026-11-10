import Foundation
import MuxaProtocol

/// A live run of one request version: an agent session that ends with a canned Result.
struct DemoRun: Sendable {
    /// The run's record id, the session's `taskId`.
    var task: String
    var session: String
    var started: Date
}

/// The demo's assistant requests (ADR 0048 §6–9), with the host's checks. Delivery
/// follows from the run's age: pending, received, processing, then a Result.
extension DemoAssistant {
    /// Seconds after the start at which the run is received, processing, and resulted.
    static let receivedAfter: TimeInterval = 1.5
    static let processingAfter: TimeInterval = 3
    static let resultAfter: TimeInterval = 8

    /// The UI tests' `-demo-no-daemon`: the host refuses every request, as one without the daemon does.
    static let daemonOff: Bool = {
        #if DEBUG
        ProcessInfo.processInfo.arguments.contains("-demo-no-daemon")
        #else
        false
        #endif
    }()

    static let noDaemon = "Automatic processing is off: assistant requests run in the muxa daemon. "
        + "On the host, run: muxa-bridge daemon install"

    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// What a request did to the runs: the session to start, and the ones to close.
    struct RunChange {
        var started: DemoRun?
        var ended: [DemoRun] = []
    }

    /// The requests of the listed items.
    func listedRequests(of items: [WorkItem]) -> [AssistantRequest] {
        let ids = Set(items.flatMap(\.requests))
        return requests.filter { ids.contains($0.id) }
    }

    /// `get-work`: the item, every Capture it links, and its requests.
    func detail(_ id: String, shown: [WorkItem]) throws(Refusal) -> (WorkItem, [Capture], [AssistantRequest]) {
        guard let item = shown.first(where: { $0.id == id }) ?? work.first(where: { $0.id == id }) else {
            throw Refusal(.badRequest, "no Work item \(id)")
        }
        let linked = item.captures.compactMap { c in captures.first { $0.id == c } }
        let asked = item.requests.compactMap { q in requests.first { $0.id == q } }
        return (item, linked, asked)
    }

    /// Refuses as the host does before anything is written: no daemon, or a workspace
    /// with no assistant.
    static func checkAssistant(in workspace: String) throws(Refusal) {
        if daemonOff { throw Refusal(.unsupported, noDaemon) }
        guard workspace == "work" else {
            throw Refusal(.unsupported, "workspace \"\(workspace)\" takes no assistant requests: set "
                + "[workspace.assistant] delegate = true for it in workspaces.toml on the host")
        }
    }

    /// `request-assistant`. The same `work` and `rev` return the request already made;
    /// a new `rev` adds a version to the workspace's request and ends the older run.
    mutating func ask(
        _ id: String, rev: UInt64, scope: AssistScope, workspace: String?, purpose: String?
    ) throws(Refusal) -> (WorkItem, AssistantRequest, RunChange) {
        guard let w = work.firstIndex(where: { $0.id == id }) else { throw Refusal(.badRequest, "no Work item \(id)") }
        if let made = requestsByRev["\(id) \(rev)"], let r = requests.first(where: { $0.id == made }) {
            return (work[w], r, RunChange())
        }
        guard work[w].rev == rev else { throw Refusal(.conflict, "\(id) changed since you read it") }
        guard work[w].state != .done, work[w].state != .withdrawn else { throw Refusal(.badRequest, "\(id) is \(work[w].state.raw)") }
        guard let workspace = workspace ?? work[w].workspace else { throw Refusal(.badRequest, "\(id) has no workspace") }
        guard (purpose?.utf8.count ?? 0) <= 4096 else { throw Refusal(.badRequest, "the purpose is over 4096 bytes") }
        try Self.checkAssistant(in: workspace)
        var change = RunChange()
        let r: Int
        if let i = requests.firstIndex(where: { $0.work == id && $0.workspace == workspace }) {
            if let old = runs.removeValue(forKey: requests[i].id) { change.ended.append(old) }
            requests[i].version += 1
            requests[i].scope = scope
            requests[i].purpose = purpose
            r = i
        } else {
            requests.append(AssistantRequest(
                id: String(format: "q-%08x", requests.count + 0x100), work: id, version: 1, workspace: workspace,
                scope: scope, delivery: .pending, deliveryAt: Self.now, createdAt: Self.now, updatedAt: Self.now,
                purpose: purpose))
            r = requests.count - 1
            work[w].requests.append(requests[r].id)
        }
        runCount += 1
        let run = DemoRun(
            task: String(format: "t-q%05d", runCount),
            session: "\(scope.raw)-\(requests[r].id)-v\(requests[r].version)-" + String(format: "t-q%05d", runCount),
            started: Date())
        runs[requests[r].id] = run
        change.started = run
        requests[r].run = run.task
        requests[r].delivery = .pending
        requests[r].deliveryAt = Self.now
        requests[r].updatedAt = Self.now
        work[w].rev += 1
        work[w].updatedAt = Self.now
        requestsByRev["\(id) \(rev)"] = requests[r].id
        return (work[w], requests[r], change)
    }

    /// `capture {assist}`: the Capture, then a Work item and its request. When the
    /// assistant cannot start, the Capture stays unsorted and the refusal comes with it.
    /// A resend of a sorted Capture returns its Work item and request and starts nothing.
    mutating func capture(
        clientId: String, text: String, workspace: String?, assist: AssistSpec
    ) throws(Refusal) -> (Capture, WorkItem?, AssistantRequest?, RunChange, Refusal?) {
        guard let workspace else { throw Refusal(.badRequest, "assist needs a workspace") }
        let saved = try capture(clientId: clientId, text: text, workspace: workspace)
        if saved.state == .sorted {
            let item = work.first { saved.work.contains($0.id) }
            let request = requests.first { $0.work == item?.id }
            return (saved, item, request, RunChange(), nil)
        }
        do throws(Refusal) {
            try Self.checkAssistant(in: workspace)
        } catch {
            return (saved, nil, nil, RunChange(), error)
        }
        let (sorted, made) = try sort(saved.id, rev: saved.rev, action: .work, into: nil, title: nil)
        guard let made else { throw Refusal(.internal, "the capture made no Work item") }
        let (item, request, change) = try ask(made.id, rev: made.rev, scope: assist.scope, workspace: workspace, purpose: assist.purpose)
        return (sorted, item, request, change, nil)
    }

    /// `cancel-request`: the run's session closes at once, so it is `cancelled`.
    mutating func cancel(_ id: String, version: UInt64) throws(Refusal) -> (WorkItem, AssistantRequest, RunChange) {
        guard let r = requests.firstIndex(where: { $0.id == id }),
              let w = work.firstIndex(where: { $0.id == requests[r].work })
        else { throw Refusal(.badRequest, "\(id) is not a request") }
        guard requests[r].version == version else { throw Refusal(.conflict, "\(id) is at version \(requests[r].version)") }
        guard !requests[r].results.contains(where: { $0.version == version }) else {
            throw Refusal(.badRequest, "\(id) v\(version) has its result")
        }
        var change = RunChange()
        if let old = runs.removeValue(forKey: id) {
            change.ended.append(old)
            requests[r].delivery = .cancelled
            requests[r].deliveryAt = Self.now
            requests[r].updatedAt = Self.now
            requests[r].run = nil
        }
        return (work[w], requests[r], change)
    }

    /// Moves each live run on by its age; a run past `resultAfter` stores its Result.
    /// Returns the runs that ended.
    mutating func advance(at date: Date = Date()) -> [DemoRun] {
        var ended: [DemoRun] = []
        for (id, run) in runs {
            guard let r = requests.firstIndex(where: { $0.id == id }) else { continue }
            let age = date.timeIntervalSince(run.started)
            let started = UInt64(run.started.timeIntervalSince1970)
            switch age {
            case ..<Self.receivedAfter: continue
            case ..<Self.processingAfter: setDelivery(r, .received, at: started + UInt64(Self.receivedAfter))
            case ..<Self.resultAfter: setDelivery(r, .processing, at: started + UInt64(Self.processingAfter))
            default:
                storeResult(r)
                runs[id] = nil
                ended.append(run)
            }
        }
        return ended
    }

    private mutating func setDelivery(_ r: Int, _ delivery: Delivery, at: UInt64) {
        guard requests[r].delivery != delivery else { return }
        requests[r].delivery = delivery
        requests[r].deliveryAt = at
        requests[r].updatedAt = Self.now
    }

    private mutating func storeResult(_ r: Int) {
        let request = requests[r]
        let other = work.first { $0.id != request.work && $0.workspace == request.workspace }?.id
        let stale = request.results.map { old -> AssistResult in
            var kept = old
            kept.stale = true
            return kept
        }
        let fresh = Self.result(version: request.version, at: Self.now, duplicate: other)
        requests[r].results = Array(([fresh] + stale).prefix(10))
        requests[r].run = nil
        setDelivery(r, .resulted, at: Self.now)
    }

    // MARK: Fixtures

    static func result(version: UInt64, at: UInt64, stale: Bool = false, duplicate: String?) -> AssistResult {
        AssistResult(
            version: version, at: at,
            summary: "Two ways fit. Moving the plans first is safer: invoices keep their numbers.",
            stale: stale,
            findings: [
                Finding(text: "The billing provider moves plans and customers, not open invoices.", sources: [
                    ResultSource(url: "https://example.com/docs/billing/migrate", title: "Migrating billing", readAt: at - 600),
                ]),
                Finding(text: "Our invoice numbers come from the old provider's sequence.", sources: [
                    ResultSource(path: "/work/app/billing/invoice_number.rb"),
                ]),
            ],
            openQuestions: ["Do open invoices move, or finish on the old provider?"],
            next: ["Ask finance which month to switch.", "Write the plan mapping."],
            files: ["/work/assistant/research-notes/plan-mapping.md"],
            possibleDuplicates: duplicate.map { [PossibleDuplicate(work: $0, reason: "Also changes how invoices are made.")] } ?? [])
    }

    private static func resulted(
        _ id: String, work: String, scope: AssistScope, version: UInt64, ago: UInt64, results: [AssistResult]
    ) -> AssistantRequest {
        AssistantRequest(id: id, work: work, version: version, workspace: "work", scope: scope, delivery: .resulted,
                         deliveryAt: now - ago, createdAt: now - ago - 3600, updatedAt: now - ago, results: results)
    }

    /// The billing item's research, asked twice: its first Result is stale.
    static var testsRequest: AssistantRequest {
        resulted("q-0000d001", work: "w-0000b001", scope: .research, version: 2, ago: 120, results: [
            result(version: 2, at: now - 120, duplicate: "w-0000b002"),
            AssistResult(version: 1, at: now - 3000, summary: "One way fits: move everything at once.", stale: true),
        ])
    }

    static var showcaseRequest: AssistantRequest {
        resulted("q-5c00d001", work: "w-5c00b002", scope: .compare, version: 1, ago: 900, results: [
            AssistResult(
                version: 1, at: now - 900,
                summary: "Usage pricing fits the API plan; seats fit the team plan. Two rivals moved to usage this year.",
                findings: [
                    Finding(text: "Two of three rivals price their API by usage since spring.", sources: [
                        ResultSource(url: "https://example.com/pricing", title: "Rival pricing", readAt: now - 1800),
                    ]),
                    Finding(text: "Most of Q2's growth came from teams under 10 seats.", sources: [
                        ResultSource(path: "/work/finance/q2-revenue.csv"),
                    ]),
                ],
                openQuestions: ["Do we keep the free tier?"],
                next: ["Draft the memo's pricing table."],
                files: ["/work/assistant/compare-pricing/draft.md"],
                possibleDuplicates: [PossibleDuplicate(work: "w-5c00b001", reason: "Both change what an invoice shows.")]),
        ])
    }
}
