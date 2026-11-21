package tech.xdge.muxa.v2

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import tech.xdge.muxa.transport.ByteChannel

/**
 * A protocol v2 connection (`protocol/PROTOCOL-v2.md`). It keeps what a resume
 * needs: the token, and per channel the next output offset and the input not
 * yet acknowledged. After [V2Event.Lost], open a new client with [state].
 */
class V2Client private constructor(private val channel: ByteChannel, initial: State) {
    /** What survives a lost connection. */
    data class State(val token: String? = null, val channels: Map<Int, ChannelState> = emptyMap()) {
        /** The channels a resume asks for. */
        val openChannels: List<Int> get() = channels.keys.sorted()
    }

    /** A channel's resume bookkeeping; clients only carry it to the next [open]. */
    data class ChannelState(
        val terminal: String,
        val cols: Int,
        val rows: Int,
        /** The next output offset expected. */
        val offset: Long = 0,
        val lastSeq: Long = 0,
        val acked: Long = 0,
        val pending: List<Pair<Long, ByteArray>> = emptyList(),
    )

    private class Waiter(val reply: CompletableDeferred<JsonObject>, val attach: ChannelState?)

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val lock = Mutex()
    private val outbox = Channel<ByteArray>(Channel.UNLIMITED)
    private val out = Channel<V2Event>(Channel.UNLIMITED)
    private var st = initial
    private var nextId = 0L
    private val waiting = mutableMapOf<Long, Waiter>()
    private val hello = CompletableDeferred<JsonObject>()
    private var gone: String? = null

    val events: ReceiveChannel<V2Event> = out

    /** The channels the server resumed, from `welcome`. */
    var resumed: List<Int> = emptyList()
        private set
    var backend: String = ""
        private set

    /** What a new client needs to resume this one. */
    suspend fun state(): State = lock.withLock { st }

    private suspend fun start(name: String) {
        scope.launch {
            try {
                for (bytes in outbox) channel.send(bytes)
            } catch (e: Exception) {
                lost("cannot write: ${e.message}")
            }
        }
        scope.launch {
            val reader = FrameReader()
            try {
                for (chunk in channel.bytes) reader.push(chunk).forEach { handle(it) }
                lost("the connection closed")
            } catch (e: Exception) {
                lost(e.message ?: e.toString())
            }
        }
        val resume: JsonElement = st.token?.let { token ->
            buildJsonObject {
                put("token", token)
                put("channels", buildJsonArray {
                    st.channels.forEach { (n, ch) ->
                        add(buildJsonObject {
                            put("channel", n); put("terminal", ch.terminal); put("offset", ch.offset)
                            put("input_seq", ch.acked); put("cols", ch.cols); put("rows", ch.rows)
                        })
                    }
                })
            }
        } ?: JsonNull
        val msg = buildJsonObject {
            put("type", "hello"); put("version", 2); put("client", name); put("resume", resume)
        }
        send(Frame.control(msg.toString()))
        hello.await()
    }

    suspend fun close() {
        outbox.close()
        out.close()
        channel.close()
        scope.cancel()
    }

    // Requests

    private suspend fun request(r: V2Request, attach: ChannelState? = null): JsonObject {
        val waiter = lock.withLock {
            gone?.let { throw V2Error.Lost(it) }
            nextId += 1
            val w = Waiter(CompletableDeferred(), attach)
            waiting[nextId] = w
            send(Frame.control(r.json(nextId).toString()))
            w
        }
        return waiter.reply.await()
    }

    suspend fun terminals(): List<Terminal> = field("terminals", request(V2Request.Terminals))

    /** Attaches and returns the channel. A `paint` and the screen follow. */
    suspend fun attach(terminal: String, cols: Int, rows: Int, history: Int = 0): Int =
        field("channel", request(V2Request.Attach(terminal, cols, rows, history), ChannelState(terminal, cols, rows)))

    suspend fun detach(channel: Int) {
        request(V2Request.Detach(channel))
        lock.withLock { st = st.copy(channels = st.channels - channel) }
    }

