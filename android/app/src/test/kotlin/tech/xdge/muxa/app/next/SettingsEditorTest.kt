package tech.xdge.muxa.app.next

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.xdge.muxa.v2.Settings
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.V2Error

class SettingsEditorTest {
    /** A host whose next update fails with [refuse], else bumps the revision. */
    private class FakeHost(var current: Settings = Settings("1")) : SettingsHost {
        var refuse: V2Error.Refused? = null
        val sent = mutableListOf<Pair<String, List<SettingsOp>>>()

        override suspend fun settings() = current

        override suspend fun settingsUpdate(revision: String, ops: List<SettingsOp>): Settings {
            sent += revision to ops
            refuse?.let { throw it }
            current = current.copy(revision = (revision.toInt() + 1).toString())
            return current
        }
    }

    private val op = SettingsOp.WorkerRemove("reviewer")

    @Test
    fun aSaveSendsTheRevisionItReadAndKeepsTheReply() = runBlocking {
        val host = FakeHost()
        val editor = SettingsEditor(host).apply { load() }
        assertNull(editor.save(listOf(op)))
        assertNull(editor.save(listOf(op)))
        assertEquals(listOf("1", "2"), host.sent.map { it.first })
        assertEquals("3", editor.settings?.revision)
    }

    @Test
    fun aConflictReloadsAndSaysTheHostChanged() = runBlocking {
        val host = FakeHost()
        val editor = SettingsEditor(host).apply { load() }
        host.current = Settings("7")
        host.refuse = V2Error.Refused("conflict", "the settings changed; read them again")
        assertEquals(SettingsEditor.CHANGED, editor.save(listOf(op)))
        assertEquals("7", editor.settings?.revision)
        assertEquals(SettingsEditor.CHANGED, editor.notice)
    }

    @Test
    fun readOnlyLocksEditingWithoutSendingMore() = runBlocking {
        val host = FakeHost()
        val editor = SettingsEditor(host).apply { load() }
        host.refuse = V2Error.Refused("read-only", "the host locked its settings")
        assertEquals(SettingsEditor.LOCKED, editor.save(listOf(op)))
        assertTrue(editor.readOnly)
        assertEquals(SettingsEditor.LOCKED, editor.save(listOf(op)))
        assertEquals(1, host.sent.size)
    }

    @Test
    fun aViewThatSaysReadOnlyIsLockedFromTheStart() = runBlocking {
        val editor = SettingsEditor(FakeHost(Settings("1", readOnly = true))).apply { load() }
        assertTrue(editor.readOnly)
    }

    @Test
    fun aBadRequestShowsTheServersWordsAndKeepsTheSettings() = runBlocking {
        val host = FakeHost()
        val editor = SettingsEditor(host).apply { load() }
        host.refuse = V2Error.Refused("bad-request", "ops[0] worker-remove: no worker reviewer")
        assertEquals("ops[0] worker-remove: no worker reviewer", editor.save(listOf(op)))
        assertEquals("1", editor.settings?.revision)
        assertNull(editor.notice)
        assertFalse(editor.readOnly)
    }
}
