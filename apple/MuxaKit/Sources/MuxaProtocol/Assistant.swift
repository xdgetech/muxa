import Foundation

// Captures, Work items and assistant requests (ADR 0048). None of them is a Task or an Inbox item.

/// The user's original input, from `capture-saved` or `inbox-list`. Its `text` is never rewritten.
public struct Capture: Equatable, Sendable, Identifiable {
    /// `c-` and 8 hex digits.
    public var id: String
    public var rev: UInt64
    /// The client's id for this capture; the host stores one Capture per value.
    public var clientId: String
    /// At most 16 KiB.
    public var text: String
    public var source: CaptureSource
    public var state: CaptureState
    /// Epoch seconds.
    public var createdAt: UInt64
    /// Epoch seconds.
    public var updatedAt: UInt64
    public var workspace: String?
    /// The Work items made from it or linked to it.
    public var work: [String]

    public init(
        id: String, rev: UInt64, clientId: String, text: String, source: CaptureSource, state: CaptureState,
        createdAt: UInt64, updatedAt: UInt64, workspace: String? = nil, work: [String] = []
    ) {
        self.id = id
        self.rev = rev
        self.clientId = clientId
        self.text = text
        self.source = source
        self.state = state
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.workspace = workspace
        self.work = work
    }
}

extension Capture: Codable {
    private enum Key: String, CodingKey {
        case id, rev, clientId, text, source, state, createdAt, updatedAt, workspace, work
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        id = try c.decode(String.self, forKey: .id)
        rev = try c.decode(UInt64.self, forKey: .rev)
        clientId = try c.decode(String.self, forKey: .clientId)
        text = try c.decode(String.self, forKey: .text)
        source = try c.decode(CaptureSource.self, forKey: .source)
        state = try c.decode(CaptureState.self, forKey: .state)
        createdAt = try c.decode(UInt64.self, forKey: .createdAt)
        updatedAt = try c.decode(UInt64.self, forKey: .updatedAt)
        workspace = try c.decodeIfPresent(String.self, forKey: .workspace)
        work = try c.decodeIfPresent([String].self, forKey: .work) ?? []
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(id, forKey: .id)
        try c.encode(rev, forKey: .rev)
        try c.encode(clientId, forKey: .clientId)
        try c.encode(text, forKey: .text)
        try c.encode(source, forKey: .source)
        try c.encode(state, forKey: .state)
        try c.encode(createdAt, forKey: .createdAt)
        try c.encode(updatedAt, forKey: .updatedAt)
        try c.encodeIfPresent(workspace, forKey: .workspace)
        if !work.isEmpty { try c.encode(work, forKey: .work) }
    }
}

/// Where a Capture was made. A source added by a newer bridge decodes as `other`.
public enum CaptureSource: Equatable, Hashable, Sendable {
    case app, tmux, cli
    case other(String)

    private static let names: [(CaptureSource, String)] = [(.app, "app"), (.tmux, "tmux"), (.cli, "cli")]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// A state added by a newer bridge decodes as `other`.
public enum CaptureState: Equatable, Hashable, Sendable {
    /// Saved, not sorted yet: listed under Unsorted.
    case received
    /// Sorted into a Work item, linked to one, or kept as a note.
    case sorted
    case other(String)

    private static let names: [(CaptureState, String)] = [(.received, "received"), (.sorted, "sorted")]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// The user's unfinished work, from `work-saved`, `capture-saved` or `inbox-list`.
/// Only the user completes or withdraws it.
public struct WorkItem: Equatable, Sendable, Identifiable {
    /// `w-` and 8 hex digits.
    public var id: String
    /// Grows by one on every write; `update-work` sends the one the client saw.
    public var rev: UInt64
    /// One line, at most 200 characters.
    public var title: String
    public var state: WorkState
    /// Epoch seconds.
    public var createdAt: UInt64
    /// Epoch seconds.
    public var updatedAt: UInt64
    /// The workspace the work belongs to, when known.
    public var workspace: String?
    /// Set by `later`: listed below the others, as Inbox items (ADR 0040).
    public var laterAt: UInt64?
    /// With `withdrawn` after `merge`: the Work item it was merged into.
    public var mergedInto: String?
    /// Level 10: set by `pin`. Pinned items are listed first, in pin order (ADR 0049).
    public var pinnedAt: UInt64?
    /// Level 10: the user's deadline. Only the user sets it.
    public var due: Due?
    public var captures: [String]
    /// Assistant requests, oldest first.
    public var requests: [String]
    /// Tasks started for it, oldest first.
    public var tasks: [String]

