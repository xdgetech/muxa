package tech.xdge.muxa.app.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** The design's corner radii, as on iOS (`Chrome.swift`). Buttons are rounded rectangles, never pills. */
object Corners {
    val button = RoundedCornerShape(10.dp)
    val link = RoundedCornerShape(8.dp)
    val chip = RoundedCornerShape(6.dp)
    val dialog = RoundedCornerShape(16.dp)
}

/** What a filled button's color says. */
enum class Tone {
    /** The screen's main action, in the accent. */
    ACCENT,

    /** The action answers a Needs-you prompt, in amber, as the design's Approve. */
    NEEDS_YOU,
}

/** The main action: filled, 44 dp tall, 600-weight text. */
@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, tone: Tone = Tone.ACCENT) {
    val (fill, content) = when (tone) {
        Tone.ACCENT -> Palette.accent to Palette.ground
        Tone.NEEDS_YOU -> StateColors.needsYou to StateColors.onNeedsYou
    }
    MuxaButton(text, onClick, modifier, enabled, filled(fill, content), border = null, FontWeight.SemiBold)
}

/** A quiet action beside a primary one. */
@Composable
fun SecondaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    MuxaButton(text, onClick, modifier, enabled, filled(Palette.secondaryFill, Palette.text), BorderStroke(1.dp, Palette.secondaryBorder), FontWeight.Medium)
}

/** An action that deletes or loses work: Delete, Remove Anyway, Remove Workspace. */
@Composable
fun DestructiveButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    MuxaButton(
        text, onClick, modifier, enabled,
        filled(Palette.destructiveFill, Palette.destructive), BorderStroke(1.dp, Palette.destructiveBorder), FontWeight.SemiBold,
    )
}

/** A text link in the accent, such as a dialog's Cancel. */
@Composable
fun LinkButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, color: Color = Palette.accent) {
    TextButton(
        onClick, modifier.heightIn(min = MIN_TOUCH), enabled = enabled, shape = Corners.link,
        colors = ButtonDefaults.textButtonColors(contentColor = color, disabledContentColor = color.copy(alpha = DISABLED_ALPHA)),
        contentPadding = PaddingValues(horizontal = 12.dp),
    ) { Text(text, fontSize = 14.sp, fontWeight = FontWeight.Medium) }
}

@Composable
private fun MuxaButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier,
    enabled: Boolean,
    colors: ButtonColors,
    border: BorderStroke?,
    weight: FontWeight,
) {
    Button(
        onClick, modifier.heightIn(min = MIN_TOUCH), enabled = enabled, shape = Corners.button, colors = colors,
        border = border,
        contentPadding = PaddingValues(horizontal = 16.dp),
    ) { Text(text, fontSize = 14.sp, fontWeight = weight, maxLines = 1) }
}

@Composable
private fun filled(fill: Color, content: Color) = ButtonDefaults.buttonColors(
    containerColor = fill,
    contentColor = content,
    disabledContainerColor = fill.copy(alpha = DISABLED_ALPHA),
    disabledContentColor = content.copy(alpha = DISABLED_ALPHA),
)

private val MIN_TOUCH = 44.dp
private const val DISABLED_ALPHA = 0.4f
