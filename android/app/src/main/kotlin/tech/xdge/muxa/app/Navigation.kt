package tech.xdge.muxa.app

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.launch
import tech.xdge.muxa.protocol.InboxItem

// The sidebar's modes and the detail that follows the selection (ADR 0035).

/** What the sidebar holds. The device remembers it; each window keeps its own (ADR 0029). */
enum class SidebarMode(val title: String) {
    INBOX("Inbox"),
    TASKS("Tasks"),
    SESSIONS("Sessions");

    /** Its name in test tags and debug launches. */
    val tag: String get() = name.lowercase()

    companion object {
        fun of(tag: String?): SidebarMode? = entries.firstOrNull { it.tag == tag }
    }
}

/** What the detail shows when no pane is shown over it. A session's pane is [AppModel.paneShown]. */
sealed interface Selection {
    data class Item(val id: String) : Selection

    data class Task(val id: String) : Selection

    /** The Routines list, which Tasks mode opens from its first row. */
    data object Routines : Selection

    data class Routine(val key: String) : Selection
}

/** The mode the device last used, for the next window and the next launch. */
class ModeStore(private val prefs: SharedPreferences?) {
    constructor(context: Context) : this(context.getSharedPreferences("navigation", Context.MODE_PRIVATE))

    var mode: SidebarMode
        get() = SidebarMode.of(prefs?.getString(KEY, null)) ?: SidebarMode.INBOX
        set(value) {
            prefs?.edit()?.putString(KEY, value.tag)?.apply()
        }

    private companion object {
        const val KEY = "sidebar-mode"
    }
}

/**
 * Shows [new] in the sidebar. The detail goes to the mode's own, except a shown pane, which
 * stays. Inbox and Tasks read their lists again.
 */
fun AppModel.selectMode(new: SidebarMode) {
    if (mode != new) {
        mode = new
        selection = null
        if (new != SidebarMode.SESSIONS) paneShown = false
    }
    if (new != SidebarMode.SESSIONS) refreshWork()
}

/** A mode key: that mode, with the sidebar shown. */
fun AppModel.showMode(new: SidebarMode) {
    selectMode(new)
    listShown = true
}

/** Reads the Tasks and the Inbox again, as the lists open. */
fun AppModel.refreshWork() {
    if (!canShowTasks) return
    scope.launch {
        runCatching { session.loadTasks() }
        session.loadWork()
    }
}

/** Shows [next] in the detail; on a phone it is pushed over the list, elsewhere the list shows beside it. */
fun AppModel.select(next: Selection?) {
    selection = next
    paneShown = false
    listShown = !phoneLayout || next == null
}

/** Opens the Task's detail. */
fun AppModel.showTask(id: String) = select(Selection.Task(id))

fun AppModel.showRoutine(key: String) = select(Selection.Routine(key))

/**
 * Back on a phone: the pane returns to the screen it was opened from, a Routine to the
 * Routines, and any other screen to the list.
 */
fun AppModel.back() {
    val from = selection
    when {
        paneShown && from != null -> paneShown = false
        !paneShown && from is Selection.Routine -> selection = Selection.Routines
        else -> {
            if (!paneShown) selection = null
            listShown = true
        }
    }
    refocusPane()
}

/**
 * The window's layout changed, by a fold or a resize. A phone shows the list unless something
 * is selected; beside the list, the sidebar shows again unless a pane is shown.
 */
fun AppModel.layoutChanged(phone: Boolean) {
    val was = phoneLayout
    phoneLayout = phone
    if (was == phone) return
    listShown = if (phone) selection == null && !paneShown else listShown || !paneShown
}

/** True when the back gesture has somewhere to go: anything pushed over the list. */
val AppModel.canGoBack: Boolean get() = !listShown

/** The Inbox item whose session the shown pane is: its row stays highlighted. */
val AppModel.paneItem: InboxItem?
    get() {
        if (!paneShown) return null
        val s = currentSession ?: return null
        return inboxItems.firstOrNull { item -> item.pane?.let { p -> s.panes.any { it.id == p } } == true || (item.task != null && item.task == s.taskId) }
    }

/** The Task whose session the shown pane is. */
val AppModel.paneTask: String? get() = currentSession?.taskId?.takeIf { paneShown }