    public init(
        id: String, rev: UInt64, title: String, state: WorkState, createdAt: UInt64, updatedAt: UInt64,
        workspace: String? = nil, laterAt: UInt64? = nil, mergedInto: String? = nil, pinnedAt: UInt64? = nil,
        due: Due? = nil, captures: [String] = [], requests: [String] = [], tasks: [String] = []
    ) {
        self.id = id
        self.rev = rev
        self.title = title
        self.state = state
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.workspace = workspace
        self.laterAt = laterAt
        self.mergedInto = mergedInto
        self.pinnedAt = pinnedAt
        self.due = due
        self.captures = captures
        self.requests = requests
        self.tasks = tasks
    }
}

extension WorkItem: Codable {
    private enum Key: String, CodingKey {
        case id, rev, title, state, createdAt, updatedAt, workspace, laterAt, mergedInto, pinnedAt, due
        case captures, requests, tasks
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        id = try c.decode(String.self, forKey: .id)
        rev = try c.decode(UInt64.self, forKey: .rev)
        title = try c.decode(String.self, forKey: .title)
        state = try c.decode(WorkState.self, forKey: .state)
        createdAt = try c.decode(UInt64.self, forKey: .createdAt)
        updatedAt = try c.decode(UInt64.self, forKey: .updatedAt)
        workspace = try c.decodeIfPresent(String.self, forKey: .workspace)
        laterAt = try c.decodeIfPresent(UInt64.self, forKey: .laterAt)
        mergedInto = try c.decodeIfPresent(String.self, forKey: .mergedInto)
        pinnedAt = try c.decodeIfPresent(UInt64.self, forKey: .pinnedAt)
        due = try c.decodeIfPresent(Due.self, forKey: .due)
        captures = try c.decodeIfPresent([String].self, forKey: .captures) ?? []
        requests = try c.decodeIfPresent([String].self, forKey: .requests) ?? []
        tasks = try c.decodeIfPresent([String].self, forKey: .tasks) ?? []
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(id, forKey: .id)
        try c.encode(rev, forKey: .rev)
        try c.encode(title, forKey: .title)
        try c.encode(state, forKey: .state)
        try c.encode(createdAt, forKey: .createdAt)
        try c.encode(updatedAt, forKey: .updatedAt)
        try c.encodeIfPresent(workspace, forKey: .workspace)
        try c.encodeIfPresent(laterAt, forKey: .laterAt)
        try c.encodeIfPresent(mergedInto, forKey: .mergedInto)
        try c.encodeIfPresent(pinnedAt, forKey: .pinnedAt)
        try c.encodeIfPresent(due, forKey: .due)
        if !captures.isEmpty { try c.encode(captures, forKey: .captures) }
        if !requests.isEmpty { try c.encode(requests, forKey: .requests) }
        if !tasks.isEmpty { try c.encode(tasks, forKey: .tasks) }
    }
}

/// `inProgress` and `waitingDecision` are derived by the host from the linked Tasks
/// and requests. A state added by a newer bridge decodes as `other`; PROTOCOL.md
/// says to treat it as `open`.
public enum WorkState: Equatable, Hashable, Sendable {
    case open
    case inProgress
    case waitingDecision
    case done
    case withdrawn
    case other(String)

