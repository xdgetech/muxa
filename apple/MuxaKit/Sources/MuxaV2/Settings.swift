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
    /// The Capture assistant (ADR 0084); nil from an older server.
    public var assistant: SettingsAssistant?
    /// `[[subscription]]`: named agent logins (ADR 0091).
    public var subscriptions: [SubscriptionSpec]
    /// Logins on the host that are no subscription yet, for onboarding (ADR 0091 §7).
    public var foundLogins: [FoundLogin]

    public init(revision: String, readOnly: Bool = false, problem: String? = nil, workspaces: [SettingsWorkspace] = [],
                workers: [WorkerSpec] = [], assistant: SettingsAssistant? = nil, subscriptions: [SubscriptionSpec] = [],
                foundLogins: [FoundLogin] = []) {
        self.subscriptions = subscriptions
        self.foundLogins = foundLogins
        self.revision = revision
        self.readOnly = readOnly
        self.problem = problem
        self.workspaces = workspaces
        self.workers = workers
        self.assistant = assistant
    }

    enum CodingKeys: String, CodingKey {
        case revision, problem, workspaces, workers, assistant, subscriptions
        case readOnly = "read_only"
        case foundLogins = "found_logins"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        revision = try c.decode(String.self, forKey: .revision)
        readOnly = try c.decode(.readOnly, or: false)
        problem = try c.decodeIfPresent(String.self, forKey: .problem)
        workspaces = try c.decode(.workspaces, or: [])
        workers = try c.decode(.workers, or: [])
        assistant = try c.decodeIfPresent(SettingsAssistant.self, forKey: .assistant)
        subscriptions = try c.decode(.subscriptions, or: [])
        foundLogins = try c.decode(.foundLogins, or: [])
    }
}

/// A login the host found without opening a file: a default folder, or a
/// Workspace's env variable. An env value never crosses the wire, so a
/// Workspace's login has no `home`; `subscriptionAdopt` makes it one.
public struct FoundLogin: Codable, Equatable, Sendable, Hashable {
    /// `claude-code` or `codex`.
    public var agent: String
    /// `~/.claude` or `~/.codex`; nil for a Workspace's env login.
    public var home: String?
    /// The Workspace whose env names this login.
    public var workspace: String?

    public init(agent: String, home: String? = nil, workspace: String? = nil) {
        self.agent = agent
        self.home = home
        self.workspace = workspace
    }

    enum CodingKeys: String, CodingKey { case agent, home, workspace }
}

/// `[[subscription]]` (ADR 0091): an agent login muxa names. The login stays
/// with the agent; muxa keeps the name and the folder.
public struct SubscriptionSpec: Codable, Equatable, Sendable, Identifiable {
    /// Lowercase letters, digits, `-` and `_`, as a Bot's name.
    public var name: String
    /// `claude-code` or `codex`.
    public var agent: String
    /// As written, `~/` included.
    public var home: String
    /// A label only.
    public var plan: String?

    public var id: String { name }

    public init(name: String, agent: String, home: String, plan: String? = nil) {
        self.name = name
        self.agent = agent
        self.home = home
        self.plan = plan
    }

    enum CodingKeys: String, CodingKey { case name, agent, home, plan }
}

/// `[assistant]` in `workspaces.toml`: the Capture assistant, with no
/// Workspace and no connections (ADR 0084).
public struct SettingsAssistant: Codable, Equatable, Sendable {
    public var enabled: Bool
    /// Nil: the default agent.
    public var worker: String?
    /// `assistant/soul.md`; nil when it cannot be read, empty when there is none.
    public var soulText: String?
    /// `assistant/memory.md`, which the assistant writes.
    public var memoryText: String?
    /// The Claude subscriptions it runs on, in order (ADR 0091).
    public var subscriptions: [String]

    public init(enabled: Bool = false, worker: String? = nil, soulText: String? = "", memoryText: String? = "",
                subscriptions: [String] = []) {
        self.subscriptions = subscriptions
        self.enabled = enabled
        self.worker = worker
        self.soulText = soulText
        self.memoryText = memoryText
    }

