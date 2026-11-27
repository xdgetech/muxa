import Foundation

/// What the demo answers to `host` and `disk` (ADR 0102, 0105): a host short
/// of memory, with a group for each live agent of the demo's Tasks and Routines.
enum DemoHost {
    private static let gib: UInt64 = 1 << 30
    private static let mib: UInt64 = 1 << 20

    /// Each agent's figures by terminal id: memory in MiB, CPU, processes,
    /// and how long it has been idle, in seconds. Only t7 reports `done`.
    private static let agents: [String: (memory: UInt64, cpu: Double, processes: Int, idle: Int64?)] = [
        "t1": (1_240, 8.5, 9, nil),
        "t2": (410, 0.2, 5, nil),
        "t5": (320, 1.1, 4, nil),
        "t7": (930, 0.0, 6, 2 * 86_400),
        "t8": (210, 0.9, 3, nil),
        "t9": (640, 2.4, 5, nil),
    ]

    static func report(terminals: [Terminal], now: Date = .now) -> HostReport {
        let nowMs = Int64(now.timeIntervalSince1970 * 1000)
        let work = terminals
            .filter { $0.live && $0.labels["muxa.role"] == "agent" }
            .compactMap { t -> UsageGroup? in
                guard let kind = kind(t), let a = agents[t.id] else { return nil }
                return UsageGroup(
                    kind: kind, name: t.taskTitle ?? t.labels["muxa.routine"] ?? t.name ?? t.id, terminal: t.id,
                    workspace: t.labels["muxa.workspace"], cpu: a.cpu, memory: a.memory * mib, processes: a.processes,
                    idleSinceMs: a.idle.map { nowMs - $0 * 1000 })
            }
            .sorted { $0.memory > $1.memory }
        let rest = [
            UsageGroup(kind: .simulator, name: "Simulator", cpu: 3.2, memory: 3_100 * mib, processes: 240),
            UsageGroup(kind: .app, name: "Google Chrome", cpu: 4.1, memory: 2_400 * mib, processes: 31),
            UsageGroup(kind: .claudeOutside, name: "Claude Code", cpu: 0.6, memory: 2_100 * mib, processes: 6),
            UsageGroup(kind: .docker, name: "Docker", cpu: 1.4, memory: 1_700 * mib, processes: 4),
            UsageGroup(kind: .other, name: "Other", cpu: 2.0, memory: 1_300 * mib, processes: 412),
        ]
        return HostReport(
            sampledMs: nowMs, cpu: HostCpu(percent: 12, cores: 10), load: [2.1, 2.4, 2.6],
            memory: HostMemory(total: 24 * gib, used: 23 * gib, compressed: 7_800 * mib, pressure: .warn),
            swap: HostSwap(used: 4_200 * mib, total: 5 * gib),
            disks: [
                HostDisk(name: "system", path: "/", free: 41 * gib, total: 460 * gib),
                HostDisk(name: "build", path: "/Volumes/Build", free: 1_200 * gib, total: 1_860 * gib),
            ],
            groups: work + rest, alerts: ["memory"])
    }

    private static func kind(_ t: Terminal) -> UsageKind? {
        switch t.labels["muxa.kind"] {
        case "task": .task
        case "routine": t.lead != nil ? .member : .routine
        default: nil
        }
    }

    /// Two Task places with their build output, and leftovers no Task holds.
    static let diskRows: [DiskRow] = [
        DiskRow(path: "/demo/app-fix-login", kind: "task-worktree", workspace: "app", branch: "fix-login",
                bytes: 412 * mib, buildBytes: 2_150 * mib, held: "live", clean: false, merged: false, cleanable: false),
        DiskRow(path: "/demo/app-docs-audit", kind: "task-worktree", workspace: "app", branch: "docs-audit",
                bytes: 38 * mib, buildBytes: nil, held: "live", clean: true, merged: false, cleanable: false),
        DiskRow(path: "/demo/app/.claude/worktrees/agent-1a2b", kind: "claude-worktree", workspace: "app",
                branch: "worktree-agent-1a2b", bytes: 640 * mib, buildBytes: nil, held: nil, clean: true, merged: true,
                cleanable: true),
        DiskRow(path: "/Volumes/Build/muxa/app-old-branch", kind: "build-folder", workspace: "app", branch: nil,
                bytes: 3_400 * mib, buildBytes: nil, held: nil, clean: nil, merged: nil, cleanable: true),
    ]
}
