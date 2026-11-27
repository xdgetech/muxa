import Foundation

/// The host's resources (ADR 0105). Sizes are bytes; `cpu` is a percent of all cores.
public struct HostReport: Codable, Equatable, Sendable {
    /// ms since the Unix epoch.
    public var sampledMs: Int64
    public var cpu: HostCpu
    /// The 1, 5 and 15 minute load averages.
    public var load: [Double]
    public var memory: HostMemory
    public var swap: HostSwap
    public var disks: [HostDisk]
    /// Tasks, talks, Routine runs and members first, then the rest; biggest first in each.
    public var groups: [UsageGroup]
    /// Values past the host's limits: `cpu`, `memory`, `disk:<path>`.
    public var alerts: [String]

    public init(sampledMs: Int64, cpu: HostCpu, load: [Double], memory: HostMemory, swap: HostSwap,
                disks: [HostDisk], groups: [UsageGroup], alerts: [String]) {
        self.sampledMs = sampledMs
        self.cpu = cpu
        self.load = load
        self.memory = memory
        self.swap = swap
        self.disks = disks
        self.groups = groups
        self.alerts = alerts
    }

    /// The system disk, else the first one listed.
    public var systemDisk: HostDisk? { disks.first { $0.name == "system" } ?? disks.first }
    public var buildDisk: HostDisk? { disks.first { $0.name == "build" } }

    public var cpuAlert: Bool { alerts.contains("cpu") }
    public var memoryAlert: Bool { alerts.contains("memory") }
    public func alerted(_ disk: HostDisk) -> Bool { alerts.contains("disk:\(disk.path)") }
}

public struct HostCpu: Codable, Equatable, Sendable {
    public var percent: Double
    public var cores: Int

    public init(percent: Double, cores: Int) {
        self.percent = percent
        self.cores = cores
    }
}

public struct HostMemory: Codable, Equatable, Sendable {
    public var total: UInt64
    public var used: UInt64
    /// nil where the OS keeps no compressed memory.
    public var compressed: UInt64?
    public var pressure: MemoryPressure

    public init(total: UInt64, used: UInt64, compressed: UInt64? = nil, pressure: MemoryPressure) {
        self.total = total
        self.used = used
        self.compressed = compressed
        self.pressure = pressure
    }
}

public enum MemoryPressure: Codable, Equatable, Sendable {
    case normal, warn, critical
    /// `unknown`, or a value this client does not know yet.
    case unknown(String)

    public init(rawValue: String) {
        switch rawValue {
        case "normal": self = .normal
        case "warn": self = .warn
        case "critical": self = .critical
        default: self = .unknown(rawValue)
        }
    }

    public var rawValue: String {
        switch self {
        case .normal: "normal"
        case .warn: "warn"
        case .critical: "critical"
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

public struct HostSwap: Codable, Equatable, Sendable {
    public var used: UInt64
    public var total: UInt64

    public init(used: UInt64, total: UInt64) {
        self.used = used
        self.total = total
    }
}

public struct HostDisk: Codable, Equatable, Sendable {
    /// `system` or `build`.
    public var name: String
    public var path: String
    public var free: UInt64
    public var total: UInt64

    public init(name: String, path: String, free: UInt64, total: UInt64) {
        self.name = name
        self.path = path
        self.free = free
        self.total = total
    }
}

/// What uses the host, grouped by what the user can act on. A name only, never a command line.
public struct UsageGroup: Codable, Equatable, Sendable {
    public var kind: UsageKind
    public var name: String
    /// The agent terminal of a Task, talk, Routine run or member.
    public var terminal: String?
    public var workspace: String?
    public var cpu: Double
    /// What the OS charges to the group's processes, in bytes.
    public var memory: UInt64
    public var processes: Int
    /// When the agent last reported `done` or `idle`; nil while it works.
    public var idleSinceMs: Int64?

    public init(kind: UsageKind, name: String, terminal: String? = nil, workspace: String? = nil, cpu: Double,
                memory: UInt64, processes: Int, idleSinceMs: Int64? = nil) {
        self.kind = kind
        self.name = name
        self.terminal = terminal
        self.workspace = workspace
        self.cpu = cpu
        self.memory = memory
        self.processes = processes
        self.idleSinceMs = idleSinceMs
    }

    /// A Task, talk, Routine run or member: work with an agent terminal.
    public var isAgentWork: Bool { [.task, .routine, .member].contains(kind) }
}

public enum UsageKind: Codable, Equatable, Hashable, Sendable {
    case task, routine, member, claudeOutside, simulator, docker, vm, app, other
    /// A kind a newer host added.
    case unknown(String)

    public init(rawValue: String) {
        switch rawValue {
        case "task": self = .task
        case "routine": self = .routine
        case "member": self = .member
        case "claude-outside": self = .claudeOutside
        case "simulator": self = .simulator
        case "docker": self = .docker
        case "vm": self = .vm
        case "app": self = .app
        case "other": self = .other
        default: self = .unknown(rawValue)
        }
    }

    public var rawValue: String {
        switch self {
        case .task: "task"
        case .routine: "routine"
        case .member: "member"
        case .claudeOutside: "claude-outside"
        case .simulator: "simulator"
        case .docker: "docker"
        case .vm: "vm"
        case .app: "app"
        case .other: "other"
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
