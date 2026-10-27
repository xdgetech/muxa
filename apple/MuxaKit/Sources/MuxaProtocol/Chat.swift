import Foundation

/// One item of an agent conversation, the same for every agent.
public struct ChatEntry: Codable, Equatable, Identifiable, Sendable {
    public enum Kind: String, Codable, Sendable {
        case user, assistant, tool, notice
    }

    /// Stable within a chat; a later entry with the same id replaces this one.
    public var id: String
    public var kind: Kind
    /// The prompt, the reply (markdown), or the notice. Empty for a tool call.
    public var text: String
    public var tool: ChatTool?

    public init(id: String, kind: Kind, text: String = "", tool: ChatTool? = nil) {
        self.id = id
        self.kind = kind
        self.text = text
        self.tool = tool
    }

    private enum Key: String, CodingKey { case id, kind, text, tool }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        id = try c.decode(String.self, forKey: .id)
        kind = try c.decode(Kind.self, forKey: .kind)
        text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
        tool = try c.decodeIfPresent(ChatTool.self, forKey: .tool)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(id, forKey: .id)
        try c.encode(kind, forKey: .kind)
        if !text.isEmpty { try c.encode(text, forKey: .text) }
        try c.encodeIfPresent(tool, forKey: .tool)
    }
}

public struct ChatTool: Codable, Equatable, Sendable {
    public var name: String
    /// One line about the call, e.g. the command or the file.
    public var summary: String
    /// The start of the result, once it arrived.
    public var result: String?
    public var error: Bool

    public init(name: String, summary: String, result: String? = nil, error: Bool = false) {
        self.name = name
        self.summary = summary
        self.result = result
        self.error = error
    }

    private enum Key: String, CodingKey { case name, summary, result, error }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        name = try c.decode(String.self, forKey: .name)
        summary = try c.decode(String.self, forKey: .summary)
        result = try c.decodeIfPresent(String.self, forKey: .result)
        error = try c.decodeIfPresent(Bool.self, forKey: .error) ?? false
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(name, forKey: .name)
        try c.encode(summary, forKey: .summary)
        try c.encodeIfPresent(result, forKey: .result)
        if error { try c.encode(true, forKey: .error) }
    }
}

/// The body of a `chat-entries` message.
public struct ChatUpdate: Codable, Equatable, Sendable {
    /// The id of the `chat` request this belongs to.
    public var chat: UInt64
    /// Grows by one per message of a chat.
    public var seq: UInt64
    /// Clear the list before applying `entries`.
    public var reset: Bool
    public var entries: [ChatEntry]
    /// The agent's state now; nil when unknown.
    public var status: Agent?
    /// On a reset: the adapter that reads this agent, e.g. `claude-code`.
    public var agent: String?
    /// On a reset: earlier history exists but was not sent.
    public var truncated: Bool
    /// What the agent waits on now, with its choices; nil when none is known.
    public var prompt: ChatPrompt?

    public init(
        chat: UInt64, seq: UInt64, reset: Bool = false, entries: [ChatEntry], status: Agent? = nil,
        agent: String? = nil, truncated: Bool = false, prompt: ChatPrompt? = nil
    ) {
        self.chat = chat
        self.seq = seq
        self.reset = reset
        self.entries = entries
        self.status = status
        self.agent = agent
        self.truncated = truncated
        self.prompt = prompt
    }

    private enum Key: String, CodingKey { case chat, seq, reset, entries, status, agent, truncated, prompt }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        chat = try c.decode(UInt64.self, forKey: .chat)
        seq = try c.decode(UInt64.self, forKey: .seq)
        reset = try c.decodeIfPresent(Bool.self, forKey: .reset) ?? false
        entries = try c.decode([ChatEntry].self, forKey: .entries)
        status = try c.decodeIfPresent(Agent.self, forKey: .status)
        agent = try c.decodeIfPresent(String.self, forKey: .agent)
        truncated = try c.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
        prompt = try c.decodeIfPresent(ChatPrompt.self, forKey: .prompt)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(chat, forKey: .chat)
        try c.encode(seq, forKey: .seq)
        if reset { try c.encode(true, forKey: .reset) }
        try c.encode(entries, forKey: .entries)
        try c.encodeIfPresent(status, forKey: .status)
        try c.encodeIfPresent(agent, forKey: .agent)
        if truncated { try c.encode(true, forKey: .truncated) }
        try c.encodeIfPresent(prompt, forKey: .prompt)
    }
}

/// A choice the agent waits on. Each option carries the keys that pick it in the
/// agent's menu, so the client needs no agent knowledge.
public struct ChatPrompt: Codable, Equatable, Sendable {
    /// Unknown kinds decode as `other`, so a newer bridge does not break the message.
    public enum Kind: Equatable, Sendable {
        case permission, question, plan
        case other(String)

        init(raw: String) {
            switch raw {
            case "permission": self = .permission
            case "question": self = .question
            case "plan": self = .plan
            default: self = .other(raw)
            }
        }

        var raw: String {
            switch self {
            case .permission: "permission"
            case .question: "question"
            case .plan: "plan"
            case .other(let raw): raw
            }
        }
    }

    public struct Option: Codable, Equatable, Sendable {
        public var label: String
        /// tmux key names, as in a `keys` request.
        public var keys: [String]
        /// Declines or cancels, e.g. a deny.
        public var cancel: Bool

        public init(label: String, keys: [String], cancel: Bool = false) {
            self.label = label
            self.keys = keys
            self.cancel = cancel
        }

        private enum Key: String, CodingKey { case label, keys, cancel }

        public init(from decoder: any Decoder) throws {
            let c = try decoder.container(keyedBy: Key.self)
            label = try c.decode(String.self, forKey: .label)
            keys = try c.decode([String].self, forKey: .keys)
            cancel = try c.decodeIfPresent(Bool.self, forKey: .cancel) ?? false
        }

        public func encode(to encoder: any Encoder) throws {
            var c = encoder.container(keyedBy: Key.self)
            try c.encode(label, forKey: .label)
            try c.encode(keys, forKey: .keys)
            if cancel { try c.encode(true, forKey: .cancel) }
        }
    }

    /// The id of the tool entry the agent waits on.
    public var entry: String
    public var kind: Kind
    /// For a question: the question as the agent asks it.
    public var question: String?
    public var options: [Option]

    public init(entry: String, kind: Kind, question: String? = nil, options: [Option]) {
        self.entry = entry
        self.kind = kind
        self.question = question
        self.options = options
    }

    private enum Key: String, CodingKey { case entry, kind, question, options }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        entry = try c.decode(String.self, forKey: .entry)
        kind = Kind(raw: try c.decode(String.self, forKey: .kind))
        question = try c.decodeIfPresent(String.self, forKey: .question)
        options = try c.decode([Option].self, forKey: .options)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(entry, forKey: .entry)
        try c.encode(kind.raw, forKey: .kind)
        try c.encodeIfPresent(question, forKey: .question)
        try c.encode(options, forKey: .options)
    }
}
