import Foundation

/// A request's intent: missing is Keep, null is Workspace default.
public enum WorkerSelection: Equatable, Sendable, Codable {
    case keep, workspaceDefault, named(String)

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        self = c.decodeNil() ? .workspaceDefault : .named(try c.decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .keep:
            throw EncodingError.invalidValue(self, .init(codingPath: encoder.codingPath,
                debugDescription: "Keep must be omitted by the enclosing request"))
        case .workspaceDefault: try c.encodeNil()
        case .named(let name): try c.encode(name)
        }
    }
}

/// Run history records the requested selection, independently of observed agent state.
public struct WorkerAssignment: Codable, Equatable, Sendable {
    public var kind: String
    public var name: String?
    public init(kind: String, name: String? = nil) {
        self.kind = kind
        self.name = name
    }
    public var isKnown: Bool {
        switch kind {
        case "workspace-default": name == nil
        case "worker": name?.isEmpty == false
        default: false
        }
    }
}

/// Host-owned public metadata; never native args, instruction content or env.
public struct WorkerInfo: Codable, Equatable, Sendable {
    public var name: String
    public var agent: String
    public var description: String?
    public var workspaces: [String]?
    public var loop: LoopSpec?
    public init(name: String, agent: String, description: String? = nil,
                workspaces: [String]? = nil, loop: LoopSpec? = nil) {
        self.name = name
        self.agent = agent
        self.description = description
        self.workspaces = workspaces
        self.loop = loop
    }
}
