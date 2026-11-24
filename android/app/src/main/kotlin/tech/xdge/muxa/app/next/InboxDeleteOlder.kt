package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Box
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import tech.xdge.muxa.app.ui.DestructiveButton
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.LinkButton
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.v2.Inbox
import tech.xdge.muxa.v2.V2Request

/** The ages "Delete older than…" offers, in days. */
val DELETE_OLDER_DAYS = listOf(1, 3, 5)

private const val DAY_SECONDS = 86_400L

/** How many open and done items `inbox-delete-older` with [days] removes at [now] (Unix seconds), as the host decides it (ADR 0092). */
fun olderCount(inbox: Inbox, days: Int, now: Long): Int {
    val cutoff = now - days * DAY_SECONDS
    return (inbox.items + inbox.done).count { it.created < cutoff }
}

fun daysText(days: Int) = if (days == 1) "1 day" else "$days days"

/** The Inbox's More menu: Delete older than 1, 3 or 5 days, each after a confirmation. */
@Composable
fun InboxMoreMenu(model: NextModel) {
    var menu by remember { mutableStateOf(false) }
    var asking by remember { mutableStateOf<Int?>(null) }
    Box {
        IconButton({ menu = true }, Modifier.testTag("inbox.more")) {
            Icon(Icons.more, "More", tint = Palette.textSecondary)
        }
        DropdownMenu(menu, { menu = false }) {
            DropdownMenuItem({ Text("Delete older than…", color = Palette.textTertiary) }, {}, enabled = false)
            for (days in DELETE_OLDER_DAYS) {
                DropdownMenuItem({ Text(daysText(days)) }, {
                    menu = false
                    asking = days
                }, Modifier.testTag("inbox.older.$days"))
            }
        }
    }
    asking?.let { days ->
        DeleteOlderDialog(olderCount(model.inbox, days, System.currentTimeMillis() / 1000), days, {
            asking = null
            model.inbox(V2Request.InboxDeleteOlder(days))
        }) { asking = null }
    }
}

@Composable
private fun DeleteOlderDialog(count: Int, days: Int, confirm: () -> Unit, close: () -> Unit) {
    if (count == 0) {
        AlertDialog(
            onDismissRequest = close,
            modifier = Modifier.testTag("inbox.older.dialog"),
            text = { Text("No items are older than ${daysText(days)}.") },
            confirmButton = { PrimaryButton("OK", close, Modifier.testTag("inbox.older.ok")) },
        )
        return
    }
    AlertDialog(
        onDismissRequest = close,
        modifier = Modifier.testTag("inbox.older.dialog"),
        title = { Text("Delete ${if (count == 1) "1 item" else "$count items"} older than ${daysText(days)}?") },
        text = { Text("Open and Done items both go. This cannot be undone.") },
        confirmButton = { DestructiveButton("Delete", confirm, Modifier.testTag("inbox.older.confirm")) },
        dismissButton = { LinkButton("Cancel", close) },
    )
}
