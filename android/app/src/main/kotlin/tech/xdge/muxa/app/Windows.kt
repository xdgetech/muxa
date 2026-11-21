package tech.xdge.muxa.app

import tech.xdge.muxa.app.next.NextModel
import java.util.UUID

/**
 * One [NextModel] per app window, each with its own connection (ADR 0029).
 * Models outlive their Activity, so a fold, a resize or a recreation keeps the connection.
 * Only the main window's model outlives its window: the launcher opens it again.
 */
class Windows(private val create: () -> NextModel) {
    val main: NextModel = create()
    private val others = mutableMapOf<String, NextModel>()

    /** Windows with a live Activity, by id. */
    private val shown = mutableSetOf<String>()

    fun model(id: String): NextModel = if (id == MAIN) main else others.getOrPut(id, create)

    /**
     * The window an Activity shows: [requested], else the main one when no Activity shows
     * it, else a new one, as for the system's "new window" action.
     */
    fun claim(requested: String?): String {
        val id = requested ?: if (MAIN in shown) UUID.randomUUID().toString() else MAIN
        shown += id
        return id
    }

    /**
     * The window's Activity is gone. A window the user closed disconnects at once; the
     * main one waits for the grace period, as the whole app does.
     */
    fun closed(id: String, finished: Boolean): NextModel? {
        shown -= id
        if (!finished || id == MAIN) return null
        return others.remove(id)
    }

    /** Every model, for the app's foreground and background. */
    val all: List<NextModel> get() = listOf(main) + others.values

    companion object {
        const val MAIN = "main"
    }
}
