package tech.xdge.muxa.session

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import java.io.File
import java.io.IOException

/**
 * A capture the host has not confirmed (ADR 0046 §10). It keeps its [clientId], so a resend
 * returns the Capture the host may already have stored instead of making a second one.
 */
@Serializable
data class UnsentCapture(
    val clientId: String,
    /** The saved host it was made for, by id. */
    val host: String,
    val text: String,
    val workspace: String? = null,
    /** Epoch seconds. */
    val createdAt: Long,
    /** The host refused the last send, with this reason; null while it waits for a connection. */
    val error: String? = null,
)

/**
 * The device's unsent captures, as JSON in one file, shared by every window. The text is
 * the user's and stays in this file only: it is never logged.
 */
class UnsentCaptures(private val file: File?) {
    private val _contents = MutableStateFlow<List<UnsentCapture>>(emptyList())

    /** Oldest first. */
    val contents: StateFlow<List<UnsentCapture>> = _contents.asStateFlow()

    private val _storageError = MutableStateFlow<String?>(null)

    /** Why the last load or save failed, for the UI. Null when it worked. */
    val storageError: StateFlow<String?> = _storageError.asStateFlow()

    init {
        if (file != null && file.exists()) {
            try {
                _contents.value = json.decodeFromString(file.readText())
            } catch (e: IOException) {
                _storageError.value = "Unsent captures could not be read: ${e.message}"
            } catch (e: SerializationException) {
                _storageError.value = "Unsent captures could not be read: ${e.message}"
            }
        }
    }

    fun forHost(host: String?): List<UnsentCapture> = _contents.value.filter { it.host == host }

    /** Keeps [capture], replacing one with the same `clientId`. */
    fun add(capture: UnsentCapture) = change { list -> list.filter { it.clientId != capture.clientId } + capture }

    fun remove(clientId: String) = change { list -> list.filter { it.clientId != clientId } }

    /** The host refused it; it stays, with [reason], until the user sends it again or deletes it. */
    fun refused(clientId: String, reason: String) = change { list ->
        list.map { if (it.clientId == clientId) it.copy(error = reason) else it }
    }

    private fun change(edit: (List<UnsentCapture>) -> List<UnsentCapture>) {
        val before = _contents.value
        _contents.update(edit)
        if (_contents.value != before) save()
    }

    private fun save() {
        val file = file ?: return
        try {
            file.parentFile?.mkdirs()
            // Written beside the file and moved over it, so a crash never leaves half a file.
            val tmp = File(file.parentFile, file.name + ".tmp")
            tmp.writeText(json.encodeToString(_contents.value))
            if (!tmp.renameTo(file)) throw IOException("cannot replace ${file.name}")
            _storageError.value = null
        } catch (e: IOException) {
            _storageError.value = "Unsent captures could not be saved: ${e.message}"
        }
    }

    private companion object {
        val json = Json { ignoreUnknownKeys = true }
    }
}

/** What a failed send means for a capture: null to keep waiting for a connection, else the host's refusal. */
fun unsentReason(error: Throwable): String? = when (error) {
    is PaneSession.Failure.Bridge, is PaneSession.Failure.WorkChanged -> PaneSession.describe(error)
    else -> null
}
