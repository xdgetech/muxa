package tech.xdge.muxa.v2

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test

/** The client against the in-memory demo server. */
class DemoTest {
    private suspend fun open() = V2Client.open(DemoChannel(), "test")

    @Test
    fun aTerminalShowsItsScreenAndEchoes() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val shell = c.terminals().first { it.name == "shell" }
            val ch = c.attach(shell.id, 80, 24)
            assertEquals(V2Event.Paint(ch), c.events.receive())
            assertEquals(V2Event.Output(ch, "$ ".toByteArray()), c.events.receive())
            c.input(ch, "ls\r".toByteArray())
            assertEquals(V2Event.Output(ch, "ls\r\n".toByteArray()), c.events.receive())
            c.close()
        }
    }

    @Test
    fun aCaptureGoesOnTopAndWatchersSeeIt() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            c.watch()
            val capture = c.parseCapture("reply Sam #personal due tomorrow", "2026-11-25")
            assertEquals(Capture(ItemKind.REPLY, "Sam", "Personal", "2026-11-26"), capture)
            val inbox = c.inbox(V2Request.InboxAdd(capture))
            assertEquals("Sam", inbox.items.first().title)
            assertEquals(V2Event.InboxChanged(inbox), c.events.receive())
            val done = c.inbox(V2Request.InboxDone(inbox.items[0].id))
            assertEquals("Sam", done.done.first().title)
            c.close()
        }
    }

    @Test
    fun aRunningRoutineIsAConflict() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val (routines, _) = c.routines()
            assertEquals(listOf("dependency-digest", "nightly-backup", "release-notes"), routines.map { it.name })
            c.runRoutine("app", "release-notes")
            try {
                c.runRoutine("app", "release-notes")
                fail("release-notes already runs")
            } catch (e: V2Error.Refused) {
                assertEquals("conflict", e.code)
            }
            c.close()
        }
    }

    @Test
    fun aTaskStartsInItsPlaceOnce() = runBlocking {
        withTimeout(10_000) {
            val c = open()
            val (audit, existing) = c.startTask("app", "Audit", places = listOf(PlaceSpec.Read("/demo/app")))
            assertEquals(Triple("Audit", false, null), Triple(audit.name, existing, audit.branch))
            val fix = listOf(PlaceSpec.Branch("/demo/app", "fix-2"))
            val (first, _) = c.startTask("app", "Fix", places = fix)
            assertEquals("fix-2", first.branch)
            val (again, running) = c.startTask("app", "Fix", places = fix)
            assertEquals(first.id to true, again.id to running)
            c.close()
        }
    }

    @Test
    fun placesDecodeFromTheLabel() {
        val t = Terminal(
            "t9", "Audit", live = true, cols = 80, rows = 24,
            labels = mapOf("muxa.places" to """[{"kind":"folder","path":"/w/tasks/a"},{"kind":"read","path":"/w/docs"}]"""),
        )
        assertEquals(listOf(Place(PlaceKind.FOLDER, "/w/tasks/a"), Place(PlaceKind.READ, "/w/docs")), t.places)
    }
}
