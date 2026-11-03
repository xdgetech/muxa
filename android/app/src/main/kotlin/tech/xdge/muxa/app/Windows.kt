package tech.xdge.muxa.app

import android.content.Context
import android.content.Intent
import java.util.UUID

/**
 * One [AppModel] per app window, each with its own connection, pane and keyboard (ADR 0029).
 * Models outlive their Activity, so a fold, a resize or a recreation keeps the connection.
 * Only the main window's model outlives its window: the launcher opens it again.
 */
class Windows(private val create: () -> AppModel) {
    val main: AppModel = create()
    private val others = mutableMapOf<String, AppModel>()

    /** Windows with a live Activity, by id. */
    private val shown = mutableSetOf<String>()

    fun model(id: String): AppModel = if (id == MAIN) main else others.getOrPut(id, create)

    /** True when [id] has a model already, so it keeps its connection and pane. */
    fun has(id: String): Boolean = id == MAIN || id in others

    /**
     * The window an Activity shows: [requested], else the main one when no Activity shows
     * it, else a new one, as for the system's "new window" action.
     */
    fun claim(requested: String?): String {
        val id = requested ?: if (MAIN in shown) UUID.randomUUID().toString() else MAIN
        shown += id
        return id
    }

    /** A new window that starts as [start] says. Its Activity claims it by the returned id. */
    fun open(start: AppModel.WindowStart): String {
        val id = UUID.randomUUID().toString()
        model(id).begin(start)
        return id
    }

    /**
     * The window's Activity is gone. A window the user closed disconnects at once; the
     * main one waits for the grace period, as the whole app does.
     */
    fun closed(id: String, finished: Boolean): AppModel? {
        shown -= id
        if (!finished || id == MAIN) return null
        return others.remove(id)
    }

    /** Every model, for the app's foreground and background. */
    val all: List<AppModel> get() = listOf(main) + others.values

    companion object {
        const val MAIN = "main"
    }
}

/**
 * Opens [model]'s pane in a new window: beside this one when the screen has room
 * (split screen or desktop windows), else as its own task in Recents.
 */
fun Context.openInNewWindow(model: AppModel) {
    val start = model.windowStart() ?: return
    val app = applicationContext as MuxaApplication
    val id = app.windows.open(start)
    val intent = Intent(this, MainActivity::class.java)
        .putExtra(MainActivity.EXTRA_WINDOW, id)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_MULTIPLE_TASK or Intent.FLAG_ACTIVITY_LAUNCH_ADJACENT)
    startActivity(intent)
}
