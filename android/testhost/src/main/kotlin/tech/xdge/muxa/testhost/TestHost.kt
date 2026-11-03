package tech.xdge.muxa.testhost

import org.apache.sshd.common.keyprovider.KeyPairProvider
import org.apache.sshd.server.Environment
import org.apache.sshd.server.ExitCallback
import org.apache.sshd.server.SshServer
import org.apache.sshd.server.auth.pubkey.AcceptAllPublickeyAuthenticator
import org.apache.sshd.server.channel.ChannelSession
import org.apache.sshd.server.command.Command
import org.apache.sshd.server.command.CommandFactory
import java.io.InputStream
import java.io.OutputStream
import java.security.KeyPairGenerator
import kotlin.concurrent.thread
import kotlin.system.exitProcess

/**
 * A throwaway SSH host for the release-build test (scripts/test-android-release.sh).
 * It listens on 127.0.0.1 only, with a new host key each run, and accepts any client
 * key. Every exec runs the given bridge command instead of the requested one, as a
 * `command=` entry in `authorized_keys` does. It has no shell and no password login.
 *
 * Usage: testhost PORT BRIDGE [ARGS...]
 */
fun main(args: Array<String>) {
    val port = args.firstOrNull()?.toIntOrNull()
    val bridge = args.drop(1)
    if (port == null || bridge.isEmpty()) {
        System.err.println("usage: testhost PORT BRIDGE [ARGS...]")
        exitProcess(2)
    }
    val hostKey = KeyPairGenerator.getInstance("EC").apply { initialize(256) }.generateKeyPair()
    val server = SshServer.setUpDefaultServer().apply {
        host = "127.0.0.1"
        this.port = port
        keyPairProvider = KeyPairProvider.wrap(hostKey)
        publickeyAuthenticator = AcceptAllPublickeyAuthenticator.INSTANCE
        commandFactory = CommandFactory { _, _ -> BridgeCommand(bridge) }
    }
    server.start()
    println("listening on 127.0.0.1:$port")
    Thread.currentThread().join()
}

/** The bridge as a child process, its streams copied byte for byte. */
private class BridgeCommand(private val command: List<String>) : Command {
    private lateinit var input: InputStream
    private lateinit var output: OutputStream
    private lateinit var error: OutputStream
    private var exit: ExitCallback? = null
    private var process: Process? = null

    override fun setInputStream(stream: InputStream) { input = stream }
    override fun setOutputStream(stream: OutputStream) { output = stream }
    override fun setErrorStream(stream: OutputStream) { error = stream }
    override fun setExitCallback(callback: ExitCallback) { exit = callback }

    override fun start(channel: ChannelSession, env: Environment) {
        val p = ProcessBuilder(command).start()
        process = p
        thread(isDaemon = true) { copy(input, p.outputStream, closeTarget = true) }
        val err = thread(isDaemon = true) { copy(p.errorStream, error) }
        thread(isDaemon = true) {
            copy(p.inputStream, output)
            err.join()
            exit?.onExit(p.waitFor())
        }
    }

    override fun destroy(channel: ChannelSession) {
        process?.destroy()
    }

    private fun copy(from: InputStream, to: OutputStream, closeTarget: Boolean = false) {
        val buf = ByteArray(64 * 1024)
        try {
            while (true) {
                val n = from.read(buf)
                if (n < 0) break
                to.write(buf, 0, n)
                to.flush()
            }
        } catch (e: java.io.IOException) {
            // The channel or the process closed; the exit callback reports the end.
        } finally {
            if (closeTarget) runCatching { to.close() }
        }
    }
}
