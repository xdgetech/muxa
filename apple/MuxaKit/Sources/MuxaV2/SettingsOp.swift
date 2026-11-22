import Foundation

/// A field `workspaceUpdate` may keep (absent on the wire), clear (`null`) or set.
public enum FieldChange<Value: Codable & Equatable & Sendable>: Equatable, Sendable {
    case keep
    case clear
    case set(Value)
}

/// One change of `settings-update` (ADR 0074). Names are never renamed: to
/// rename, remove and add. A `secret` and an env `value` go only to the host;
/// `description`, `debugDescription` and the mirror never show them.
public enum SettingsOp: Equatable, Sendable {
    case workspaceAdd(name: String, root: String, repos: [String] = [])
    case workspaceUpdate(workspace: String, root: String? = nil, repos: [String]? = nil,
                         agentCommand: FieldChange<String> = .keep, parkAfter: FieldChange<String> = .keep)
    /// Also removes the Keychain secrets of its connections; its folder stays.
    case workspaceRemove(workspace: String)
    case envSet(workspace: String, key: String, value: String)
    case envUnset(workspace: String, key: String)
    /// Nil: no Task Loops.
    case loopChecks(workspace: String, checks: LoopChecks?)
    case routinesGate(workspace: String, enabled: Bool, directWrites: Bool = false)
    case connectionAdd(workspace: String, connection: ConnectionSpec, secret: String? = nil)
    /// Changes the connection named `connection.name`.
    case connectionUpdate(workspace: String, connection: ConnectionSpec, secret: String? = nil)
    case connectionSecret(workspace: String, name: String, secret: String)
    case connectionRemove(workspace: String, name: String)
    case mcpAdd(workspace: String, mcp: McpSpec)
    /// Replaces the server named `mcp.name`.
    case mcpUpdate(workspace: String, mcp: McpSpec)
    case mcpRemove(workspace: String, name: String)
    /// `instructionsText` replaces the instruction file's content.
    case routineAdd(workspace: String, routine: RoutineSpec, instructionsText: String? = nil)
    case routineUpdate(workspace: String, routine: RoutineSpec, instructionsText: String? = nil)
    /// Its instruction file stays.
    case routineRemove(workspace: String, name: String)
    case workerAdd(worker: WorkerSpec)
    /// Replaces the worker named `worker.name`.
    case workerUpdate(worker: WorkerSpec)
    case workerRemove(name: String)

    /// The `op` on the wire.
    public var name: String {
        switch self {
        case .workspaceAdd: "workspace-add"
        case .workspaceUpdate: "workspace-update"
        case .workspaceRemove: "workspace-remove"
        case .envSet: "env-set"
        case .envUnset: "env-unset"
        case .loopChecks: "loop-checks"
        case .routinesGate: "routines-gate"
        case .connectionAdd: "connection-add"
        case .connectionUpdate: "connection-update"
        case .connectionSecret: "connection-secret"
        case .connectionRemove: "connection-remove"
        case .mcpAdd: "mcp-add"
        case .mcpUpdate: "mcp-update"
        case .mcpRemove: "mcp-remove"
        case .routineAdd: "routine-add"
        case .routineUpdate: "routine-update"
        case .routineRemove: "routine-remove"
        case .workerAdd: "worker-add"
        case .workerUpdate: "worker-update"
        case .workerRemove: "worker-remove"
        }
    }

    /// The op as a JSON object, `null` included where it clears.
    func jsonObject() throws -> Any {
        try JSONSerialization.jsonObject(with: Coding.plainEncoder.encode(self))
    }
}

