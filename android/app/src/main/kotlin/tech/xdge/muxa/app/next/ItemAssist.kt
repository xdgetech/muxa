package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.app.ui.PrimaryButton
import tech.xdge.muxa.app.ui.SecondaryButton
import tech.xdge.muxa.v2.ItemSuggestion
import tech.xdge.muxa.v2.SettingsOp
import tech.xdge.muxa.v2.SettingsProposal

// What a talk with a Bot (ADR 0083) and the Capture assistant (ADR 0084) add to an item.

/** One proposed op as a line, such as "Add Routine nightly-sweep (off)". */
fun proposalLine(op: SettingsOp): String = when (op) {
    is SettingsOp.RoutineAdd -> "Add Routine ${op.routine.name}" + if (op.routine.enabled) "" else " (off)"
    is SettingsOp.RoutineUpdate -> "Change Routine ${op.routine.name}"
    is SettingsOp.RoutineRemove -> "Remove Routine ${op.name}"
    is SettingsOp.BotAdd -> "Add Bot ${op.bot.name}"
    is SettingsOp.BotUpdate -> "Change Bot ${op.bot.name}"
    else -> op.op
}

/** The settings a Bot proposed. Apply sends them with the revision they were made against; [open] is false for a done item. */
@Composable
fun SettingsProposalBox(p: SettingsProposal, open: Boolean, busy: Boolean, apply: () -> Unit) {
    ItemBox("Settings proposal", Modifier.testTag("detail.settingsproposal")) {
        p.ops.forEachIndexed { i, op -> Text(proposalLine(op), Modifier.testTag("detail.settingsproposal.$i"), fontSize = 15.sp) }
        Text("Nothing changes until you apply it.", fontSize = 12.sp, color = Palette.textTertiary)
        if (open) PrimaryButton("Apply", apply, Modifier.testTag("detail.apply"), enabled = !busy)
    }
}

/** What the Capture assistant suggests, with Accept and Dismiss, and Start Task and Ask when it suggests them. */
@Composable
fun SuggestionBox(s: ItemSuggestion, answer: (accept: Boolean) -> Unit, startTask: (() -> Unit)?, ask: (() -> Unit)?) {
    ItemBox("Suggestion", Modifier.testTag("detail.suggestion")) {
        s.kind?.let { SuggestionFact("Kind", it.title, "suggestion.kind") }
        s.workspace?.let { SuggestionFact("Workspace", it, "suggestion.workspace") }
        s.due?.let { SuggestionFact("Due", dueText(it, NextModel.today()), "suggestion.due") }
        s.note?.let { SuggestionFact("Note", it, "suggestion.note") }
        if (s.split.isNotEmpty()) SuggestionFact("Split into", s.split.joinToString("\n") { "• $it" }, "suggestion.split")
        s.taskGoal?.let { SuggestionFact("Task", it, "suggestion.goal") }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            PrimaryButton("Accept", { answer(true) }, Modifier.testTag("suggestion.accept"))
            SecondaryButton("Dismiss", { answer(false) }, Modifier.testTag("suggestion.dismiss"))
            startTask?.let { SecondaryButton("Start Task", it, Modifier.testTag("suggestion.starttask")) }
            ask?.let { SecondaryButton("Ask ${s.bot}", it, Modifier.testTag("suggestion.ask")) }
        }
    }
}

@Composable
private fun SuggestionFact(label: String, value: String, tag: String) {
    Column {
        Text(label, fontSize = 12.sp, color = Palette.textTertiary)
        Text(value, Modifier.testTag(tag), fontSize = 15.sp)
    }
}

/** Ask one of the item's Workspace's [bots] about it: a talk whose goal is the item. */
@Composable
fun AskBotButton(bots: List<String>, enabled: Boolean, ask: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Column {
        SecondaryButton("Ask a Bot", { open = true }, Modifier.testTag("detail.ask"), enabled = enabled)
        DropdownMenu(open, { open = false }) {
            for (b in bots) {
                DropdownMenuItem({ Text(b) }, {
                    open = false
                    ask(b)
                }, Modifier.testTag("detail.ask.$b"))
            }
        }
    }
}