    private static let names: [(WorkState, String)] = [
        (.open, "open"), (.inProgress, "in-progress"), (.waitingDecision, "waiting-decision"),
        (.done, "done"), (.withdrawn, "withdrawn"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// One delegation of a Work item to a workspace's agent. Each `version` runs at most once.
public struct AssistantRequest: Equatable, Sendable, Identifiable {
    /// `q-` and 8 hex digits.
    public var id: String
    public var work: String
    /// Grows by one each time the user asks again.
    public var version: UInt64
    public var workspace: String
    public var scope: AssistScope
    public var delivery: Delivery
    /// Epoch seconds: when `delivery` was last derived.
    public var deliveryAt: UInt64
    /// Epoch seconds.
    public var createdAt: UInt64
    /// Epoch seconds.
    public var updatedAt: UInt64
    /// What the user asked for, at most 4096 bytes.
    public var purpose: String?
    /// The current version's run record, `t-…` of kind `assistant-run`.
    public var run: String?
    /// Newest first, at most 10.
    public var results: [AssistResult]
    /// Level 12: the current version's chain, step 1 first (ADR 0051 §3). `delivery`
    /// and `run` are the newest step's.
    public var steps: [RequestStep]

    public init(
        id: String, work: String, version: UInt64, workspace: String, scope: AssistScope, delivery: Delivery,
        deliveryAt: UInt64, createdAt: UInt64, updatedAt: UInt64, purpose: String? = nil, run: String? = nil,
        results: [AssistResult] = [], steps: [RequestStep] = []
    ) {
        self.id = id
        self.work = work
        self.version = version
        self.workspace = workspace
        self.scope = scope
        self.delivery = delivery
        self.deliveryAt = deliveryAt
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.purpose = purpose
        self.run = run
        self.results = results
        self.steps = steps
    }
}

extension AssistantRequest: Codable {
    private enum Key: String, CodingKey {
        case id, work, version, workspace, scope, delivery, deliveryAt, createdAt, updatedAt, purpose, run, results
        case steps
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        id = try c.decode(String.self, forKey: .id)
        work = try c.decode(String.self, forKey: .work)
        version = try c.decode(UInt64.self, forKey: .version)
        workspace = try c.decode(String.self, forKey: .workspace)
        scope = try c.decode(AssistScope.self, forKey: .scope)
        delivery = try c.decode(Delivery.self, forKey: .delivery)
        deliveryAt = try c.decode(UInt64.self, forKey: .deliveryAt)
        createdAt = try c.decode(UInt64.self, forKey: .createdAt)
        updatedAt = try c.decode(UInt64.self, forKey: .updatedAt)
        purpose = try c.decodeIfPresent(String.self, forKey: .purpose)
        run = try c.decodeIfPresent(String.self, forKey: .run)
        results = try c.decodeIfPresent([AssistResult].self, forKey: .results) ?? []
        steps = try c.decodeIfPresent([RequestStep].self, forKey: .steps) ?? []
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(id, forKey: .id)
        try c.encode(work, forKey: .work)
        try c.encode(version, forKey: .version)
        try c.encode(workspace, forKey: .workspace)
        try c.encode(scope, forKey: .scope)
        try c.encode(delivery, forKey: .delivery)
        try c.encode(deliveryAt, forKey: .deliveryAt)
        try c.encode(createdAt, forKey: .createdAt)
        try c.encode(updatedAt, forKey: .updatedAt)
        try c.encodeIfPresent(purpose, forKey: .purpose)
        try c.encodeIfPresent(run, forKey: .run)
        if !results.isEmpty { try c.encode(results, forKey: .results) }
        if !steps.isEmpty { try c.encode(steps, forKey: .steps) }
    }
}

/// What the assistant is asked to do. A scope added by a newer bridge decodes as `other`.
public enum AssistScope: Equatable, Hashable, Sendable {
    case research, compare, draft
    case other(String)

    private static let names: [(AssistScope, String)] = [
        (.research, "research"), (.compare, "compare"), (.draft, "draft"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// Derived from the run record and the adapter, never from the agent's words. A value
/// added by a newer bridge decodes as `other`: show it as unknown.
public enum Delivery: Equatable, Hashable, Sendable {
    /// The run exists; the prompt is not typed yet.
    case pending
    /// The prompt was typed and confirmed.
    case received
    /// The adapter reports the agent busy.
    case processing
    /// A send was interrupted. Never retyped automatically.
    case unconfirmed
    case resulted
    case failed
    /// No result in the workspace's `run_timeout`. The agent is not stopped.
    case timedOut
    case cancelRequested
    case cancelled
    case other(String)

    private static let names: [(Delivery, String)] = [
        (.pending, "pending"), (.received, "received"), (.processing, "processing"),
        (.unconfirmed, "unconfirmed"), (.resulted, "resulted"), (.failed, "failed"), (.timedOut, "timed-out"),
        (.cancelRequested, "cancel-requested"), (.cancelled, "cancelled"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

extension CaptureSource: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension CaptureState: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension WorkState: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension AssistScope: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension Delivery: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

/// What a run returned, after the host's check. A result never changes the Work item's state.
public struct AssistResult: Equatable, Sendable {
    /// The request version that produced it.
    public var version: UInt64
    /// Epoch seconds: when the host stored it.
    public var at: UInt64
    /// At most 4096 bytes.
    public var summary: String
    /// An older version's result.
    public var stale: Bool
    public var findings: [Finding]
    public var openQuestions: [String]
    public var next: [String]
    /// Paths the run wrote, absolute.
    public var files: [String]
    public var possibleDuplicates: [PossibleDuplicate]
    /// Level 10: the delegate's proposed notes (ADR 0049 §1).
    public var notes: Notes?
    /// Level 12: the staff member whose step produced it.
    public var worker: String?
    /// Level 12: the step of `version` that produced it. Nil: step 1.
    public var step: UInt32?
    /// Level 13: writes outside the host it proposes, at most 3 (ADR 0052).
    public var writes: [ResultWrite]

    public init(
        version: UInt64, at: UInt64, summary: String, stale: Bool = false, findings: [Finding] = [],
        openQuestions: [String] = [], next: [String] = [], files: [String] = [],
        possibleDuplicates: [PossibleDuplicate] = [], notes: Notes? = nil, worker: String? = nil, step: UInt32? = nil,
        writes: [ResultWrite] = []
    ) {
        self.version = version
        self.at = at
        self.summary = summary
        self.stale = stale
        self.findings = findings
        self.openQuestions = openQuestions
        self.next = next
        self.files = files
        self.possibleDuplicates = possibleDuplicates
        self.notes = notes
        self.worker = worker
        self.step = step
        self.writes = writes
    }
}

extension AssistResult: Codable {
    private enum Key: String, CodingKey {
        case version, at, summary, stale, findings, openQuestions, next, files, possibleDuplicates, notes, worker, step
        case writes
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        version = try c.decode(UInt64.self, forKey: .version)
        at = try c.decode(UInt64.self, forKey: .at)
        summary = try c.decode(String.self, forKey: .summary)
        stale = try c.decodeIfPresent(Bool.self, forKey: .stale) ?? false
        findings = try c.decodeIfPresent([Finding].self, forKey: .findings) ?? []
        openQuestions = try c.decodeIfPresent([String].self, forKey: .openQuestions) ?? []
        next = try c.decodeIfPresent([String].self, forKey: .next) ?? []
        files = try c.decodeIfPresent([String].self, forKey: .files) ?? []
        possibleDuplicates = try c.decodeIfPresent([PossibleDuplicate].self, forKey: .possibleDuplicates) ?? []
        notes = try c.decodeIfPresent(Notes.self, forKey: .notes)
        worker = try c.decodeIfPresent(String.self, forKey: .worker)
        step = try c.decodeIfPresent(UInt32.self, forKey: .step)
        writes = try c.decodeIfPresent([ResultWrite].self, forKey: .writes) ?? []
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(version, forKey: .version)
        try c.encode(at, forKey: .at)
        try c.encode(summary, forKey: .summary)
        if stale { try c.encode(true, forKey: .stale) }
        if !findings.isEmpty { try c.encode(findings, forKey: .findings) }
        if !openQuestions.isEmpty { try c.encode(openQuestions, forKey: .openQuestions) }
        if !next.isEmpty { try c.encode(next, forKey: .next) }
        if !files.isEmpty { try c.encode(files, forKey: .files) }
        if !possibleDuplicates.isEmpty { try c.encode(possibleDuplicates, forKey: .possibleDuplicates) }
        try c.encodeIfPresent(notes, forKey: .notes)
        try c.encodeIfPresent(worker, forKey: .worker)
        try c.encodeIfPresent(step, forKey: .step)
        if !writes.isEmpty { try c.encode(writes, forKey: .writes) }
    }
}

/// A deadline the user set on a Work item (ADR 0049 §6).
public struct Due: Codable, Equatable, Sendable {
    /// Epoch seconds.
    public var at: UInt64
    /// IANA name, e.g. `Asia/Tokyo`: the zone the user set it in.
    public var timezone: String

    public init(at: UInt64, timezone: String) {
        self.at = at
        self.timezone = timezone
    }
}

/// A run's proposed replacement of the workspace's `.muxa/assistant/NOTES.md`.
/// `apply-notes` writes it only while the live file still hashes to `base`.
public struct Notes: Codable, Equatable, Sendable {
    /// The full new text, at most 16 KiB.
    public var text: String
    /// Lowercase hex SHA-256 of the file the run read; of no bytes when there was none.
    public var base: String

    public init(text: String, base: String) {
        self.text = text
        self.base = base
    }
}

public struct Finding: Codable, Equatable, Sendable {
    public var text: String
    /// At least one.
    public var sources: [ResultSource]

    public init(text: String, sources: [ResultSource]) {
        self.text = text
        self.sources = sources
    }
}

/// A URL or a path, with when the run read it.
public struct ResultSource: Codable, Equatable, Sendable {
    public var url: String?
    public var path: String?
    public var title: String?
    /// Epoch seconds.
    public var readAt: UInt64?

    public init(url: String? = nil, path: String? = nil, title: String? = nil, readAt: UInt64? = nil) {
        self.url = url
        self.path = path
        self.title = title
        self.readAt = readAt
    }
}

public struct PossibleDuplicate: Codable, Equatable, Sendable {
    /// The other Work item.
    public var work: String
    public var reason: String

    public init(work: String, reason: String) {
        self.work = work
        self.reason = reason
    }
}

/// `capture {assist}`: also make a Work item and ask the assistant right away.
public struct AssistSpec: Codable, Equatable, Sendable {
    public var scope: AssistScope
    public var purpose: String?
    /// Level 12: a staff member of the workspace. Nil: its lead.
    public var worker: String?

    public init(scope: AssistScope, purpose: String? = nil, worker: String? = nil) {
        self.scope = scope
        self.purpose = purpose
        self.worker = worker
    }
}

/// How `sort-capture` sorts a Capture.
public enum SortAction: String, Codable, Sendable {
    /// Make a new Work item from the Capture.
    case work
    /// Link the Capture to an existing Work item.
    case link
    /// Keep the Capture as a note; no Work item.
    case note
}

/// What `update-work` does.
public enum WorkAction: String, Codable, Sendable {
    /// Change `title`, `workspace` or, at level 10, `due`.
    case edit
    case later, complete, withdraw, reopen
    /// Withdraw this item into `into`; its Captures and requests move there.
    case merge
    /// Level 10: list it first, after the items pinned before it.
    case pin
    /// Level 10: clear `pinnedAt`.
    case unpin
}
