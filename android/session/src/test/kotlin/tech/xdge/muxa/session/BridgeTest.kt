package tech.xdge.muxa.session

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Test
import tech.xdge.muxa.terminal.PaneScreen
import tech.xdge.muxa.transport.ProcessBridge
import java.io.File
import kotlin.random.Random

/**
 * [PaneSession] against the real bridge on a throwaway server (`-L muxa-test-android-*`).
 * Opt-in: `make test-android-tmux` sets MUXA_BRIDGE to a built muxa-bridge.
 */
class BridgeTest {
    private val bridge = System.getenv("MUXA_BRIDGE")
    private val socket = "muxa-test-android-${ProcessHandle.current().pid()}-${Random.nextInt(1_000_000)}"

    private fun mt(vararg args: String): String {
        // Always an isolated server (`-L`), never the developer's default one.
        val p = ProcessBuilder(listOf("tmux", "-L", socket, "-f", "/dev/null") + args).apply {
            environment().remove("TMUX")
            environment()["PATH"] = "/opt/homebrew/bin:/usr/local/bin:" + (environment()["PATH"] ?: "/usr/bin:/bin")
        }.redirectErrorStream(true).start()
        val out = p.inputStream.readBytes().toString(Charsets.UTF_8).trim()
        p.waitFor()
        return out
    }

    @Test
    fun watchTypeAndLeaveTheLayoutAlone() = runBlocking {
        assumeTrue("set MUXA_BRIDGE to run", bridge != null)
        mt("new-session", "-d", "-s", "t", "-x", "40", "-y", "6", "printf 'ready\\n'; exec cat")
        val path = mt("display-message", "-p", "#{socket_path}")
        try {
            val before = mt("display-message", "-p", "-t", "t", "#{window_width}x#{window_height}")
            val session = PaneSession(oneThread())
            val screen = PaneScreen()
            session.renderer = screen
            session.connect { ProcessBridge("'$bridge' --socket '$socket'") }
            val pane = session.state.value.sessions.first().panes.first()

            session.watch(pane.id)
            waitUntil { screen.awaitIdle(); screen.rows().firstOrNull() == "ready" }
            session.send("hi")
            session.send(listOf("Enter"))
            // The tty echoes "hi", then cat prints it again.
            waitUntil { screen.awaitIdle(); screen.rows() == listOf("ready", "hi", "hi") }
            assertEquals(pane.cols, screen.snapshot().cols)

            session.disconnect()
            assertEquals(before, mt("display-message", "-p", "-t", "t", "#{window_width}x#{window_height}"))
        } finally {
            mt("kill-server")
            // The server leaves its socket file behind.
            File(path).delete()
        }
    }
}