    /** The Task's agent terminal, and whether it was already running. */
    suspend fun startTask(
        workspace: String,
        title: String,
        goal: String? = null,
        places: List<PlaceSpec> = emptyList(),
        plan: Boolean = false,
    ): Pair<Terminal, Boolean> {
        val reply = request(V2Request.StartTask(workspace, title, goal, places, plan))
        return field<Terminal>("terminal", reply) to field<Boolean>("existing", reply)
    }

    /** Asks for `changed`, `removed`, `resync` and `inbox` events. */
    suspend fun watch() {
        request(V2Request.Watch)
    }

    suspend fun parseCapture(text: String, today: String): Capture = field("capture", request(V2Request.ParseCapture(text, today)))

    /** Any Inbox request; the reply is the whole Inbox. */
    suspend fun inbox(r: V2Request = V2Request.GetInbox): Inbox = field("inbox", request(r))

    /** The Routines, and the Workspaces whose Routines could not be read. */
    suspend fun routines(): Pair<List<Routine>, List<RoutineProblem>> {
        val reply = request(V2Request.Routines)
        return field<List<Routine>>("routines", reply) to field<List<RoutineProblem>>("problems", reply)
    }

    /** Runs a Routine now; [V2Error.Refused] with code `conflict` while it already runs. */
    suspend fun runRoutine(workspace: String, name: String) {
        request(V2Request.RunRoutine(workspace, name))
    }

    // Terminal channels

    /** Sends input; it is kept until the server acknowledges it. */
    suspend fun input(channel: Int, bytes: ByteArray) = lock.withLock {
        val ch = st.channels[channel] ?: throw V2Error.BadMessage("no channel $channel")
        val seq = ch.lastSeq + 1
        put(channel, ch.copy(lastSeq = seq, pending = ch.pending + (seq to bytes)))
        send(Frame.withU64(channel, seq, bytes))
    }

    suspend fun resize(channel: Int, cols: Int, rows: Int) = lock.withLock {
        st.channels[channel]?.let { put(channel, it.copy(cols = cols, rows = rows)) }
        send(Frame.resize(channel, cols, rows))
    }

    // Incoming

    private suspend fun handle(frame: Frame) = lock.withLock {
        val n = frame.channel
        when {
            n == 0 && frame.kind == FrameKind.DATA -> control(frame.payload.toString(Charsets.UTF_8))
            frame.kind == FrameKind.DATA -> output(n, frame)
            frame.kind == FrameKind.ACK -> {
                val seq = frame.splitU64()?.first ?: return@withLock
                st.channels[n]?.let { ch ->
                    put(n, ch.copy(pending = ch.pending.filter { it.first > seq }, acked = maxOf(ch.acked, seq)))
                }
            }
            frame.kind == FrameKind.CLOSE -> {
                st = st.copy(channels = st.channels - n)
                val exit = runCatching { json.decodeFromString<Exit>(frame.payload.toString(Charsets.UTF_8)) }.getOrNull()
                out.trySend(V2Event.Closed(n, exit))
            }
            else -> {}
        }
    }

    private fun output(n: Int, frame: Frame) {
        val (offset, bytes) = frame.splitU64() ?: return
        val ch = st.channels[n] ?: return
        // A resume may send again bytes this client already has.
        val skip = minOf(bytes.size.toLong(), maxOf(0L, ch.offset - offset)).toInt()
        put(n, ch.copy(offset = maxOf(ch.offset, offset + bytes.size)))
        send(Frame.credit(n, bytes.size))
        if (skip < bytes.size) out.trySend(V2Event.Output(n, bytes.copyOfRange(skip, bytes.size)))
    }

