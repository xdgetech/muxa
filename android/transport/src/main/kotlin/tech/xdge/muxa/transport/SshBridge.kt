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
import kotlinx.serialization.Serializable
import net.schmizz.sshj.DefaultConfig
import net.schmizz.sshj.SSHClient
import net.schmizz.sshj.common.Buffer
import net.schmizz.sshj.common.KeyType
import net.schmizz.sshj.connection.channel.direct.Session
import net.schmizz.sshj.transport.TransportException
import net.schmizz.sshj.transport.verification.HostKeyVerifier
import net.schmizz.sshj.userauth.UserAuthException
import net.schmizz.sshj.userauth.keyprovider.KeyPairWrapper
import java.io.IOException
import java.security.PublicKey
import java.util.concurrent.atomic.AtomicReference

/** Where and how to reach the bridge. */
@Serializable
data class SshTarget(
    val host: String,
    val port: Int = 22,
    val user: String,
    /** The exec command. With a restricted `authorized_keys` entry the server ignores it. */
    val command: String = DEFAULT_COMMAND,
    /** The host key accepted on first use. Null until the user has confirmed one. */
    val hostKey: HostKey? = null,
) {
    companion object {
        const val DEFAULT_COMMAND = "~/.local/bin/muxa-bridge"
    }
}

/** The bridge as an SSH exec channel. No PTY: bytes pass through unchanged. */
class SshBridge private constructor(
    private val client: SSHClient,
    private val session: Session,
    private val command: Session.Command,
) : BridgeChannel {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val out = Channel<String>(Channel.UNLIMITED)
    private val writeLock = Mutex()
    override val lines: ReceiveChannel<String> = out

    private fun start() {
        scope.launch {
            val framer = LineFramer()
            val buf = ByteArray(64 * 1024)
            try {
                val input = command.inputStream
                while (true) {
                    val n = runInterruptible { input.read(buf) }
                    if (n < 0) break
                    framer.push(buf, n).forEach { out.send(it) }
                }
                out.close()
            } catch (e: Exception) {
                out.close(e)
            }
        }
        // stderr is the bridge's own log, not protocol. It is read so the channel's
        // window keeps moving; an unread stderr would stall stdout.
        scope.launch {
            val buf = ByteArray(8 * 1024)
            runCatching {
                while (command.errorStream.read(buf) >= 0) {
                    // Dropped.
                }
            }
        }
    }

    override suspend fun send(line: String) {
        val bytes = (line + "\n").toByteArray(Charsets.UTF_8)
        writeLock.withLock {
            withContext(Dispatchers.IO) {
                try {
                    command.outputStream.write(bytes)
                    command.outputStream.flush()
                } catch (_: IOException) {
                    throw TransportError.Closed()
                }
            }
        }
    }

    override suspend fun close() {
        withContext(Dispatchers.IO) {
            runCatching { command.close() }
            runCatching { session.close() }
            runCatching { client.disconnect() }
        }
        scope.cancel()
    }

    companion object {
        private const val TIMEOUT_MS = 15_000

        /**
         * Connects and starts the bridge. Fails with [TransportError.HostKeyMismatch] when the
         * host key is not the pinned one, or when none is pinned yet: the caller shows its
         * fingerprint, and connects again with it pinned once the user accepts it.
         */
        suspend fun connect(target: SshTarget, key: ClientKey): SshBridge = withContext(Dispatchers.IO) {
            val client = login(target) { it.authPublickey(target.user, KeyPairWrapper(key.keyPair())) }
            try {
                val session = client.startSession()
                val command = session.exec(target.command)
                SshBridge(client, session, command).also { it.start() }
            } catch (e: IOException) {
                runCatching { client.disconnect() }
                throw TransportError.Failed("cannot start the bridge on ${target.host}: ${e.message}")
            }
        }

        /** Connects to [target], checks its host key, and authenticates with [auth]. */
        internal fun login(target: SshTarget, auth: (SSHClient) -> Unit): SSHClient {
            val check = HostKeyCheck(target.hostKey)
            val client = SSHClient(DefaultConfig())
            client.connectTimeout = TIMEOUT_MS
            client.timeout = 0
            client.addHostKeyVerifier(check)
            try {
                client.connect(target.host, target.port)
                auth(client)
                return client
            } catch (e: Exception) {
                runCatching { client.disconnect() }
                check.rejected.get()?.let { throw TransportError.HostKeyMismatch(it) }
                if (e is UserAuthException) throw TransportError.NotAuthorized()
                if (e is TransportException || e is IOException) {
                    throw TransportError.Failed("cannot connect to ${target.host}: ${e.message}")
                }
                throw e
            }
        }
    }
}

/** Accepts only the pinned host key, and remembers a rejected one for the caller. */
internal class HostKeyCheck(private val pinned: HostKey?) : HostKeyVerifier {
    val rejected = AtomicReference<HostKey?>(null)

    override fun verify(hostname: String, port: Int, key: PublicKey): Boolean {
        val presented = HostKey.of(KeyType.fromKey(key).toString(), Buffer.PlainBuffer().putPublicKey(key).compactData)
        if (presented == pinned) return true
        rejected.set(presented)
        return false
    }

    override fun findExistingAlgorithms(hostname: String, port: Int): List<String> =
        pinned?.let { listOf(it.openSSH.substringBefore(' ')) } ?: emptyList()
}
