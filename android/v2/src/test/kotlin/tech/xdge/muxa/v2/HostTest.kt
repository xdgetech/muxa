package tech.xdge.muxa.v2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The host report as protocol v2 sends it (ADR 0103). */
class HostTest {
    private val sample = """
        {"sampled_ms":1795700000000,
         "cpu":{"percent":12.5,"cores":10},
         "load":[2.1,2.4,2.6],
         "memory":{"total":25769803776,"used":24696061952,"compressed":8160437862,"pressure":"warn"},
         "swap":{"used":4402341478,"total":5368709120},
         "disks":[{"name":"build","path":"/Volumes/B","free":1,"total":2},
                  {"name":"system","path":"/","free":44023414784,"total":494384795648}],
         "groups":[{"kind":"task","name":"Host resources","terminal":"t7","workspace":"xdge","cpu":0.3,"memory":429496729,"processes":6,"idle_since_ms":1795690000000},
                   {"kind":"simulator","name":"Simulator","cpu":0.1,"memory":3328599654,"processes":240},
                   {"kind":"quantum","name":"New","cpu":0,"memory":1,"processes":1}],
         "alerts":["memory","disk:/"]}
    """

    @Test
    fun aReportDecodesWithItsGroupsAndAlerts() {
        val r = json.decodeFromString<HostReport>(sample)
        assertEquals(1_795_700_000_000, r.sampledMs)
        assertEquals(HostCpu(12.5, 10), r.cpu)
        assertEquals(8_160_437_862L, r.memory.compressed)
        assertEquals("warn", r.memory.pressure)
        assertEquals("/", r.systemDisk?.path)
        assertTrue(r.alerted(r.systemDisk!!))
        val task = r.groups[0]
        assertEquals(UsageKind.TASK, task.usageKind)
        assertTrue(task.isAgentWork)
        assertEquals(1_795_690_000_000, task.idleSinceMs)
        assertNull(r.groups[1].idleSinceMs)
        assertEquals(UsageKind.SIMULATOR, r.groups[1].usageKind)
        assertEquals(UsageKind.UNKNOWN, r.groups[2].usageKind)
    }

    @Test
    fun noCompressedMemoryIsNull() {
        val r = json.decodeFromString<HostReport>(
            """{"sampled_ms":1,"cpu":{"percent":0,"cores":1},"load":[0,0,0],
               "memory":{"total":2,"used":1,"pressure":"normal"},"swap":{"used":0,"total":0},
               "disks":[],"groups":[],"alerts":[]}""",
        )
        assertNull(r.memory.compressed)
        assertNull(r.systemDisk)
    }

    @Test
    fun theRequestsNameTheirTypes() {
        assertEquals("host", V2Request.Host.json(1)["type"]?.toString()?.trim('"'))
        assertEquals("host-watch", V2Request.HostWatch.json(2)["type"]?.toString()?.trim('"'))
        assertEquals("host-unwatch", V2Request.HostUnwatch.json(3)["type"]?.toString()?.trim('"'))
        assertEquals(setOf("id", "type"), V2Request.HostWatch.json(2).keys)
    }
}
