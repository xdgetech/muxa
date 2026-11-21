package tech.xdge.muxa.transport

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import net.schmizz.sshj.SSHClient
import net.schmizz.sshj.connection.channel.direct.Session
import net.schmizz.sshj.userauth.keyprovider.KeyPairWrapper
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream

/**
 * A running program as raw bytes both ways: protocol v2's transport.
 * Implementations: [SshExec] (the app), [ProcessBytes] (tests and tools on the host).
 */
interface ByteChannel {
    /** Bytes as they arrive. Closes when the program exits. */
    val bytes: ReceiveChannel<ByteArray>

    suspend fun send(data: ByteArray)

    suspend fun close()
}

/** Bytes from [input], writes to [output]; [onClose] ends the program. */
open class StreamBytes(
    private val input: InputStream,
    private val output: OutputStream,
    private val onClose: () -> Unit,
) : ByteChannel {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val out = Channel<ByteArray>(Channel.UNLIMITED)
    private val writeLock = Mutex()
    override val bytes: ReceiveChannel<ByteArray> = out

    init {
        scope.launch {
            val buf = ByteArray(64 * 1024)
            try {
                while (true) {
                    val n = runInterruptible { input.read(buf) }
                    if (n < 0) break
                    out.send(buf.copyOf(n))
                }
                out.close()
            } catch (e: Exception) {
                out.close(e)
            }
        }
    }

    override suspend fun send(data: ByteArray) {
        writeLock.withLock {
            withContext(Dispatchers.IO) {
                try {
                    output.write(data)
                    output.flush()
                } catch (_: IOException) {
                    throw TransportError.Closed()
                }
            }
        }
    }

    override suspend fun close() {
        withContext(Dispatchers.IO) {
            runCatching { output.close() }
            runCatching { onClose() }
        }
        scope.cancel()
    }
}

/** A local process as raw bytes, run with `/bin/sh -c`. For tests and tools. */
class ProcessBytes private constructor(private val process: Process) :
    StreamBytes(process.inputStream, process.outputStream, { if (process.isAlive) process.destroy() }) {
    constructor(command: String) : this(
        ProcessBuilder("/bin/sh", "-c", command).redirectError(ProcessBuilder.Redirect.DISCARD).start(),
    )
}

/** An SSH exec channel as raw bytes. No PTY: bytes pass through unchanged. */
class SshExec private constructor(
    client: SSHClient,
    session: Session,
    command: Session.Command,
) : StreamBytes(command.inputStream, command.outputStream, {
    runCatching { command.close() }
    runCatching { session.close() }
    runCatching { client.disconnect() }
}) {
    companion object {
        /** Connects and runs [command] (default: the target's). Host keys as [SshBridge.connect]. */
        suspend fun connect(target: SshTarget, key: ClientKey, command: String = target.command): SshExec =
            withContext(Dispatchers.IO) {
                val client = SshBridge.login(target) { it.authPublickey(target.user, KeyPairWrapper(key.keyPair())) }
                try {
                    val session = client.startSession()
                    val cmd = session.exec(command)
                    // stderr is the program's own log, not protocol. It is read so the
                    // window keeps moving; an unread stderr would stall stdout.
                    Thread { runCatching { cmd.errorStream.readBytes() } }.apply { isDaemon = true }.start()
                    SshExec(client, session, cmd)
                } catch (e: IOException) {
                    runCatching { client.disconnect() }
                    throw TransportError.Failed("cannot start $command on ${target.host}: ${e.message}")
                }
            }
    }
}