    private fun control(text: String) {
        val msg = runCatching { json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return
        val type = msg.str("type") ?: return
        val id = (msg["id"] as? JsonPrimitive)?.longOrNull
        when (type) {
            "welcome" -> {
                st = st.copy(token = msg.str("token"))
                backend = msg.str("backend") ?: ""
                resumed(msg["resumed"] as? JsonArray ?: JsonArray(emptyList()))
                hello.complete(msg)
            }
            "error" if id == null && !hello.isCompleted -> hello.completeExceptionally(refusal(msg))
            "reply", "error" -> {
                val w = id?.let { waiting.remove(it) } ?: return
                if (type == "error") return run { w.reply.completeExceptionally(refusal(msg)) }
                val n = (msg["channel"] as? JsonPrimitive)?.longOrNull?.toInt()
                if (w.attach != null && n != null) put(n, w.attach)
                w.reply.complete(msg)
            }
            "paint" -> {
                val n = (msg["channel"] as? JsonPrimitive)?.longOrNull?.toInt() ?: return
                st.channels[n]?.let { put(n, it.copy(offset = 0)) }
                out.trySend(V2Event.Paint(n))
            }
            "changed" -> runCatching { field<Terminal>("terminal", msg) }.getOrNull()?.let { out.trySend(V2Event.Changed(it)) }
            "removed" -> msg.str("terminal")?.let { out.trySend(V2Event.Removed(it)) }
            "resync" -> out.trySend(V2Event.Resync)
            "inbox" -> runCatching { field<Inbox>("inbox", msg) }.getOrNull()?.let { out.trySend(V2Event.InboxChanged(it)) }
        }
    }

    /** Resends input the server did not apply; ends channels it did not resume. */
    private fun resumed(listed: JsonArray) {
        val applied = listed.mapNotNull { e ->
            val o = e as? JsonObject ?: return@mapNotNull null
            val n = (o["channel"] as? JsonPrimitive)?.longOrNull?.toInt() ?: return@mapNotNull null
            n to (o["input_seq"] as? JsonPrimitive)?.longOrNull
        }.toMap()
        resumed = applied.keys.sorted()
        for (n in st.channels.keys.sorted()) {
            val ch = st.channels.getValue(n)
            if (n !in applied) {
                st = st.copy(channels = st.channels - n)
                out.trySend(V2Event.Closed(n, null))
                continue
            }
            val seq = applied[n]
            if (seq != null) {
                val left = ch.pending.filter { it.first > seq }
                put(n, ch.copy(pending = left, acked = maxOf(ch.acked, seq)))
                left.forEach { (s, b) -> send(Frame.withU64(n, s, b)) }
            } else {
                // The server cannot tell what it applied: never type twice.
                put(n, ch.copy(pending = emptyList()))
            }
        }
    }

    private suspend fun lost(why: String) = lock.withLock {
        if (gone != null) return@withLock
        gone = why
        hello.completeExceptionally(V2Error.Lost(why))
        waiting.values.forEach { it.reply.completeExceptionally(V2Error.Lost(why)) }
        waiting.clear()
        out.trySend(V2Event.Lost(why))
        out.close()
        outbox.close()
    }

    private fun put(n: Int, ch: ChannelState) {
        st = st.copy(channels = st.channels + (n to ch))
    }

    private fun send(frame: Frame) {
        outbox.trySend(frame.encode())
    }

    companion object {
        /** Says hello, with a resume when [state] has a token. */
        suspend fun open(channel: ByteChannel, name: String, state: State = State()): V2Client =
            V2Client(channel, state).also { it.start(name) }
    }
}

private fun JsonObject.str(key: String) = (this[key] as? JsonPrimitive)?.contentOrNull

private fun refusal(msg: JsonObject) = V2Error.Refused(msg.str("code") ?: "?", msg.str("message") ?: "")

/** [key] of a reply, decoded. */
internal inline fun <reified T> field(key: String, msg: JsonObject): T {
    val value = msg[key] ?: throw V2Error.BadMessage("no $key in the reply")
    return try {
        json.decodeFromJsonElement(value)
    } catch (e: Exception) {
        throw V2Error.BadMessage("$key: ${e.message}")
    }
}