    enum CodingKeys: String, CodingKey {
        case enabled, worker, subscriptions
        case soulText = "soul_text"
        case memoryText = "memory_text"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try c.decode(.enabled, or: false)
        worker = try c.decodeIfPresent(String.self, forKey: .worker)
        soulText = try c.decodeIfPresent(String.self, forKey: .soulText)
        memoryText = try c.decodeIfPresent(String.self, forKey: .memoryText)
        subscriptions = try c.decode(.subscriptions, or: [])
    }
}

public struct SettingsWorkspace: Codable, Equatable, Sendable, Identifiable {
    public var name: String
    /// As written in the file, `~/` included.
    public var root: String
    public var repos: [String]
    /// Repos the host found under the root, hidden ones too (ADR 0031).
    public var foundRepos: [String]
    /// Found repos the Workspace leaves out of its repo list.
    public var hiddenRepos: [String]
    /// `30m`, `12h`, `2d`; nil: never parked (ADR 0034).
    public var parkAfter: String?
    public var agent: SettingsAgent
    /// Nil: no Task Loops.
    public var loop: WorkspaceLoop?
    public var routinesEnabled: Bool
    public var directWrites: Bool
    /// The user's claude.ai connectors load beside the MCP servers (ADR 0087).
    public var claudeConnectors: Bool
    /// Claude Code permission rules for this Workspace's Bots only (ADR 0087).
    public var botRules: BotRules
    /// The subscriptions its agents use, in order (ADR 0091).
    public var subscriptions: [String] = []
    /// False: each agent keeps the first of its kind, full or not; the user
    /// switches by moving one to the top (ADR 0091 §4).
    public var switchSubscriptions: Bool = true
    public var connections: [SettingsConnection]
    public var mcp: [McpSpec]
    /// Also while `routinesEnabled` is false.
    public var routines: [SettingsRoutine]
    /// The `[[bot]]` tables of `.muxa/routines.toml` (ADR 0082).
    public var bots: [SettingsBot] = []
    /// Why `.muxa/routines.toml` cannot be used as it is now.
    public var routinesProblem: String?

    public var id: String { name }

    public init(name: String, root: String, repos: [String] = [], parkAfter: String? = nil, agent: SettingsAgent = SettingsAgent(),
                loop: WorkspaceLoop? = nil, routinesEnabled: Bool = false, directWrites: Bool = false,
                connections: [SettingsConnection] = [], mcp: [McpSpec] = [], routines: [SettingsRoutine] = [],
                routinesProblem: String? = nil, foundRepos: [String] = [], hiddenRepos: [String] = [],
                botRules: BotRules = BotRules(), bots: [SettingsBot] = [], claudeConnectors: Bool = false,
                subscriptions: [String] = [], switchSubscriptions: Bool = true) {
        self.subscriptions = subscriptions
        self.switchSubscriptions = switchSubscriptions
        self.bots = bots
        self.botRules = botRules
        self.claudeConnectors = claudeConnectors
        self.name = name
        self.root = root
        self.repos = repos
        self.foundRepos = foundRepos
        self.hiddenRepos = hiddenRepos
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
        case name, root, repos, agent, loop, connections, mcp, routines, bots, subscriptions
        case parkAfter = "park_after"
        case routinesEnabled = "routines_enabled"
        case directWrites = "direct_writes"
        case claudeConnectors = "claude_connectors"
        case switchSubscriptions = "switch_subscriptions"
        case botRules = "bot_rules"
        case routinesProblem = "routines_problem"
        case foundRepos = "found_repos"
        case hiddenRepos = "hidden_repos"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        root = try c.decode(String.self, forKey: .root)
        repos = try c.decode(.repos, or: [])
        foundRepos = try c.decode(.foundRepos, or: [])
        hiddenRepos = try c.decode(.hiddenRepos, or: [])
        parkAfter = try c.decodeIfPresent(String.self, forKey: .parkAfter)
        agent = try c.decode(.agent, or: SettingsAgent())
        loop = try c.decodeIfPresent(WorkspaceLoop.self, forKey: .loop)
        routinesEnabled = try c.decode(.routinesEnabled, or: false)
        directWrites = try c.decode(.directWrites, or: false)
        claudeConnectors = try c.decode(.claudeConnectors, or: false)
        botRules = try c.decode(.botRules, or: BotRules())
        subscriptions = try c.decode(.subscriptions, or: [])
        switchSubscriptions = try c.decode(.switchSubscriptions, or: true)
        connections = try c.decode(.connections, or: [])
        mcp = try c.decode(.mcp, or: [])
        routines = try c.decode(.routines, or: [])
        bots = try c.decode(.bots, or: [])
        routinesProblem = try c.decodeIfPresent(String.self, forKey: .routinesProblem)
    }
}

