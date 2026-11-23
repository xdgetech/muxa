package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.KindPalette
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.v2.ItemLink
import tech.xdge.muxa.v2.ItemProposal

// What a Bot's item carries besides its note (ADR 0082): links, a proposal, and the Bot.

/** "from pr-watch": the Bot whose run made the item. */
@Composable
fun BotChip(bot: String, modifier: Modifier = Modifier) {
    Chip("from $bot", KindPalette.todo, modifier)
}

/** "with pr-watch": the Bot a talk Task talks with (ADR 0083). */
@Composable
fun TalkChip(bot: String, modifier: Modifier = Modifier) {
    Chip("with $bot", KindPalette.todo, modifier)
}

/** The pages the item is about; each opens in the browser. */
@Composable
fun ItemLinks(links: List<ItemLink>) {
    val context = LocalContext.current
    ItemBox("Links", Modifier.testTag("detail.links")) {
        links.forEachIndexed { i, link ->
            Text(
                link.title?.takeIf { it.isNotBlank() } ?: link.url,
                Modifier.fillMaxWidth().clickable(role = Role.Button) { openWebLink(context, link.url) }
                    .padding(vertical = 4.dp).testTag("detail.link.$i"),
                color = Palette.accent,
                fontSize = 15.sp,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/** What a Decide item proposes. muxa never performs it: Start Task takes it as the goal. */
@Composable
fun ProposalBox(p: ItemProposal) {
    ItemBox("Proposal", Modifier.testTag("detail.proposal")) {
        ProposalFact("Action", p.action, "detail.proposal.action")
        ProposalFact("Target", p.target, "detail.proposal.target")
        Text("Content", fontSize = 12.sp, color = Palette.textTertiary)
        SelectionContainer(Modifier.fillMaxWidth().background(Palette.control, SHAPE).padding(10.dp)) {
            Text(p.content, Modifier.testTag("detail.proposal.content"), fontFamily = FontFamily.Monospace, fontSize = 13.sp)
        }
    }
}

@Composable
private fun ProposalFact(label: String, value: String, tag: String) {
    Column {
        Text(label, fontSize = 12.sp, color = Palette.textTertiary)
        SelectionContainer { Text(value, Modifier.testTag(tag), fontSize = 15.sp) }
    }
}

/** A titled box like the item's Summary. */
@Composable
fun ItemBox(title: String, modifier: Modifier, content: @Composable () -> Unit) {
    Column(
        modifier.fillMaxWidth().background(Palette.surface, BOX).border(1.dp, Palette.border, BOX).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Text(title, fontSize = 12.sp, color = Palette.textTertiary)
        content()
    }
}

private val BOX = RoundedCornerShape(10.dp)
private val SHAPE = RoundedCornerShape(8.dp)
