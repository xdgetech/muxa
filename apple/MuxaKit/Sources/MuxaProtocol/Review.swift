import Foundation

// A request's Result to review (ADR 0058), at `work` level 16: every stored Result makes one
// Inbox item of kind `result`, answered like a Task's review.

/// A `result` item's Result, so the user reviews it without opening the Work item. Its
/// summary is the item's `summary`. Never logged.
public struct ResultReview: Equatable, Sendable {
    /// `q-…`.
    public var request: String
    public var version: UInt64
    /// The step of `version` that produced the Result; 1 without a staff.
    public var step: UInt32
    /// The Work item, to open its detail.
    public var work: String
    /// The staff member who wrote it. Nil: the workspace has no staff.
    public var worker: String?
    /// Each finding's first line, at most 200 characters.
    public var findings: [String]
    public var openQuestions: [String]
    public var next: [String]
    /// Paths the run wrote, absolute.
    public var files: [String]
    /// The hand-off the step proposes; `approve` starts it while it is offered.
    public var handoff: Handoff?

    public init(
        request: String, version: UInt64, step: UInt32, work: String, worker: String? = nil,
        findings: [String] = [], openQuestions: [String] = [], next: [String] = [], files: [String] = [],
        handoff: Handoff? = nil
    ) {
        self.request = request
        self.version = version
        self.step = step
        self.work = work
        self.worker = worker
        self.findings = findings
        self.openQuestions = openQuestions
        self.next = next
        self.files = files
        self.handoff = handoff
    }
}

extension ResultReview: Codable {
    private enum Key: String, CodingKey {
        case request, version, step, work, worker, findings, openQuestions, next, files, handoff
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        request = try c.decode(String.self, forKey: .request)
        version = try c.decode(UInt64.self, forKey: .version)
        step = try c.decode(UInt32.self, forKey: .step)
        work = try c.decode(String.self, forKey: .work)
        worker = try c.decodeIfPresent(String.self, forKey: .worker)
        findings = try c.decodeIfPresent([String].self, forKey: .findings) ?? []
        openQuestions = try c.decodeIfPresent([String].self, forKey: .openQuestions) ?? []
        next = try c.decodeIfPresent([String].self, forKey: .next) ?? []
        files = try c.decodeIfPresent([String].self, forKey: .files) ?? []
        handoff = try c.decodeIfPresent(Handoff.self, forKey: .handoff)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(request, forKey: .request)
        try c.encode(version, forKey: .version)
        try c.encode(step, forKey: .step)
        try c.encode(work, forKey: .work)
        try c.encodeIfPresent(worker, forKey: .worker)
        if !findings.isEmpty { try c.encode(findings, forKey: .findings) }
        if !openQuestions.isEmpty { try c.encode(openQuestions, forKey: .openQuestions) }
        if !next.isEmpty { try c.encode(next, forKey: .next) }
        if !files.isEmpty { try c.encode(files, forKey: .files) }
        try c.encodeIfPresent(handoff, forKey: .handoff)
    }
}