/// `[workspace.bots]`: Claude Code permission rules such as `Bash` or
/// `mcp__claude_ai_Gmail`, given only to the Workspace's Bots (ADR 0087).
public struct BotRules: Codable, Equatable, Sendable {
    public var allow: [String]
    public var deny: [String]

    public init(allow: [String] = [], deny: [String] = []) {
        self.allow = allow
        self.deny = deny
    }

    public var isEmpty: Bool { allow.isEmpty && deny.isEmpty }

    enum CodingKeys: String, CodingKey { case allow, deny }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        allow = try c.decode(.allow, or: [])
        deny = try c.decode(.deny, or: [])
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
    /// The `[[bot]]` that owns it (ADR 0082).
    public var bot: String?
    /// Claude Code's edit tools are denied to the run.
    public var readOnly: Bool
    /// The event that runs it instead of a time: only `review` (ADR 0084).
    public var on: String?

    public var id: String { name }

    public init(name: String, at: String? = nil, schedule: String? = nil, timezone: String? = nil, worker: String? = nil,
                instructions: String, project: String? = nil, enabled: Bool = true, items: UInt32? = nil,
                directWrites: Bool = false, bot: String? = nil, readOnly: Bool = false, on: String? = nil) {
        self.bot = bot
        self.readOnly = readOnly
        self.on = on
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
        case name, at, schedule, timezone, worker, instructions, project, enabled, items, bot, on
        case directWrites = "direct_writes"
        case readOnly = "read_only"
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
        bot = try c.decodeIfPresent(String.self, forKey: .bot)
        readOnly = try c.decode(.readOnly, or: false)
        on = try c.decodeIfPresent(String.self, forKey: .on)
    }
}

/// `[[bot]]` (ADR 0082): a named owner of Routines with a Soul and a Memory.
public struct BotSpec: Codable, Equatable, Sendable, Identifiable {
    /// Lowercase letters, digits, `-` and `_`.
    public var name: String
    public var description: String?
    public var worker: String?
    /// Nil: 3.
    public var dailyItems: UInt32?
    public var enabled: Bool
    /// A team lead's members (ADR 0085): Bots of the same Workspace that lead
    /// no team themselves.
    public var members: [String]
    /// A lead's handoffs a day, all members together. Nil: 10.
    public var handoffsPerDay: UInt32?
    /// Shell commands only this Bot may run (ADR 0088): the inside of a
    /// Claude Code `Bash(...)` rule, such as `/path/collect.sh delivery:*`.
    public var commands: [String]
    /// Its own order of its Workspace's subscriptions (ADR 0091). Empty: the
    /// Workspace's.
    public var subscriptions: [String]

    public var id: String { name }

    /// 1 to 64 lowercase letters, digits, `-` or `_`, as the host checks a name.
    public static func isValidName(_ name: String) -> Bool {
        (1...64).contains(name.count)
            && name.allSatisfy { ($0.isASCII && ($0.isLowercase || $0.isNumber)) || $0 == "-" || $0 == "_" }
    }

    public init(name: String, description: String? = nil, worker: String? = nil, dailyItems: UInt32? = nil,
                enabled: Bool = true, members: [String] = [], handoffsPerDay: UInt32? = nil, commands: [String] = [],
                subscriptions: [String] = []) {
        self.subscriptions = subscriptions
        self.commands = commands
        self.name = name
        self.description = description
        self.worker = worker
        self.dailyItems = dailyItems
        self.enabled = enabled
        self.members = members
        self.handoffsPerDay = handoffsPerDay
    }