extension SettingsOp: Codable {
    enum CodingKeys: String, CodingKey {
        case op, name, root, repos, workspace, key, value, checks, enabled, connection, secret, mcp, routine, worker
        case agentCommand = "agent_command"
        case parkAfter = "park_after"
        case directWrites = "direct_writes"
        case instructionsText = "instructions_text"
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .op)
        switch self {
        case let .workspaceAdd(name, root, repos):
            try c.encode(name, forKey: .name)
            try c.encode(root, forKey: .root)
            try c.encode(repos, forKey: .repos)
        case let .workspaceUpdate(workspace, root, repos, agentCommand, parkAfter):
            try c.encode(workspace, forKey: .workspace)
            try c.encodeIfPresent(root, forKey: .root)
            try c.encodeIfPresent(repos, forKey: .repos)
            try c.encode(agentCommand, forKey: .agentCommand)
            try c.encode(parkAfter, forKey: .parkAfter)
        case let .workspaceRemove(workspace):
            try c.encode(workspace, forKey: .workspace)
        case let .envSet(workspace, key, value):
            try c.encode(workspace, forKey: .workspace)
            try c.encode(key, forKey: .key)
            try c.encode(value, forKey: .value)
        case let .envUnset(workspace, key):
            try c.encode(workspace, forKey: .workspace)
            try c.encode(key, forKey: .key)
        case let .loopChecks(workspace, checks):
            try c.encode(workspace, forKey: .workspace)
            if let checks { try c.encode(checks, forKey: .checks) } else { try c.encodeNil(forKey: .checks) }
        case let .routinesGate(workspace, enabled, directWrites):
            try c.encode(workspace, forKey: .workspace)
            try c.encode(enabled, forKey: .enabled)
            try c.encode(directWrites, forKey: .directWrites)
        case let .connectionAdd(workspace, connection, secret), let .connectionUpdate(workspace, connection, secret):
            try c.encode(workspace, forKey: .workspace)
            try c.encode(connection, forKey: .connection)
            try c.encodeIfPresent(secret, forKey: .secret)
        case let .connectionSecret(workspace, name, secret):
            try c.encode(workspace, forKey: .workspace)
            try c.encode(name, forKey: .name)
            try c.encode(secret, forKey: .secret)
        case let .connectionRemove(workspace, name), let .mcpRemove(workspace, name), let .routineRemove(workspace, name):
            try c.encode(workspace, forKey: .workspace)
            try c.encode(name, forKey: .name)
        case let .mcpAdd(workspace, mcp), let .mcpUpdate(workspace, mcp):
            try c.encode(workspace, forKey: .workspace)
            try c.encode(mcp, forKey: .mcp)
        case let .routineAdd(workspace, routine, text), let .routineUpdate(workspace, routine, text):
            try c.encode(workspace, forKey: .workspace)
            try c.encode(routine, forKey: .routine)
            try c.encodeIfPresent(text, forKey: .instructionsText)
        case let .workerAdd(worker), let .workerUpdate(worker):
            try c.encode(worker, forKey: .worker)
        case let .workerRemove(name):
            try c.encode(name, forKey: .name)
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let op = try c.decode(String.self, forKey: .op)
        func s(_ key: CodingKeys) throws -> String { try c.decode(String.self, forKey: key) }
        func ws() throws -> String { try s(.workspace) }
        switch op {
        case "workspace-add":
            self = .workspaceAdd(name: try s(.name), root: try s(.root), repos: try c.decode(.repos, or: []))
        case "workspace-update":
            self = .workspaceUpdate(
                workspace: try ws(), root: try c.decodeIfPresent(String.self, forKey: .root),
                repos: try c.decodeIfPresent([String].self, forKey: .repos),
                agentCommand: try FieldChange(c, .agentCommand), parkAfter: try FieldChange(c, .parkAfter))
        case "workspace-remove": self = .workspaceRemove(workspace: try ws())
        case "env-set": self = .envSet(workspace: try ws(), key: try s(.key), value: try s(.value))
        case "env-unset": self = .envUnset(workspace: try ws(), key: try s(.key))
        case "loop-checks":
            self = .loopChecks(workspace: try ws(), checks: try c.decodeIfPresent(LoopChecks.self, forKey: .checks))
        case "routines-gate":
            self = .routinesGate(workspace: try ws(), enabled: try c.decode(Bool.self, forKey: .enabled),
                                 directWrites: try c.decode(.directWrites, or: false))
        case "connection-add", "connection-update":
            let spec = try c.decode(ConnectionSpec.self, forKey: .connection)
            let secret = try c.decodeIfPresent(String.self, forKey: .secret)
            self = op == "connection-add"
                ? .connectionAdd(workspace: try ws(), connection: spec, secret: secret)
                : .connectionUpdate(workspace: try ws(), connection: spec, secret: secret)
        case "connection-secret": self = .connectionSecret(workspace: try ws(), name: try s(.name), secret: try s(.secret))
        case "connection-remove": self = .connectionRemove(workspace: try ws(), name: try s(.name))
        case "mcp-add": self = .mcpAdd(workspace: try ws(), mcp: try c.decode(McpSpec.self, forKey: .mcp))
        case "mcp-update": self = .mcpUpdate(workspace: try ws(), mcp: try c.decode(McpSpec.self, forKey: .mcp))
        case "mcp-remove": self = .mcpRemove(workspace: try ws(), name: try s(.name))
        case "routine-add", "routine-update":
            let spec = try c.decode(RoutineSpec.self, forKey: .routine)
            let text = try c.decodeIfPresent(String.self, forKey: .instructionsText)
            self = op == "routine-add"
                ? .routineAdd(workspace: try ws(), routine: spec, instructionsText: text)
                : .routineUpdate(workspace: try ws(), routine: spec, instructionsText: text)
        case "routine-remove": self = .routineRemove(workspace: try ws(), name: try s(.name))
        case "worker-add": self = .workerAdd(worker: try c.decode(WorkerSpec.self, forKey: .worker))
        case "worker-update": self = .workerUpdate(worker: try c.decode(WorkerSpec.self, forKey: .worker))
        case "worker-remove": self = .workerRemove(name: try s(.name))
        default:
            throw DecodingError.dataCorruptedError(forKey: .op, in: c, debugDescription: "unknown op")
        }
    }
}

