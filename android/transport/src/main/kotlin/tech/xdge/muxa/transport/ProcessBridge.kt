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
import java.io.IOException

/** The bridge as a local process, run with `/bin/sh -c`. For tests and tools on the host. */
class ProcessBridge(command: String) : BridgeChannel {
    private val process = ProcessBuilder("/bin/sh", "-c", command)
        .redirectError(ProcessBuilder.Redirect.DISCARD)
        .start()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val out = Channel<String>(Channel.UNLIMITED)
    private val writeLock = Mutex()
    override val lines: ReceiveChannel<String> = out

    init {
        scope.launch {
            val framer = LineFramer()
            val buf = ByteArray(64 * 1024)
            try {
                while (true) {
                    val n = runInterruptible { process.inputStream.read(buf) }
                    if (n < 0) break
                    framer.push(buf, n).forEach { out.send(it) }
                }
                out.close()
            } catch (e: Exception) {
                out.close(e)
            }
        }
    }

    override suspend fun send(line: String) {
        writeLock.withLock {
            withContext(Dispatchers.IO) {
                try {
                    process.outputStream.write((line + "\n").toByteArray(Charsets.UTF_8))
                    process.outputStream.flush()
                } catch (_: IOException) {
                    throw TransportError.Closed()
                }
            }
        }
    }

    override suspend fun close() {
        withContext(Dispatchers.IO) {
            runCatching { process.outputStream.close() }
            if (process.isAlive) process.destroy()
        }
        scope.cancel()
    }
}
