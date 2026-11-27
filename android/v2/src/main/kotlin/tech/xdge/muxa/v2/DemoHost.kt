package tech.xdge.muxa.v2

/**
 * The demo's answers to `host` and `disk`: a host short of memory whose live
 * agents are its usage groups, and the disk use of its Task places.
 */
internal object DemoHost {
    private const val GIB = 1L shl 30
    private const val MIB = 1L shl 20
    private const val HOUR_MS = 3_600_000L
    private const val DAY_MS = 24 * HOUR_MS

    /** The talk has waited 2 days; other idle agents 4 hours. */
    private const val LONG_IDLE = "t7"

    fun report(terminals: List<Terminal>, nowMs: Long): HostReport = HostReport(
        sampledMs = nowMs,
        cpu = HostCpu(percent = 12.0, cores = 10),
        load = listOf(2.1, 2.4, 2.6),
        memory = HostMemory(total = 24 * GIB, used = 23 * GIB, compressed = 7 * GIB, pressure = "warn"),
        swap = HostSwap(used = 4 * GIB, total = 5 * GIB),
        disks = listOf(
            HostDisk("system", "/", free = 41 * GIB, total = 460 * GIB),
            HostDisk("build", "/Volumes/Build", free = 1200 * GIB, total = 1800 * GIB),
        ),
        groups = agentGroups(terminals, nowMs) + OTHERS,
        alerts = listOf("memory"),
    )

    private fun agentGroups(terminals: List<Terminal>, nowMs: Long): List<UsageGroup> =
        terminals.filter { it.live && it.isAgent }.mapIndexed { i, t ->
            val idle = t.status?.state in IDLE_STATES
            UsageGroup(
                kind = t.labels["muxa.kind"] ?: "task",
                name = t.taskTitle ?: t.name ?: t.id,
                terminal = t.id,
                workspace = t.labels["muxa.workspace"],
                cpu = if (idle) 0.1 else 4.5 - i,
                memory = 900 * MIB - i * 120 * MIB,
                processes = 6,
                idleSinceMs = if (!idle) null else nowMs - if (t.id == LONG_IDLE) 2 * DAY_MS else 4 * HOUR_MS,
            )
        }

    private val IDLE_STATES = setOf("done", "idle")

    private val OTHERS = listOf(
        UsageGroup("simulator", "Simulator", cpu = 1.0, memory = 3 * GIB + 200 * MIB, processes = 240),
        UsageGroup("app", "Google Chrome", cpu = 3.2, memory = 2 * GIB + 400 * MIB, processes = 31),
        UsageGroup("vm", "Docker VM", cpu = 0.8, memory = GIB + 700 * MIB, processes = 1),
        UsageGroup("docker", "Docker", cpu = 0.2, memory = 300 * MIB, processes = 9),
        UsageGroup("claude-outside", "Claude Code", cpu = 0.4, memory = GIB + 100 * MIB, processes = 4),
        UsageGroup("other", "Other", cpu = 2.0, memory = 2 * GIB, processes = 412),
    )

    /** fix-login's and docs-audit's worktrees, and three leftovers. */
    val DISK: List<DiskRow> = listOf(
        DiskRow(
            "/demo/app-fix-login", "task-worktree", workspace = "app", branch = "fix-login",
            bytes = 420 * MIB, buildBytes = 2 * GIB, held = "live", clean = false, merged = false,
        ),
        DiskRow(
            "/demo/app-docs-audit", "task-worktree", workspace = "app", branch = "docs-audit",
            bytes = 380 * MIB, buildBytes = 0, held = "live", clean = true, merged = false,
        ),
        DiskRow(
            "/demo/app/.claude/worktrees/agent-a1", "claude-worktree", workspace = "app",
            bytes = 350 * MIB, clean = true, merged = true, cleanable = true,
        ),
        DiskRow(
            "/demo/app/.claude/worktrees/agent-b2", "claude-worktree", workspace = "app",
            bytes = 310 * MIB, clean = false, merged = false,
        ),
        DiskRow("/Volumes/Build/app-old-spike", "build-folder", workspace = "app", bytes = 3 * GIB, cleanable = true),
    )
}
