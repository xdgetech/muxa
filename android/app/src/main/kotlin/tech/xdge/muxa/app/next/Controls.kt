package tech.xdge.muxa.app.next

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Corners
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.app.ui.Palette
import tech.xdge.muxa.v2.ItemKind

/** One choice of a [Segmented] control. */
data class Segment(val title: String, val tag: String, val enabled: Boolean = true)

/** The design's segmented control: a dark track, the picked choice raised. 48 dp tall. */
@Composable
fun Segmented(segments: List<Segment>, picked: Int, pick: (Int) -> Unit, modifier: Modifier = Modifier, track: Color = Palette.surface) {
    Row(
        modifier
            .height(48.dp)
            .background(track, RoundedCornerShape(10.dp))
            .border(1.dp, Palette.border, RoundedCornerShape(10.dp))
            .padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        segments.forEachIndexed { i, s ->
            val on = i == picked
            Box(
                Modifier
                    .weight(1f)
                    .height(42.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (on) Palette.picked else Color.Transparent)
                    .semantics { selected = on }
                    .clickable(enabled = s.enabled, role = Role.Tab) { pick(i) }
                    .alpha(if (s.enabled) 1f else DISABLED_ALPHA)
                    .testTag(s.tag),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    s.title,
                    color = if (on) Palette.text else Palette.textSecondary,
                    fontSize = 13.sp,
                    fontWeight = if (on) FontWeight.SemiBold else FontWeight.Medium,
                    maxLines = 1,
                )
            }
        }
    }
}

/** A square icon button with a border, as the design's "+" buttons. */
@Composable
fun BoxedIconButton(
    icon: ImageVector,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    round: Boolean = false,
) {
    val shape = if (round) CircleShape else RoundedCornerShape(12.dp)
    Box(
        modifier
            .size(48.dp)
            .clip(shape)
            .background(Palette.control)
            .border(1.dp, Palette.borderStrong, shape)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) { Icon(icon, null, tint = Palette.text) }
}

/** The round accent button that sends a line. */
@Composable
fun SendButton(description: String, enabled: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(48.dp)
            .clip(CircleShape)
            .background(if (enabled) Palette.accent else Palette.control)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) { Icon(Icons.sendUp, null, tint = if (enabled) Palette.ground else Palette.textTertiary) }
}

/** A state's dot. */
@Composable
fun Dot(color: Color, size: Dp = 10.dp, modifier: Modifier = Modifier) {
    Box(modifier.size(size).background(color, CircleShape))
}

/** A screen's large title with its trailing controls, as the phone design. */
@Composable
fun LargeTitle(title: String, trailing: @Composable RowScope.() -> Unit = {}) {
    Row(
        Modifier.fillMaxWidth().padding(start = 18.dp, end = 10.dp, top = 12.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Text(title, Modifier.weight(1f), fontSize = 30.sp, fontWeight = FontWeight.Bold, maxLines = 1)
        trailing()
    }
}

/** An Inbox kind as the design's colored chip. */
@Composable
fun KindChip(kind: ItemKind, modifier: Modifier = Modifier) {
    val c = kind.colors
    Text(
        kind.title,
        modifier.background(c.background, Corners.chip).padding(horizontal = 4.dp, vertical = 3.dp),
        color = c.content,
        fontSize = 11.sp,
        fontWeight = FontWeight.SemiBold,
        textAlign = TextAlign.Center,
        maxLines = 1,
        style = MaterialTheme.typography.labelSmall,
    )
}

private const val DISABLED_ALPHA = 0.4f
