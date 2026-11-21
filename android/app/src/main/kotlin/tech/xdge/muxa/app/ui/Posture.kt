package tech.xdge.muxa.app.ui

import androidx.activity.compose.LocalActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.LocalDensity
import androidx.window.layout.FoldingFeature
import androidx.window.layout.WindowInfoTracker
import kotlinx.coroutines.flow.map

/**
 * The layout of this window now: its size, which split screen and desktop windows make
 * their own, and its hinge, which Jetpack WindowManager reports.
 */
@Composable
fun currentWindowLayout(): WindowLayout {
    val window = LocalWindowInfo.current.containerSize
    val density = LocalDensity.current.density
    val activity = LocalActivity.current ?: return WindowLayout.Phone
    val hinge by remember(activity) {
        WindowInfoTracker.getOrCreate(activity).windowLayoutInfo(activity).map { info ->
            info.displayFeatures.filterIsInstance<FoldingFeature>().firstOrNull()?.let { f ->
                val vertical = f.orientation == FoldingFeature.Orientation.VERTICAL
                val bounds = f.bounds
                Hinge(
                    vertical = vertical,
                    halfOpened = f.state == FoldingFeature.State.HALF_OPENED,
                    separating = f.isSeparating,
                    start = ((if (vertical) bounds.left else bounds.top) / density).toInt(),
                    end = ((if (vertical) bounds.right else bounds.bottom) / density).toInt(),
                )
            }
        }
    }.collectAsState(null)
    return windowLayout((window.width / density).toInt(), (window.height / density).toInt(), hinge)
}
