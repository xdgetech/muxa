package tech.xdge.muxa.app.next

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CancellationException
import tech.xdge.muxa.v2.Settings
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.V2Error

/** Where the Settings screen reads and changes the host's settings (ADR 0074). */
interface SettingsHost {
    suspend fun settings(): Settings
    suspend fun settingsUpdate(revision: String, ops: List<SettingsOp>): Settings
}

/**
 * The Settings screen's copy of the host's settings. Each save sends its ops with the
 * revision it last read. It keeps no secret or env value: the forms hold them until a save.
 */
class SettingsEditor(private val host: SettingsHost) {
    var settings: Settings? by mutableStateOf(null)
        private set

    /** Why the settings could not be read. */
    var loadError: String? by mutableStateOf(null)
        private set

    /** What the host did to the last save: it changed meanwhile, or it locked its settings. */
    var notice: String? by mutableStateOf(null)
        private set
    var saving by mutableStateOf(false)
        private set

    /** The host refused an update as `read-only` since the last read. */
    private var refusedReadOnly by mutableStateOf(false)

    val readOnly: Boolean get() = settings?.readOnly == true || refusedReadOnly

    suspend fun load() {
        try {
            settings = host.settings()
            refusedReadOnly = false
            loadError = null
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            loadError = message(e)
        }
    }

    /** Applies [ops], all or none. Null when they were saved, else the message to show in the form. */
    suspend fun save(ops: List<SettingsOp>): String? {
        val current = settings ?: return "The settings are not loaded yet."
        if (readOnly) return LOCKED
        saving = true
        try {
            settings = host.settingsUpdate(current.revision, ops)
            notice = null
            return null
        } catch (e: CancellationException) {
            throw e
        } catch (e: V2Error.Refused) {
            return refused(e)
        } catch (e: Exception) {
            return message(e)
        } finally {
            saving = false
        }
    }

    fun dismissNotice() {
        notice = null
    }

    private suspend fun refused(e: V2Error.Refused): String = when (e.code) {
        CONFLICT -> {
            load()
            CHANGED.also { notice = it }
        }
        READ_ONLY -> {
            refusedReadOnly = true
            LOCKED.also { notice = it }
        }
        else -> message(e)
    }

    companion object {
        const val CHANGED = "The settings changed on the host. They are shown as they are now: check them and save again."
        const val LOCKED = "The host locked its settings. They can be read here but not changed."
        private const val CONFLICT = "conflict"
        private const val READ_ONLY = "read-only"

        /** The server's own words, without the code the client adds to them. */
        private fun message(e: Exception): String =
            if (e is V2Error.Refused) e.message?.removeSuffix(" (${e.code})") ?: e.code else NextModel.describe(e)
    }
}
