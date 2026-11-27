package tech.xdge.muxa.app.next

import org.junit.Assert.assertEquals
import org.junit.Test
import tech.xdge.muxa.v2.DiskRow
import tech.xdge.muxa.v2.HostCpu
import tech.xdge.muxa.v2.HostDisk
import tech.xdge.muxa.v2.HostMemory
import tech.xdge.muxa.v2.HostReport
import tech.xdge.muxa.v2.HostSwap
import tech.xdge.muxa.v2.Terminal
import tech.xdge.muxa.v2.UsageGroup

class HostResourcesTest {
    private val gib = 1L shl 30

    private fun report(alerts: List<String>, disks: List<HostDisk>) = HostReport(
        sampledMs = 0, cpu = HostCpu(12.4, 10), memory = HostMemory(24 * gib, 23 * gib, pressure = "warn"),
        swap = HostSwap(0, 0), disks = disks, alerts = alerts,
    )

    @Test
    fun theSummaryShowsTheSystemDiskAndMarksAlertedValues() {
        val disks = listOf(HostDisk("build", "/b", 900 * gib, 1000 * gib), HostDisk("system", "/", 41 * gib, 460 * gib))
        val values = summaryValues(report(listOf("memory", "disk:/"), disks))
        assertEquals(listOf("CPU 12%", "Mem 96%", "41G free"), values.map { it.text })
        assertEquals(listOf(false, true, true), values.map { it.alerted })
        // A build disk's alert does not mark the system disk.
        assertEquals(false, summaryValues(report(listOf("disk:/b"), disks))[2].alerted)
    }

    @Test
    fun sizesReadShort() {
        assertEquals("512M", shortSize(512L shl 20))
        assertEquals("1.2T", shortSize(1229 * gib))
        assertEquals("420 MB", size(420L shl 20))
        assertEquals("2.4 GB", size(gib * 24 / 10))
        assertEquals("41 GB", size(41 * gib))
    }

    @Test
    fun anAgentIsWorkingOrIdleForAWhile() {
        val day = 86_400_000L
        fun group(idle: Long?) = UsageGroup("task", "t", cpu = 0.0, memory = 0, processes = 1, idleSinceMs = idle)
        assertEquals("working", agentState(group(null), 10 * day))
        assertEquals("idle 2 d", agentState(group(8 * day), 10 * day))
        assertEquals("idle 4 h", agentState(group(10 * day - 4 * 3_600_000), 10 * day))
        assertEquals("idle 0 min", agentState(group(10 * day + 5_000), 10 * day))
    }

    @Test
    fun diskRowsMatchLiveTasksByPathAndTheRestAreLeftovers() {
        val agent = Terminal(
            "t1", "fix", live = true, cols = 80, rows = 24,
            labels = mapOf(
                "muxa.kind" to "task", "muxa.role" to "agent", "muxa.dir" to "/w/fix",
                "muxa.places" to """[{"kind":"branch","path":"/w/fix-app","branch":"fix"}]""",
            ),
        )
        val rows = listOf(
            DiskRow("/w/fix-app", "task-worktree", bytes = 100, buildBytes = 50),
            DiskRow("/w/gone", "task-worktree", bytes = 7),
            DiskRow("/w/a/.claude/worktrees/x", "claude-worktree", bytes = 10, cleanable = true),
            DiskRow("/w/a/.claude/worktrees/y", "claude-worktree", bytes = 20),
            DiskRow("/b/old", "build-folder", bytes = 5, cleanable = true),
        )
        val use = diskUse(rows, listOf(agent))
        assertEquals(listOf(TaskDiskUse(agent, 100, 50)), use.tasks)
        assertEquals(
            listOf(
                Leftovers(LeftoverKind.CLAUDE_WORKTREES, 2, 30, 1),
                Leftovers(LeftoverKind.OTHER_WORKTREES, 1, 7, 0),
                Leftovers(LeftoverKind.BUILD_FOLDERS, 1, 5, 1),
            ),
            use.leftovers,
        )
    }
}
