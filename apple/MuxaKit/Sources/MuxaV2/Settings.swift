import Foundation

// The host's editable settings (ADR 0074). These types spell their wire keys
// out and decode without a key strategy: `env` and `headers` keys are
// variable names that convertFromSnakeCase would change.

/// What `settings` answers. A secret or an env value is never in it.
public struct Settings: Codable, Equatable, Sendable {
    /// A hash of the files' bytes; `settingsUpdate` must name the current one.
    public var revision: String
    /// The daemon runs with `--settings-read-only`.
    public var readOnly: Bool
    /// Why `workspaces.toml` cannot be used as it is now.
    public var problem: String?
    public var workspaces: [SettingsWorkspace]
    public var workers: [WorkerSpec]

    public init(revision: String, readOnly: Bool = false, problem: String? = nil, workspaces: [SettingsWorkspace] = [],
                workers: [WorkerSpec] = []) {
        self.revision = revision
        self.readOnly = readOnly
        self.problem = problem
        self.workspaces = workspaces
        self.workers = workers
    }

    enum CodingKeys: String, CodingKey {
        case revision, problem, workspaces, workers
        case readOnly = "read_only"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        revision = try c.decode(String.self, forKey: .revision)
        readOnly = try c.decode(.readOnly, or: false)
        problem = try c.decodeIfPresent(String.self, forKey: .problem)
        workspaces = try c.decode(.workspaces, or: [])
        workers = try c.decode(.workers, or: [])
    }
}

public struct SettingsWorkspace: Codable, Equatable, Sendable, Identifiable {
    public var name: String
    /// As written in the file, `~/` included.
    public var root: String
    public var repos: [String]
    /// `30m`, `12h`, `2d`; nil: never parked (ADR 0034).
    public var parkAfter: String?
    public var agent: SettingsAgent
    /// Nil: no Task Loops.
    public var loop: WorkspaceLoop?
    public var routinesEnabled: Bool
    public var directWrites: Bool
    public var connections: [SettingsConnection]
    public var mcp: [McpSpec]
    /// Also while `routinesEnabled` is false.
    public var routines: [SettingsRoutine]
    /// Why `.muxa/routines.toml` cannot be used as it is now.
    public var routinesProblem: String?

    public var id: String { name }

    public init(name: String, root: String, repos: [String] = [], parkAfter: String? = nil, agent: SettingsAgent = SettingsAgent(),
                loop: WorkspaceLoop? = nil, routinesEnabled: Bool = false, directWrites: Bool = false,
                connections: [SettingsConnection] = [], mcp: [McpSpec] = [], routines: [SettingsRoutine] = [],
                routinesProblem: String? = nil) {
        self.name = name
        self.root = root
        self.repos = repos
        self.parkAfter = parkAfter
        self.agent = agent
        self.loop = loop
        self.routinesEnabled = routinesEnabled
        self.directWrites = directWrites
        self.connections = connections
        self.mcp = mcp
        self.routines = routines
        self.routinesProblem = routinesProblem
    }

    enum CodingKeys: String, CodingKey {
        case name, root, repos, agent, loop, connections, mcp, routines
        case parkAfter = "park_after"
        case routinesEnabled = "routines_enabled"
        case directWrites = "direct_writes"
        case routinesProblem = "routines_problem"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        root = try c.decode(String.self, forKey: .root)
        repos = try c.decode(.repos, or: [])
        parkAfter = try c.decodeIfPresent(String.self, forKey: .parkAfter)
        agent = try c.decode(.agent, or: SettingsAgent())
        loop = try c.decodeIfPresent(WorkspaceLoop.self, forKey: .loop)
        routinesEnabled = try c.decode(.routinesEnabled, or: false)
        directWrites = try c.decode(.directWrites, or: false)
        connections = try c.decode(.connections, or: [])
        mcp = try c.decode(.mcp, or: [])
        routines = try c.decode(.routines, or: [])
        routinesProblem = try c.decodeIfPresent(String.self, forKey: .routinesProblem)
    }
}

/// `[workspace.agent]`.
public struct SettingsAgent: Codable, Equatable, Sendable {
    /// Nil: the daemon's `--task-command`.
    public var command: String?
    /// The variables' keys; a value never comes back from the host.
    public var env: [EnvKey]

    public init(command: String? = nil, env: [EnvKey] = []) {
        self.command = command
        self.env = env
    }

    enum CodingKeys: String, CodingKey { case command, env }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        command = try c.decodeIfPresent(String.self, forKey: .command)
        env = try c.decode(.env, or: [])
    }
}

public struct EnvKey: Codable, Equatable, Sendable, Identifiable {
    public var key: String
    public var set: Bool