    enum CodingKeys: String, CodingKey {
        case name, description, worker, enabled, members, commands, subscriptions
        case dailyItems = "daily_items"
        case handoffsPerDay = "handoffs_per_day"
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .name)
        try c.encodeIfPresent(description, forKey: .description)
        try c.encodeIfPresent(worker, forKey: .worker)
        try c.encodeIfPresent(dailyItems, forKey: .dailyItems)
        try c.encode(enabled, forKey: .enabled)
        // As the host spells it: no `members` for a Bot that leads no team.
        if !members.isEmpty { try c.encode(members, forKey: .members) }
        try c.encodeIfPresent(handoffsPerDay, forKey: .handoffsPerDay)
        if !commands.isEmpty { try c.encode(commands, forKey: .commands) }
        if !subscriptions.isEmpty { try c.encode(subscriptions, forKey: .subscriptions) }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        description = try c.decodeIfPresent(String.self, forKey: .description)
        worker = try c.decodeIfPresent(String.self, forKey: .worker)
        dailyItems = try c.decodeIfPresent(UInt32.self, forKey: .dailyItems)
        enabled = try c.decode(.enabled, or: true)
        members = try c.decode(.members, or: [])
        handoffsPerDay = try c.decodeIfPresent(UInt32.self, forKey: .handoffsPerDay)
        commands = try c.decode(.commands, or: [])
        subscriptions = try c.decode(.subscriptions, or: [])
    }
}

/// A Bot as `settings` shows it, with its Soul and Memory.
public struct SettingsBot: Codable, Equatable, Sendable, Identifiable {
    public var spec: BotSpec
    /// `soul.md`; nil when it cannot be read, empty when there is none.
    public var soulText: String?
    /// `memory.md`, which the Bot's runs write.
    public var memoryText: String?
    /// How many earlier memories can come back.
    public var memoryVersions: UInt32
    /// When each earlier memory was replaced, newest first (ADR 0086); empty
    /// from an older server.
    public var memoryHistory: [MemoryVersion]

    public var id: String { spec.name }
    public var name: String { spec.name }

    public init(spec: BotSpec, soulText: String? = "", memoryText: String? = "", memoryVersions: UInt32 = 0,
                memoryHistory: [MemoryVersion] = []) {
        self.spec = spec
        self.soulText = soulText
        self.memoryText = memoryText
        self.memoryVersions = memoryVersions
        self.memoryHistory = memoryHistory
    }

    enum CodingKeys: String, CodingKey {
        case soulText = "soul_text"
        case memoryText = "memory_text"
        case memoryVersions = "memory_versions"
        case memoryHistory = "memory_history"
    }

    // On the wire the spec's fields and the texts share one object.
    public init(from decoder: any Decoder) throws {
        spec = try BotSpec(from: decoder)
        let c = try decoder.container(keyedBy: CodingKeys.self)
        soulText = try c.decodeIfPresent(String.self, forKey: .soulText)
        memoryText = try c.decodeIfPresent(String.self, forKey: .memoryText)
        memoryVersions = try c.decode(.memoryVersions, or: 0)
        memoryHistory = try c.decode(.memoryHistory, or: [])
    }

    public func encode(to encoder: any Encoder) throws {
        try spec.encode(to: encoder)
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(soulText, forKey: .soulText)
        try c.encodeIfPresent(memoryText, forKey: .memoryText)
        try c.encode(memoryVersions, forKey: .memoryVersions)
        if !memoryHistory.isEmpty { try c.encode(memoryHistory, forKey: .memoryHistory) }
    }
}

/// One earlier memory `botMemoryRestore` can bring back.
public struct MemoryVersion: Codable, Equatable, Sendable, Identifiable {
    /// 1 is the newest.
    public var version: UInt32
    /// Unix seconds when it was replaced.
    public var at: Int64

    public var id: UInt32 { version }

    public init(version: UInt32, at: Int64) {
        self.version = version
        self.at = at
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
