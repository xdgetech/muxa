package tech.xdge.muxa.session

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import tech.xdge.muxa.transport.SshTarget
import java.io.File
import java.io.IOException
import java.util.UUID

/** A saved host: a name the user picked and how to reach the bridge there. */
@Serializable
data class Host(
    val id: String = UUID.randomUUID().toString(),
    val name: String,
    val target: SshTarget,
) {
    /** The name, or the address when the user left it empty. */
    val title: String get() = name.ifEmpty { target.host }
}

/**
 * The saved hosts, the last one used and each one's last pane, as JSON in one file.
 * Passwords are never stored; the pinned host key is public.
 */
class HostStore(private val file: File?) {
    @Serializable
    data class Contents(
        val hosts: List<Host> = emptyList(),
        val lastUsed: String? = null,
        /** The pane last shown on each host, opened first on the next connect. */
        val lastPanes: Map<String, LastPane> = emptyMap(),
    )

    private val _contents = MutableStateFlow(Contents())
    val contents: StateFlow<Contents> = _contents.asStateFlow()

    private val _storageError = MutableStateFlow<String?>(null)

    /** Why the last load or save failed, for the UI. Null when it worked. */
    val storageError: StateFlow<String?> = _storageError.asStateFlow()

    init {
        if (file != null && file.exists()) {
            try {
                _contents.value = json.decodeFromString(file.readText())
            } catch (e: IOException) {
                _storageError.value = "Saved hosts could not be read: ${e.message}"
            } catch (e: SerializationException) {
                _storageError.value = "Saved hosts could not be read: ${e.message}"
            }
        }
    }

    fun host(id: String?): Host? = _contents.value.hosts.firstOrNull { it.id == id }

    /** Adds [host], or replaces the saved one with the same id. */
    fun upsert(host: Host) = change { c ->
        val i = c.hosts.indexOfFirst { it.id == host.id }
        c.copy(hosts = if (i < 0) c.hosts + host else c.hosts.toMutableList().also { it[i] = host })
    }

    fun delete(id: String) = change { c ->
        c.copy(
            hosts = c.hosts.filter { it.id != id },
            lastUsed = c.lastUsed.takeIf { it != id },
            lastPanes = c.lastPanes - id,
        )
    }

    fun move(from: Int, to: Int) = change { c ->
        val list = c.hosts.toMutableList()
        list.add(to.coerceIn(0, list.size - 1), list.removeAt(from))
        c.copy(hosts = list)
    }

    fun markUsed(id: String) = change { c -> c.copy(lastUsed = id) }

    fun remember(pane: LastPane, id: String) = change { c -> c.copy(lastPanes = c.lastPanes + (id to pane)) }

    private fun change(edit: (Contents) -> Contents) {
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
            tmp.writeText(json.encodeToString(Contents.serializer(), _contents.value))
            if (!tmp.renameTo(file)) throw IOException("cannot replace ${file.name}")
            _storageError.value = null
        } catch (e: IOException) {
            _storageError.value = "Hosts could not be saved: ${e.message}"
        }
    }

    private companion object {
        val json = Json { ignoreUnknownKeys = true }
    }
}