    public var id: String { key }

    public init(key: String, set: Bool = true) {
        self.key = key
        self.set = set
    }
}

/// Whether a connection's Keychain secret exists; never the secret.
public enum SecretState: Codable, Equatable, Sendable {
    case set
    case missing
    /// The host's Keychain is locked.
    case locked
    /// The host has no macOS Keychain.
    case unavailable
    /// A state this client does not know yet.
    case unknown(String)

    public init(rawValue: String) {
        switch rawValue {
        case "set": self = .set
        case "missing": self = .missing
        case "locked": self = .locked
        case "unavailable": self = .unavailable
        default: self = .unknown(rawValue)
        }
    }

    public var rawValue: String {
        switch self {
        case .set: "set"
        case .missing: "missing"
        case .locked: "locked"
        case .unavailable: "unavailable"
        case let .unknown(word): word
        }
    }

    public init(from decoder: any Decoder) throws {
        self.init(rawValue: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(rawValue)
    }
}

/// `[[workspace.connection]]` without its secret; also what the ops take.
/// Every agent of the Workspace gets it (ADR 0075), so there is no `default`.
public struct ConnectionSpec: Codable, Equatable, Sendable {
    public var name: String
    /// The variable the agent gets the secret in.
    public var env: String
    /// A label, e.g. the account's email.
    public var account: String?
    /// `read`, `write` or nil.
    public var access: String?

    public init(name: String, env: String, account: String? = nil, access: String? = nil) {
        self.name = name
        self.env = env
        self.account = account
        self.access = access
    }

    enum CodingKeys: String, CodingKey {
        case name, env, account, access
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        env = try c.decode(String.self, forKey: .env)
        account = try c.decodeIfPresent(String.self, forKey: .account)
        access = try c.decodeIfPresent(String.self, forKey: .access)
    }
}

/// A connection as `settings` shows it: its fields and whether its secret exists.
public struct SettingsConnection: Codable, Equatable, Sendable, Identifiable {
    public var spec: ConnectionSpec
    public var secret: SecretState

    public var id: String { spec.name }
    public var name: String { spec.name }

    public init(spec: ConnectionSpec, secret: SecretState) {
        self.spec = spec
        self.secret = secret
    }

    enum CodingKeys: String, CodingKey { case secret }

    // On the wire the spec's fields and `secret` share one object.
    public init(from decoder: any Decoder) throws {
        spec = try ConnectionSpec(from: decoder)
        secret = try decoder.container(keyedBy: CodingKeys.self).decode(SecretState.self, forKey: .secret)
    }

    public func encode(to encoder: any Encoder) throws {
        try spec.encode(to: encoder)
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(secret, forKey: .secret)
    }
}

/// `[[workspace.mcp]]`: `command` with `args`, or `url` with `transport`.
/// Nothing here is a secret. Every agent of the Workspace gets it (ADR 0075).
public struct McpSpec: Codable, Equatable, Sendable, Identifiable {
    public var name: String
    public var command: String?
    public var args: [String]?
    public var url: String?
    /// `http` (nil means it too) or `sse`; `type` on the wire.
    public var transport: String?
    public var env: [String: String]
    public var headers: [String: String]
    /// The connections whose variables the server gets.
    public var connections: [String]

    public var id: String { name }

    public init(name: String, command: String? = nil, args: [String]? = nil, url: String? = nil, transport: String? = nil,
                env: [String: String] = [:], headers: [String: String] = [:], connections: [String] = []) {
        self.name = name
        self.command = command
        self.args = args
        self.url = url
        self.transport = transport
        self.env = env
        self.headers = headers
        self.connections = connections
    }

    enum CodingKeys: String, CodingKey {
        case name, command, args, url, env, headers, connections
        case transport = "type"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        command = try c.decodeIfPresent(String.self, forKey: .command)
        args = try c.decodeIfPresent([String].self, forKey: .args)
        url = try c.decodeIfPresent(String.self, forKey: .url)
        transport = try c.decodeIfPresent(String.self, forKey: .transport)
        env = try c.decode(.env, or: [:])
        headers = try c.decode(.headers, or: [:])
        connections = try c.decode(.connections, or: [])
    }
}

/// `[[routine]]` with `[routine.output]` flattened. A run gets all its
/// Workspace's connections and MCP servers (ADR 0075).
public struct RoutineSpec: Codable, Equatable, Sendable, Identifiable {
    public var name: String
    /// `HH:MM` in `timezone`.
    public var at: String?
    /// The five cron fields.
    public var schedule: String?
    public var timezone: String?
    public var worker: String?
    /// The instruction file, relative to `.muxa/`.
    public var instructions: String
    public var project: String?
    public var enabled: Bool
    public var items: UInt32?
    public var directWrites: Bool

