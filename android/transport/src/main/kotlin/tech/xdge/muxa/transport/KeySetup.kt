package tech.xdge.muxa.transport

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.IOException
import java.util.concurrent.TimeUnit

/**
 * One-time setup: logs in with the user's password and adds the device key to
 * `~/.ssh/authorized_keys`, restricted to the bridge. The password is used for this
 * one login and never stored.
 */
object KeySetup {
    enum class Outcome { ADDED, ALREADY_PRESENT }

    /**
     * The `authorized_keys` line for [key]: it may only run [command], with no PTY
     * and no forwarding (see SECURITY.md).
     */
    fun authorizedLine(key: ClientKey, command: String, comment: String): String {
        // The command sits inside a quoted option; a quote or line break would end it.
        if (command.isEmpty() || command.any { it == '"' || it == '\n' || it == '\r' }) {
            throw TransportError.Failed("the bridge command cannot contain quotes or line breaks")
        }
        return "command=\"$command\",no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding " +
            key.authorizedKey(comment)
    }

    /**
     * A POSIX shell script that appends [line] once. A new `~/.ssh` and file get
     * owner-only permissions; an existing file keeps its own. Prints `added` or `present`.
     */
    internal fun script(line: String): String {
        val quoted = "'" + line.replace("'", "'\\''") + "'"
        return "umask 077 && mkdir -p \"\$HOME/.ssh\" && f=\"\$HOME/.ssh/authorized_keys\" && touch \"\$f\" && l=$quoted && " +
            "if grep -qxF -- \"\$l\" \"\$f\"; then echo present; else " +
            "{ [ ! -s \"\$f\" ] || [ -z \"\$(tail -c 1 \"\$f\")\" ] || echo; printf '%s\\n' \"\$l\"; } >> \"\$f\" && echo added; fi"
    }

    internal fun outcome(output: String): Outcome = when (output.trim()) {
        "added" -> Outcome.ADDED
        "present" -> Outcome.ALREADY_PRESENT
        else -> throw TransportError.Failed("the host did not confirm the key was added")
    }

    /**
     * Logs in to [target] with [password] and adds [line]. The host key must already be
     * pinned; otherwise this fails with [TransportError.HostKeyMismatch] like [SshBridge.connect].
     */
    suspend fun install(line: String, target: SshTarget, password: String): Outcome = withContext(Dispatchers.IO) {
        val client = SshBridge.login(target) { it.authPassword(target.user, password) }
        try {
            client.startSession().use { session ->
                val command = session.exec(script(line))
                val output = readAtMost(command.inputStream, MAX_OUTPUT).toString(Charsets.UTF_8)
                command.join(15, TimeUnit.SECONDS)
                outcome(output)
            }
        } catch (e: IOException) {
            throw TransportError.Failed("could not update authorized_keys on ${target.host}: ${e.message}")
        } finally {
            runCatching { client.disconnect() }
        }
    }

    // `InputStream.readNBytes` needs API 33; the app runs from API 28.
    private fun readAtMost(input: java.io.InputStream, max: Int): ByteArray {
        val buf = ByteArray(max)
        var n = 0
        while (n < max) {
            val read = input.read(buf, n, max - n)
            if (read < 0) break
            n += read
        }
        return buf.copyOf(n)
    }

    private const val MAX_OUTPUT = 1024
}
