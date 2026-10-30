package tech.xdge.muxa.session

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.delay
import tech.xdge.muxa.protocol.Bytes
import tech.xdge.muxa.protocol.ChangeInfo
import tech.xdge.muxa.protocol.DocInfo
import tech.xdge.muxa.protocol.ErrorBody
import tech.xdge.muxa.protocol.ErrorCode
import tech.xdge.muxa.protocol.ImageInfo
import tech.xdge.muxa.protocol.Message
import tech.xdge.muxa.protocol.Pane
import tech.xdge.muxa.protocol.Request
import tech.xdge.muxa.protocol.Role
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.Wire
import tech.xdge.muxa.transport.BridgeChannel
import java.util.concurrent.Executors

/** An in-memory bridge: answers every request, and records them in order. */
open class FakeBridge : BridgeChannel {
    private val out = Channel<String>(Channel.UNLIMITED)
    override val lines: ReceiveChannel<String> = out
    private val _received = mutableListOf<Request>()

    @Volatile
    var closed = false
        private set
    var protocolVersion = 1
    var sessions = listOf(
        Session(
            id = "$1", name = "demo",
            panes = listOf(Pane("%1", "@1", 0, Role.AGENT, 20, 5, "claude", "/w", false, false)),
        ),
    )
    var capabilities = listOf<String>()
    var docs = listOf<DocInfo>()
    var changes = listOf<ChangeInfo>()
    var images = listOf<ImageInfo>()

    /** Panes whose `chat` is answered with `unsupported`. */
    var unsupportedChats = setOf<String>()

    /** Screen data sent after each successful watch. */
    var screen = "\u001bchello".toByteArray()

    val received: List<Request> get() = synchronized(_received) { _received.toList() }

    fun start() = emit(Message.Hello(protocolVersion, "test", "3.7c", capabilities))

    fun emit(message: Message) {
        out.trySend(Wire.encode(message))
    }

    override suspend fun send(line: String) {
        val req = Wire.decodeRequest(line)
        synchronized(_received) { _received += req }
        answer(req)
    }

    protected open fun answer(req: Request) {
        when (req) {
            is Request.List -> emit(Message.Result(req.id, sessions = sessions))
            is Request.Watch -> {
                emit(Message.Result(req.id))
                emit(Message.Screen(req.id, req.pane, 20, 5, Bytes(screen)))
            }
            is Request.Docs -> {
                emit(Message.DocList(req.id, "app", docs))
                emit(Message.Result(req.id))
            }
            is Request.Changes -> {
                emit(Message.ChangeList(req.id, "app", changes))
                emit(Message.Result(req.id))
            }
            is Request.Images -> {
                emit(Message.ImageList(req.id, "app", images))
                emit(Message.Result(req.id))
            }
            is Request.NewTask -> {
                if (req.branch.startsWith("-")) {
                    emit(Message.Result(req.id, ErrorBody(ErrorCode.BAD_REQUEST, "bad branch")))
                    return
                }
                val name = req.branch.replace("/", "-")
                sessions = sessions + Session(id = "$9", name = name, panes = emptyList())
                emit(Message.TaskCreated(req.id, name))
                emit(Message.Result(req.id))
            }
            is Request.Chat -> if (req.pane in unsupportedChats) {
                emit(Message.Result(req.id, ErrorBody(ErrorCode.UNSUPPORTED, "no adapter")))
            } else {
                emit(Message.Result(req.id))
            }
            is Request.Bye -> out.close()
            else -> emit(Message.Result(req.id))
        }
    }

    override suspend fun close() {
        closed = true
        out.close()
    }
}

/** One thread, as the app's main thread is: what [PaneSession] needs. */
fun oneThread(): CoroutineDispatcher = Executors.newSingleThreadExecutor().asCoroutineDispatcher()

suspend fun waitUntil(timeoutMillis: Long = 5_000, condition: () -> Boolean) {
    val deadline = System.currentTimeMillis() + timeoutMillis
    while (!condition()) {
        if (System.currentTimeMillis() > deadline) throw AssertionError("condition not met in $timeoutMillis ms")
        delay(10)
    }
}