    public var id: String { name }

    public init(name: String, at: String? = nil, schedule: String? = nil, timezone: String? = nil, worker: String? = nil,
                instructions: String, project: String? = nil, enabled: Bool = true, items: UInt32? = nil,
                directWrites: Bool = false) {
        self.name = name
        self.at = at
        self.schedule = schedule
        self.timezone = timezone
        self.worker = worker
        self.instructions = instructions
        self.project = project
        self.enabled = enabled
        self.items = items
        self.directWrites = directWrites
    }

    enum CodingKeys: String, CodingKey {
        case name, at, schedule, timezone, worker, instructions, project, enabled, items
        case directWrites = "direct_writes"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        at = try c.decodeIfPresent(String.self, forKey: .at)
        schedule = try c.decodeIfPresent(String.self, forKey: .schedule)
        timezone = try c.decodeIfPresent(String.self, forKey: .timezone)
        worker = try c.decodeIfPresent(String.self, forKey: .worker)
        instructions = try c.decode(String.self, forKey: .instructions)
        project = try c.decodeIfPresent(String.self, forKey: .project)
        // A Routine is on unless its file says otherwise.
        enabled = try c.decode(.enabled, or: true)
        items = try c.decodeIfPresent(UInt32.self, forKey: .items)
        directWrites = try c.decode(.directWrites, or: false)
    }
}

/// A Routine as `settings` shows it, with its instruction file's content.
public struct SettingsRoutine: Codable, Equatable, Sendable, Identifiable {
    public var spec: RoutineSpec
    /// Nil when the file cannot be read.
    public var instructionsText: String?

    public var id: String { spec.name }
    public var name: String { spec.name }

    public init(spec: RoutineSpec, instructionsText: String? = nil) {
        self.spec = spec
        self.instructionsText = instructionsText
    }

    enum CodingKeys: String, CodingKey {
        case instructionsText = "instructions_text"
    }

    // On the wire the spec's fields and `instructions_text` share one object.
    public init(from decoder: any Decoder) throws {
        spec = try RoutineSpec(from: decoder)
        instructionsText = try decoder.container(keyedBy: CodingKeys.self)
            .decodeIfPresent(String.self, forKey: .instructionsText)
    }

    public func encode(to encoder: any Encoder) throws {
        try spec.encode(to: encoder)
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(instructionsText, forKey: .instructionsText)
    }
}

/// `[[worker]]`.
public struct WorkerSpec: Codable, Equatable, Sendable, Identifiable {
    public var name: String
    public var description: String?
    /// `claude-code` or `codex`.
    public var agent: String
    public var args: [String]
    public var instructions: String?
    /// Nil: every Workspace.
    public var workspaces: [String]?
    public var loop: WorkerLoop?

    public var id: String { name }

    public init(name: String, description: String? = nil, agent: String, args: [String] = [], instructions: String? = nil,
                workspaces: [String]? = nil, loop: WorkerLoop? = nil) {
        self.name = name
        self.description = description
        self.agent = agent
        self.args = args
        self.instructions = instructions
        self.workspaces = workspaces
        self.loop = loop
    }

    enum CodingKeys: String, CodingKey {
        case name, description, agent, args, instructions, workspaces, loop
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        agent = try c.decode(String.self, forKey: .agent)
        args = try c.decode(.args, or: [])
        instructions = try c.decodeIfPresent(String.self, forKey: .instructions)
        workspaces = try c.decodeIfPresent([String].self, forKey: .workspaces)
        loop = try c.decodeIfPresent(WorkerLoop.self, forKey: .loop)
    }
}

public struct WorkerLoop: Codable, Equatable, Sendable {
    public var check: String
    public var maxAttempts: UInt32?
    /// Seconds.
    public var timeout: UInt64?

    public init(check: String, maxAttempts: UInt32? = nil, timeout: UInt64? = nil) {
        self.check = check
        self.maxAttempts = maxAttempts
        self.timeout = timeout
    }

    enum CodingKeys: String, CodingKey {
        case check, timeout
        case maxAttempts = "max_attempts"
    }
}

extension KeyedDecodingContainer {
    /// The value, or `fallback` when the key is missing or null.
    func decode<T: Decodable>(_ key: Key, or fallback: T) throws -> T {
        try decodeIfPresent(T.self, forKey: key) ?? fallback
    }
}

extension Coding {
    /// For the settings types, which spell out their wire keys.
    static let plainDecoder = JSONDecoder()
    static let plainEncoder: JSONEncoder = {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys]
        return e
    }()
}
