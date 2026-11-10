import Foundation

/// Level 3: Inbox items (`items` in `inbox-list`, the answers of ADR 0033).
public let workItems: UInt32 = 3

/// One stored question for the user (ADR 0033), from `inbox-list` or `inbox-saved`.
public struct InboxItem: Equatable, Sendable, Identifiable {
    /// `i-` and 8 hex digits for a Task's item; a Routine's entry keeps its `e-` id.
    public var id: String
    public var kind: ItemKind
    public var state: ItemState
    /// One line: what the user must decide.
    public var question: String
    public var workspace: String
    /// The Task's title, or the entry's title for a Routine's item.
    public var title: String
    /// Epoch seconds.
    public var createdAt: UInt64
    /// Set by the `read` answer. Nil: unread.
    public var readAt: UInt64?
    /// Set by `later`: the host lists the item below the others (ADR 0040).
    public var laterAt: UInt64?
    /// Only from a host before ADR 0040, with `later`: when the item is open again.
    public var until: UInt64?
    /// Set by `protect`: `delete-inbox` keeps the item (ADR 0046).
    public var protectedAt: UInt64?
    public var answeredAt: UInt64?
    /// With `answered`: the answer given. Nil also when a newer host sent an action this client does not know.
    public var answer: InboxAction?
    /// The agent's last message for the turn, or a Routine entry's body.
    public var summary: String?
    public var task: String?
    /// `$n` of the Task's session when the item was made.
    public var session: String?
    /// `%n` of the agent pane.
    public var pane: String?
    /// With kind `ask`: the menu the agent waits on.
    public var prompt: ChatPrompt?
    public var attachments: Attachments
    public var routine: String?
    public var proposal: Proposal?
    /// With kind `sort`: what approve does (`work` level 10, ADR 0049).
    public var sort: SortProposal?
    /// With kind `delegate`: what approve does.
    public var delegate: DelegateProposal?
    /// With kind `hypothesis`: what confirm makes a policy of (`work` level 11, ADR 0050).
    public var hypothesis: HypothesisProposal?
    /// With kind `handoff`: the step approve starts (`work` level 12, ADR 0051).
    public var handoff: HandoffProposal?
    /// With kind `write`: the exact write approve hands the writer (`work` level 13,
    /// ADR 0052). After approve, `task` is the writer's Task.
    public var write: WriteProposal?
    /// With kind `memory`: the text approve appends to the workspace's WORKSPACE.md
    /// (`work` level 14, ADR 0053). `task` is the same Task.
    public var memory: MemoryProposal?
    /// The answers the host accepts now, in the order to show them. Unknown
    /// actions are left out: PROTOCOL.md says to show them as nothing.
    public var answers: [InboxAction]

    public init(
        id: String, kind: ItemKind, state: ItemState, question: String, workspace: String, title: String,
        createdAt: UInt64, readAt: UInt64? = nil, laterAt: UInt64? = nil, until: UInt64? = nil,
        protectedAt: UInt64? = nil, answeredAt: UInt64? = nil, answer: InboxAction? = nil, summary: String? = nil, task: String? = nil, session: String? = nil,
        pane: String? = nil, prompt: ChatPrompt? = nil, attachments: Attachments = Attachments(),
        routine: String? = nil, proposal: Proposal? = nil, sort: SortProposal? = nil,
        delegate: DelegateProposal? = nil, hypothesis: HypothesisProposal? = nil, handoff: HandoffProposal? = nil,
        write: WriteProposal? = nil, memory: MemoryProposal? = nil,
        answers: [InboxAction] = []
    ) {
        self.id = id
        self.kind = kind
        self.state = state
        self.question = question
        self.workspace = workspace
        self.title = title
        self.createdAt = createdAt
        self.readAt = readAt
        self.laterAt = laterAt
        self.until = until
        self.protectedAt = protectedAt
        self.answeredAt = answeredAt
        self.answer = answer
        self.summary = summary
        self.task = task
        self.session = session
        self.pane = pane
        self.prompt = prompt
        self.attachments = attachments
        self.routine = routine
        self.proposal = proposal
        self.sort = sort
        self.delegate = delegate
        self.hypothesis = hypothesis
        self.handoff = handoff
        self.write = write
        self.memory = memory
        self.answers = answers
    }
}

