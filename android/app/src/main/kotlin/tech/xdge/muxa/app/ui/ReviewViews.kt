package tech.xdge.muxa.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import tech.xdge.muxa.protocol.InboxItem
import tech.xdge.muxa.protocol.ItemKind

// A request's Result in the Inbox (ADR 0058 §7): the user reviews it here without opening the
// Work item. The summary above it is the item's; its links are the item's evidence below.

/** A `result` item's Findings, Open Questions, Next Steps and Files, as plain selectable text. */
@Composable
internal fun ResultReviewView(item: InboxItem) {
    if (item.kind != ItemKind.RESULT) return
    val r = item.result ?: return
    SelectionContainer {
        Column(Modifier.testTag("item-result"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            DetailNote(listOfNotNull(r.worker, "v${r.version}", "step ${r.step}".takeIf { r.worker != null }).joinToString(" · "))
            Column(Modifier.testTag("item-result-findings")) { Bullets("Findings", r.findings) }
            Column(Modifier.testTag("item-result-questions")) { Bullets("Open Questions", r.openQuestions) }
            Column(Modifier.testTag("item-result-next")) { Bullets("Next Steps", r.next) }
            Column(Modifier.testTag("item-result-files")) { Bullets("Files", r.files) }
        }
    }
}
