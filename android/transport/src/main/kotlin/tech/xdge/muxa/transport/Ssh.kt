package tech.xdge.muxa.transport

import kotlinx.serialization.Serializable
import net.schmizz.sshj.DefaultConfig
import net.schmizz.sshj.SSHClient
import net.schmizz.sshj.common.Buffer
import net.schmizz.sshj.common.KeyType
import net.schmizz.sshj.transport.TransportException
import net.schmizz.sshj.transport.verification.HostKeyVerifier
import net.schmizz.sshj.userauth.UserAuthException
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

internal object SshLogin {
    private const val TIMEOUT_MS = 15_000

    /**
     * Connects to [target], checks its host key, and authenticates with [auth]. Fails with
     * [TransportError.HostKeyMismatch] when the host key is not the pinned one, or when none
     * is pinned yet: the caller shows its fingerprint, and connects again with it pinned once
     * the user accepts it.
     */
    fun login(target: SshTarget, auth: (SSHClient) -> Unit): SSHClient {
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