extension InboxItem: Codable {
    private enum Key: String, CodingKey {
        case id, kind, state, question, workspace, title, createdAt, readAt, laterAt, until, protectedAt, answeredAt, answer
        case summary, task, session, pane, prompt, attachments, routine, proposal, sort, delegate, hypothesis, handoff, write, memory, answers
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        id = try c.decode(String.self, forKey: .id)
        kind = try c.decode(ItemKind.self, forKey: .kind)
        state = try c.decode(ItemState.self, forKey: .state)
        question = try c.decode(String.self, forKey: .question)
        workspace = try c.decode(String.self, forKey: .workspace)
        title = try c.decode(String.self, forKey: .title)
        createdAt = try c.decode(UInt64.self, forKey: .createdAt)
        readAt = try c.decodeIfPresent(UInt64.self, forKey: .readAt)
        laterAt = try c.decodeIfPresent(UInt64.self, forKey: .laterAt)
        until = try c.decodeIfPresent(UInt64.self, forKey: .until)
        protectedAt = try c.decodeIfPresent(UInt64.self, forKey: .protectedAt)
        answeredAt = try c.decodeIfPresent(UInt64.self, forKey: .answeredAt)
        answer = try c.decodeIfPresent(String.self, forKey: .answer).flatMap(InboxAction.init(rawValue:))
        summary = try c.decodeIfPresent(String.self, forKey: .summary)
        task = try c.decodeIfPresent(String.self, forKey: .task)
        session = try c.decodeIfPresent(String.self, forKey: .session)
        pane = try c.decodeIfPresent(String.self, forKey: .pane)
        prompt = try c.decodeIfPresent(ChatPrompt.self, forKey: .prompt)
        attachments = try c.decodeIfPresent(Attachments.self, forKey: .attachments) ?? Attachments()
        routine = try c.decodeIfPresent(String.self, forKey: .routine)
        proposal = try c.decodeIfPresent(Proposal.self, forKey: .proposal)
        sort = try c.decodeIfPresent(SortProposal.self, forKey: .sort)
        delegate = try c.decodeIfPresent(DelegateProposal.self, forKey: .delegate)
        hypothesis = try c.decodeIfPresent(HypothesisProposal.self, forKey: .hypothesis)
        handoff = try c.decodeIfPresent(HandoffProposal.self, forKey: .handoff)
        write = try c.decodeIfPresent(WriteProposal.self, forKey: .write)
        memory = try c.decodeIfPresent(MemoryProposal.self, forKey: .memory)
        answers = (try c.decodeIfPresent([String].self, forKey: .answers) ?? []).compactMap(InboxAction.init(rawValue:))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(id, forKey: .id)
        try c.encode(kind, forKey: .kind)
        try c.encode(state, forKey: .state)
        try c.encode(question, forKey: .question)
        try c.encode(workspace, forKey: .workspace)
        try c.encode(title, forKey: .title)
        try c.encode(createdAt, forKey: .createdAt)
        try c.encodeIfPresent(readAt, forKey: .readAt)
        try c.encodeIfPresent(laterAt, forKey: .laterAt)
        try c.encodeIfPresent(until, forKey: .until)
        try c.encodeIfPresent(protectedAt, forKey: .protectedAt)
        try c.encodeIfPresent(answeredAt, forKey: .answeredAt)
        try c.encodeIfPresent(answer, forKey: .answer)
        try c.encodeIfPresent(summary, forKey: .summary)
        try c.encodeIfPresent(task, forKey: .task)
        try c.encodeIfPresent(session, forKey: .session)
        try c.encodeIfPresent(pane, forKey: .pane)
        try c.encodeIfPresent(prompt, forKey: .prompt)
        if !attachments.isEmpty { try c.encode(attachments, forKey: .attachments) }
        try c.encodeIfPresent(routine, forKey: .routine)
        try c.encodeIfPresent(proposal, forKey: .proposal)
        try c.encodeIfPresent(sort, forKey: .sort)
        try c.encodeIfPresent(delegate, forKey: .delegate)
        try c.encodeIfPresent(hypothesis, forKey: .hypothesis)
        try c.encodeIfPresent(handoff, forKey: .handoff)
        try c.encodeIfPresent(write, forKey: .write)
        try c.encodeIfPresent(memory, forKey: .memory)
        if !answers.isEmpty { try c.encode(answers, forKey: .answers) }
    }
}

