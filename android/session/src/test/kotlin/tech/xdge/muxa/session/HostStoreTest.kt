package tech.xdge.muxa.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import tech.xdge.muxa.transport.HostKey
import tech.xdge.muxa.transport.SshTarget

class HostStoreTest {
    @get:Rule
    val dir = TemporaryFolder()

    @Test
    fun savedHostsSurviveAReload() {
        val file = dir.root.resolve("hosts.json")
        val store = HostStore(file)
        val a = Host(name = "mini", target = SshTarget(host = "mini.example", user = "me"))
        val b = Host(name = "", target = SshTarget(host = "box.example", port = 2222, user = "dev"))
        store.upsert(a)
        store.upsert(b)
        store.markUsed(b.id)
        store.move(1, 0)
        val edited = a.copy(
            target = a.target.copy(hostKey = HostKey.parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICmVLvUbABi5WZoMHwkY/jy0DxNDGNMw2TRDgtxMPf9x")),
        )
        store.upsert(edited)

        val again = HostStore(file)
        val c = again.contents.value
        assertEquals(listOf(b, edited), c.hosts)
        assertEquals(b.id, c.lastUsed)
        assertEquals("box.example", c.hosts[0].title)
        assertNull(again.storageError.value)

        again.delete(b.id)
        assertEquals(listOf(edited), HostStore(file).contents.value.hosts)
        assertNull(again.contents.value.lastUsed)
    }

    @Test
    fun aFileFromAnOlderVersionStillLoads() {
        val file = dir.root.resolve("hosts.json")
        file.writeText(
            """{"hosts":[{"id":"h1","name":"mini","target":{"host":"mini.example","user":"me"}}],""" +
                """"lastUsed":"h1","lastPanes":{"h1":{"session":"work","pane":"%3"}}}""",
        )
        val store = HostStore(file)
        assertEquals(listOf("h1"), store.contents.value.hosts.map { it.id })
        assertEquals("h1", store.contents.value.lastUsed)
        assertNull(store.storageError.value)
    }

    @Test
    fun aDamagedFileIsReportedNotOverwritten() {
        val file = dir.root.resolve("hosts.json")
        file.writeText("not json")
        val store = HostStore(file)
        assertTrue(store.contents.value.hosts.isEmpty())
        assertNotNull(store.storageError.value)
        assertEquals("not json", file.readText())
    }
}
