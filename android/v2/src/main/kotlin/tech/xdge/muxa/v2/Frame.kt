package tech.xdge.muxa.v2

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Protocol v2 framing: `[channel u32 LE][kind u8][len u32 LE][payload]`. */
enum class FrameKind(val code: Int) {
    DATA(0), CREDIT(1), CLOSE(2), RESIZE(3), ACK(4);

    companion object {
        fun of(code: Int) = entries.firstOrNull { it.code == code }
    }
}

sealed class FrameError(message: String) : Exception(message) {
    class TooLarge(val size: Long) : FrameError("frame payload of $size bytes is over 16 MiB")
    class UnknownKind(val kind: Int) : FrameError("unknown frame kind $kind")
}

class Frame(val channel: Int, val kind: FrameKind, val payload: ByteArray) {
    fun encode(): ByteArray =
        le(HEADER + payload.size).putInt(channel).put(kind.code.toByte()).putInt(payload.size).put(payload).array()

    /** The u64 prefix of an output, input or ack payload, and the rest. */
    fun splitU64(): Pair<Long, ByteArray>? {
        if (payload.size < 8) return null
        return ByteBuffer.wrap(payload, 0, 8).order(ByteOrder.LITTLE_ENDIAN).long to payload.copyOfRange(8, payload.size)
    }

    override fun equals(other: Any?) =
        other is Frame && channel == other.channel && kind == other.kind && payload.contentEquals(other.payload)

    override fun hashCode() = 31 * (31 * channel + kind.hashCode()) + payload.contentHashCode()

    companion object {
        const val HEADER = 9
        const val MAX_PAYLOAD = 16 shl 20

        fun control(json: String) = Frame(0, FrameKind.DATA, json.toByteArray(Charsets.UTF_8))

        /** `[offset or seq u64 LE][bytes]`: terminal output, or input. */
        fun withU64(channel: Int, n: Long, bytes: ByteArray) =
            Frame(channel, FrameKind.DATA, le(8 + bytes.size).putLong(n).put(bytes).array())

        fun credit(channel: Int, bytes: Int) = Frame(channel, FrameKind.CREDIT, le(4).putInt(bytes).array())

        fun resize(channel: Int, cols: Int, rows: Int) =
            Frame(channel, FrameKind.RESIZE, le(4).putShort(cols.toShort()).putShort(rows.toShort()).array())

        fun ack(channel: Int, seq: Long) = Frame(channel, FrameKind.ACK, le(8).putLong(seq).array())

        private fun le(size: Int) = ByteBuffer.allocate(size).order(ByteOrder.LITTLE_ENDIAN)
    }
}

/** Collects bytes and gives out whole frames. */
class FrameReader {
    private val pending = ByteArrayOutputStream()

    fun push(bytes: ByteArray): List<Frame> {
        pending.write(bytes)
        var buf = pending.toByteArray()
        val frames = mutableListOf<Frame>()
        while (buf.size >= Frame.HEADER) {
            val head = ByteBuffer.wrap(buf, 0, Frame.HEADER).order(ByteOrder.LITTLE_ENDIAN)
            val channel = head.int
            val code = head.get().toInt() and 0xff
            val len = head.int.toLong() and 0xffffffffL
            if (len > Frame.MAX_PAYLOAD) throw FrameError.TooLarge(len)
            val kind = FrameKind.of(code) ?: throw FrameError.UnknownKind(code)
            val end = Frame.HEADER + len.toInt()
            if (buf.size < end) break
            frames += Frame(channel, kind, buf.copyOfRange(Frame.HEADER, end))
            buf = buf.copyOfRange(end, buf.size)
        }
        pending.reset()
        pending.write(buf)
        return frames
    }
}
