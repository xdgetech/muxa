package tech.xdge.muxa.transport

import kotlinx.coroutines.channels.ReceiveChannel
import java.io.ByteArrayOutputStream

/**
 * A running bridge process: protocol lines in, protocol lines out.
 * Implementations: [SshBridge] (the app), [ProcessBridge] (tests and tools on the host).
 */
interface BridgeChannel {
    /** Lines from the bridge, without the newline. Closes when the bridge exits. */
    val lines: ReceiveChannel<String>

    /** Writes one line; the newline is added here. */
    suspend fun send(line: String)

    /** Closes the channel. The client sends `bye` first. */
    suspend fun close()
}

sealed class TransportError(message: String) : Exception(message) {
    /** The bridge wrote a line longer than [LineFramer.MAX_LINE]. */
    class LineTooLong : TransportError("the bridge sent a line that is too long")
    class Closed : TransportError("the connection is closed")

    /** The host presented a key that does not match the pinned one, or no key is pinned yet. */
    class HostKeyMismatch(val presented: HostKey) : TransportError("the host key is not the trusted one")

    /** The host refused every login method offered: an unknown key or a wrong password. */
    class NotAuthorized : TransportError("the host refused the login")
    class Failed(message: String) : TransportError(message)
}

/** Splits a byte stream into lines. Bytes stay bytes until a whole line is there. */
class LineFramer {
    private val pending = ByteArrayOutputStream()

    fun push(bytes: ByteArray, length: Int = bytes.size): List<String> {
        val lines = mutableListOf<String>()
        var start = 0
        for (i in 0 until length) {
            if (bytes[i] == NEWLINE) {
                pending.write(bytes, start, i - start)
                lines += pending.toString(Charsets.UTF_8)
                pending.reset()
                start = i + 1
            }
        }
        pending.write(bytes, start, length - start)
        if (pending.size() > MAX_LINE) throw TransportError.LineTooLong()
        return lines
    }

    companion object {
        /** A `screen` of a large pane is about 1 MiB of base64. Far above that is a broken stream. */
        const val MAX_LINE = 32 shl 20
        private const val NEWLINE = '\n'.code.toByte()
    }
}
