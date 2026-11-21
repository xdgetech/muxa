package tech.xdge.muxa.v2

import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import tech.xdge.muxa.transport.ProcessBytes
import java.io.File

/**
 * Against the real server on the fake backend: `make test-android-v2` sets
 * `muxa.v2server` to the built `fake_server` example.
 */
class ServerTest {
    private val program: String? = System.getProperty("muxa.v2server")
    private lateinit var server: Process
    private lateinit var socket: File

    @Before
    fun start() {
        assumeTrue("set MUXA_TEST_V2_SERVER", program != null)
        socket = File.createTempFile("muxa-v2-", ".sock").also { it.delete() }
        server = ProcessBuilder(program, socket.path).redirectOutput(ProcessBuilder.Redirect.DISCARD).start()
        repeat(100) { if (!socket.exists()) Thread.sleep(50) }
    }

    @After
    fun stop() {
        if (::server.isInitialized) server.destroy()
        if (::socket.isInitialized) socket.delete()
    }

    private suspend fun connect(state: V2Client.State = V2Client.State()) =
        V2Client.open(ProcessBytes("nc -U ${socket.path}"), "test", state)

    private suspend fun V2Client.next(skip: (V2Event) -> Boolean = { false }): V2Event {
        while (true) {
            val e = events.receive()
            if (!skip(e)) return e
        }
    }

    @Test
    fun attachShowsTheScreenAndTypes() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val demo = c.terminals().first { it.name == "demo" }
            val ch = c.attach(demo.id, 80, 24)
            assertEquals(V2Event.Paint(ch), c.next())
            assertEquals(V2Event.Output(ch, "hello ".toByteArray()), c.next())
            c.input(ch, "x".toByteArray())
            assertEquals(V2Event.Output(ch, "echo:x".toByteArray()), c.next())
            c.close()
        }
    }

    @Test
    fun aResumeContinuesWithoutAPaint() = runBlocking {
        withTimeout(10_000) {
            val first = connect()
            val demo = first.terminals().first { it.name == "demo" }
            val ch = first.attach(demo.id, 80, 24)
            first.next()
            first.next()
            val state = first.state()
            first.close()
            delay(200)
            val again = connect(state)
            assertEquals(listOf(ch), again.resumed)
            again.input(ch, "y".toByteArray())
            // No paint and no `hello ` again: only the new output.
            assertEquals(V2Event.Output(ch, "echo:y".toByteArray()), again.next())
            again.close()
        }
    }

    @Test
    fun aCaptureReachesAWatchingDevice() = runBlocking {
        withTimeout(10_000) {
            val phone = connect()
            phone.watch()
            val mac = connect()
            val capture = mac.parseCapture("reply to the bank #personal due 12/1", "2026-11-25")
            assertEquals(Capture(ItemKind.REPLY, "to the bank", "Personal", "2026-12-01"), capture)
            val inbox = mac.inbox(V2Request.InboxAdd(capture))
            assertEquals(listOf("to the bank"), inbox.items.map { it.title })
            val pushed = phone.next { it !is V2Event.InboxChanged }
            assertEquals(V2Event.InboxChanged(inbox), pushed)
            try {
                mac.inbox(V2Request.InboxDone("nope"))
                fail("done on a missing item")
            } catch (e: V2Error.Refused) {
                assertEquals("not-found", e.code)
            }
            assertTrue(phone.resumed.isEmpty())
            phone.close()
            mac.close()
        }
    }

    @Test
    fun routinesListAndRunOnce() = runBlocking {
        withTimeout(10_000) {
            val c = connect()
            val (routines, problems) = c.routines()
            assertEquals(listOf("digest", "audit"), routines.map { it.name })
            assertEquals("09:00", routines[0].at)
            assertTrue(problems.isEmpty())
            c.runRoutine("Personal", "audit")
            try {
                c.runRoutine("Personal", "digest")
                fail("digest already runs")
            } catch (e: V2Error.Refused) {
                assertEquals("conflict", e.code)
            }
            c.close()
        }
    }
}
