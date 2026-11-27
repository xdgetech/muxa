package tech.xdge.muxa.v2

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** The host's resources (ADR 0103); sizes in bytes, `cpu` in percent of all cores. */
@Serializable
data class HostReport(
    /** ms since the Unix epoch. */
    @SerialName("sampled_ms") val sampledMs: Long,
    val cpu: HostCpu,
    /** The 1, 5 and 15 minute load averages. */
    val load: List<Double> = emptyList(),
    val memory: HostMemory,
    val swap: HostSwap,
    val disks: List<HostDisk> = emptyList(),
    /** Tasks, talks, Routine runs and members first, then the rest; biggest first in each. */
    val groups: List<UsageGroup> = emptyList(),
    /** `cpu`, `memory` and `disk:<path>`: values past the host's limits. */
    val alerts: List<String> = emptyList(),
) {
    /** The system disk, or the first one listed. */
    val systemDisk: HostDisk? get() = disks.firstOrNull { it.name == "system" } ?: disks.firstOrNull()

    fun alerted(disk: HostDisk): Boolean = "disk:${disk.path}" in alerts
}

@Serializable
data class HostCpu(val percent: Double, val cores: Int)

@Serializable
data class HostMemory(
    val total: Long,
    val used: Long,
    /** Absent where the OS keeps no compressed memory. */
    val compressed: Long? = null,
    /** `normal`, `warn`, `critical`, or a value a newer host added. */
    val pressure: String = "unknown",
)

@Serializable
data class HostSwap(val used: Long, val total: Long)

@Serializable
data class HostDisk(
    /** `system` or `build`. */
    val name: String,
    val path: String,
    val free: Long,
    val total: Long,
)

/** What uses the host, grouped by what the user can act on. A name only, never a command line. */
@Serializable
data class UsageGroup(
    /** Kept as sent, so a kind a newer host adds still decodes; read it through [usageKind]. */
    val kind: String,
    val name: String,
    /** The agent terminal of a Task, talk, Routine run or member. */
    val terminal: String? = null,
    val workspace: String? = null,
    val cpu: Double,
    /** What the OS charges to the group: the footprint on macOS, resident memory on Linux. */
    val memory: Long,
    val processes: Int,
    /** When the agent last reported `done` or `idle`; null while it works. */
    @SerialName("idle_since_ms") val idleSinceMs: Long? = null,
) {
    val usageKind: UsageKind get() = UsageKind.entries.firstOrNull { it.wire == kind } ?: UsageKind.UNKNOWN

    /** A Task, talk, Routine run or member: something with an agent terminal. */
    val isAgentWork: Boolean get() = usageKind in AGENT_WORK
}

enum class UsageKind(val wire: String) {
    TASK("task"),
    ROUTINE("routine"),
    MEMBER("member"),
    CLAUDE_OUTSIDE("claude-outside"),
    SIMULATOR("simulator"),
    DOCKER("docker"),
    VM("vm"),
    APP("app"),
    OTHER("other"),

    /** A kind a newer host added. */
    UNKNOWN(""),
}

private val AGENT_WORK = setOf(UsageKind.TASK, UsageKind.ROUTINE, UsageKind.MEMBER)
