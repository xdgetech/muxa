package tech.xdge.muxa.app.reader

import android.graphics.ImageDecoder
import androidx.compose.foundation.Image
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.material.icons.Icons as Material
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import tech.xdge.muxa.app.AppModel
import tech.xdge.muxa.session.ImageFolder
import java.nio.ByteBuffer

/** Previous and Next through the images of the open one's folder, with its position. */
@Composable
fun ImageStepper(model: AppModel, path: String, select: (ReaderItem) -> Unit) {
    val siblings = ImageFolder.siblings(path, model.imageListing?.entries.orEmpty())
    val index = siblings.indexOfFirst { it.path == path }
    IconButton({ select(ReaderItem.Image(siblings[index - 1].path)) }, Modifier.testTag("image-previous"), enabled = index > 0) {
        Icon(Material.AutoMirrored.Filled.KeyboardArrowLeft, "Previous Image")
    }
    if (index >= 0) {
        Text("${index + 1} of ${siblings.size}", Modifier.testTag("image-position"), style = MaterialTheme.typography.labelMedium)
    }
    IconButton({ select(ReaderItem.Image(siblings[index + 1].path)) }, Modifier.testTag("image-next"), enabled = index >= 0 && index + 1 < siblings.size) {
        Icon(Material.AutoMirrored.Filled.KeyboardArrowRight, "Next Image")
    }
}

/**
 * The open image, fitted at first, with pinch and double-tap zoom. ← and → step
 * through its folder. Decoded only when its bytes change: a screenshot can be megabytes.
 */
@Composable
fun ImageDetail(model: AppModel, path: String, select: (ReaderItem) -> Unit) {
    val image = model.state.image?.takeIf { it.path == path }
    val data = image?.data
    var bitmap by remember { mutableStateOf<ImageBitmap?>(null) }
    var failed by remember { mutableStateOf(false) }
    LaunchedEffect(data) {
        if (data == null) return@LaunchedEffect
        val decoded = runCatching { ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(data))).asImageBitmap() }.getOrNull()
        failed = decoded == null
        // A rewrite that fails to decode keeps showing the last good image.
        if (decoded != null) bitmap = decoded
    }
    val siblings = ImageFolder.siblings(path, model.imageListing?.entries.orEmpty())
    val index = siblings.indexOfFirst { it.path == path }
    val focus = remember { FocusRequester() }
    LaunchedEffect(path) { runCatching { focus.requestFocus() } }
    Box(
        Modifier.fillMaxSize().focusRequester(focus).focusable().onKeyEvent { e ->
            if (e.type != KeyEventType.KeyDown) return@onKeyEvent false
            val step = when (e.key) {
                Key.DirectionLeft -> -1
                Key.DirectionRight -> 1
                else -> return@onKeyEvent false
            }
            siblings.getOrNull(index + step)?.let { select(ReaderItem.Image(it.path)) }
            true
        },
        contentAlignment = Alignment.Center,
    ) {
        val shown = bitmap
        when {
            image == null -> Loading()
            shown != null -> Zoomable(shown, path)
            failed -> Unavailable("This image format is not supported.", null)
            image.ended != null -> Unavailable("Can't Open ${name(path)}", image.ended)
            else -> Loading()
        }
        image?.ended?.takeIf { shown != null }?.let { EndedNotice(it) }
    }
}

@Composable
private fun Zoomable(bitmap: ImageBitmap, path: String) {
    var scale by remember(path) { mutableFloatStateOf(1f) }
    var offset by remember(path) { mutableStateOf(Offset.Zero) }
    val state = rememberTransformableState { zoom, pan, _ ->
        scale = (scale * zoom).coerceIn(1f, 8f)
        offset = if (scale == 1f) Offset.Zero else offset + pan
    }
    Image(
        bitmap,
        contentDescription = name(path),
        contentScale = ContentScale.Fit,
        modifier = Modifier.fillMaxSize().testTag("image-view")
            .transformable(state)
            .pointerInput(path) {
                detectTapGestures(onDoubleTap = {
                    if (scale > 1f) {
                        scale = 1f
                        offset = Offset.Zero
                    } else {
                        scale = 2.5f
                    }
                })
            }
            .graphicsLayer(scaleX = scale, scaleY = scale, translationX = offset.x, translationY = offset.y),
    )
}