extension KeyedEncodingContainer {
    /// Nothing for `keep`, `null` for `clear`.
    mutating func encode<V>(_ change: FieldChange<V>, forKey key: Key) throws {
        switch change {
        case .keep: break
        case .clear: try encodeNil(forKey: key)
        case let .set(value): try encode(value, forKey: key)
        }
    }
}

extension FieldChange {
    init<K>(_ c: KeyedDecodingContainer<K>, _ key: K) throws {
        if !c.contains(key) {
            self = .keep
        } else if try c.decodeNil(forKey: key) {
            self = .clear
        } else {
            self = .set(try c.decode(Value.self, forKey: key))
        }
    }
}

extension SettingsOp: CustomStringConvertible, CustomDebugStringConvertible, CustomReflectable {
    private static let hidden = "<write-only>"

    public var description: String {
        switch self {
        case let .workspaceAdd(name, root, repos): "\(self.name)(\(name), root: \(root), repos: \(repos))"
        case let .workspaceUpdate(workspace, root, repos, agentCommand, parkAfter):
            "\(name)(\(workspace), root: \(root ?? "keep"), repos: \(repos.map { "\($0)" } ?? "keep"), "
                + "agent_command: \(agentCommand), park_after: \(parkAfter))"
        case let .workspaceRemove(workspace): "\(name)(\(workspace))"
        case let .envSet(workspace, key, _): "\(name)(\(workspace), \(key)=\(Self.hidden))"
        case let .envUnset(workspace, key): "\(name)(\(workspace), \(key))"
        case let .loopChecks(workspace, checks): "\(name)(\(workspace), \(checks.map { "\($0)" } ?? "none"))"
        case let .routinesGate(workspace, enabled, directWrites):
            "\(name)(\(workspace), enabled: \(enabled), direct_writes: \(directWrites))"
        case let .connectionAdd(workspace, connection, secret), let .connectionUpdate(workspace, connection, secret):
            "\(name)(\(workspace), \(connection), secret: \(secret == nil ? "none" : Self.hidden))"
        case let .connectionSecret(workspace, connection, _): "\(name)(\(workspace), \(connection), secret: \(Self.hidden))"
        case let .connectionRemove(workspace, n), let .mcpRemove(workspace, n), let .routineRemove(workspace, n):
            "\(name)(\(workspace), \(n))"
        case let .mcpAdd(workspace, mcp), let .mcpUpdate(workspace, mcp): "\(name)(\(workspace), \(mcp))"
        case let .routineAdd(workspace, routine, text), let .routineUpdate(workspace, routine, text):
            "\(name)(\(workspace), \(routine), instructions_text: \(text.map { "\($0.utf8.count) bytes" } ?? "keep"))"
        case let .workerAdd(worker), let .workerUpdate(worker): "\(name)(\(worker))"
        case let .workerRemove(n): "\(name)(\(n))"
        }
    }

    public var debugDescription: String { description }

    public var customMirror: Mirror { Mirror(self, children: ["op": description]) }
}