/// What the item asks about. A kind added by a newer bridge decodes as `other`.
public enum ItemKind: Equatable, Hashable, Sendable {
    /// The agent waits on a permission, a question or a plan.
    case ask
    /// The agent finished a turn, or its loop passed.
    case review
    /// The agent stopped with an error, or its loop stopped.
    case failure
    case proposal, briefing
    /// Level 10: the overall assistant proposes to sort a Capture.
    case sort
    /// Level 10: the overall assistant proposes to ask a workspace's delegate.
    case delegate
    /// Level 11: the overall assistant proposes a hypothesis about the user.
    case hypothesis
    /// Level 12: a staff member asks to pass a request on to another.
    case handoff
    /// Level 13: a Result proposes a write outside the host.
    case write
    /// Level 14: a Task's end proposes to add to the workspace memory.
    case memory
    case other(String)

    private static let names: [(ItemKind, String)] = [
        (.ask, "ask"), (.review, "review"), (.failure, "failure"), (.proposal, "proposal"), (.briefing, "briefing"),
        (.sort, "sort"), (.delegate, "delegate"), (.hypothesis, "hypothesis"),
        (.handoff, "handoff"), (.write, "write"), (.memory, "memory"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

/// A state added by a newer bridge decodes as `other`.
public enum ItemState: Equatable, Hashable, Sendable {
    case open, later, answered
    /// The need ended without an answer from the Inbox, e.g. the user answered in the terminal.
    case resolved
    case other(String)

    private static let names: [(ItemState, String)] = [
        (.open, "open"), (.later, "later"), (.answered, "answered"), (.resolved, "resolved"),
    ]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

extension ItemKind: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

extension ItemState: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}

/// An item's evidence. The client opens each through the pane's `changes`,
/// `docs` and `images` requests, or as a link.
public struct Attachments: Equatable, Sendable {
    /// The pane's changes when the item was made.
    public var changes: ChangeCount?
    /// Markdown paths among those changes, as `doc` takes them.
    public var docs: [String]
    /// Image paths among those changes, as `image` takes them.
    public var images: [String]
    /// The PR, or a Routine entry's links.
    public var links: [SourceLink]

    public init(changes: ChangeCount? = nil, docs: [String] = [], images: [String] = [], links: [SourceLink] = []) {
        self.changes = changes
        self.docs = docs
        self.images = images
        self.links = links
    }

    public var isEmpty: Bool { changes == nil && docs.isEmpty && images.isEmpty && links.isEmpty }
}

extension Attachments: Codable {
    private enum Key: String, CodingKey { case changes, docs, images, links }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        changes = try c.decodeIfPresent(ChangeCount.self, forKey: .changes)
        docs = try c.decodeIfPresent([String].self, forKey: .docs) ?? []
        images = try c.decodeIfPresent([String].self, forKey: .images) ?? []
        links = try c.decodeIfPresent([SourceLink].self, forKey: .links) ?? []
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encodeIfPresent(changes, forKey: .changes)
        if !docs.isEmpty { try c.encode(docs, forKey: .docs) }
        if !images.isEmpty { try c.encode(images, forKey: .images) }
        if !links.isEmpty { try c.encode(links, forKey: .links) }
    }
}

public struct ChangeCount: Codable, Equatable, Sendable {
    public var files: UInt32
    public var added: UInt32
    public var removed: UInt32
    /// `branch` when the counts are the branch's own commits, as in `diff {scope}`.
    public var scope: String?

    public init(files: UInt32, added: UInt32, removed: UInt32, scope: String? = nil) {
        self.files = files
        self.added = added
        self.removed = removed
        self.scope = scope
    }
}
