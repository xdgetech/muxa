package tech.xdge.muxa.app.pane

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.text.InputType
import android.view.GestureDetector
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import android.widget.OverScroller
import androidx.core.graphics.withTranslation
import tech.xdge.muxa.session.Composition
import tech.xdge.muxa.session.KeyBar
import tech.xdge.muxa.session.KeyMap
import tech.xdge.muxa.session.KeyOutput
import tech.xdge.muxa.session.PaneFit
import tech.xdge.muxa.terminal.PaneScreen
import tech.xdge.muxa.terminal.ScreenSnapshot
import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * Shows a pane at exactly its cols x rows, with the font its fit picks, and takes the
 * keyboard for it. It never asks for another size: the pane belongs to the desktop
 * layout (ADR 0004). A grid larger than the view scrolls, following the cursor unless
 * the user is reading.
 *
 * Named keys and chords become host key names; typed text goes as text. An input
 * method's composition (Japanese) is drawn at the cursor and sent only when committed.
 */
@SuppressLint("ViewConstructor")
class TerminalPaneView(context: Context) : View(context) {
    /** What to send to the pane. */
    var onOutput: (KeyOutput) -> Unit = {}
    var onPaste: (String) -> Unit = {}

    /** The user pinched to another fit. */
    var onPinch: (PaneFit) -> Unit = {}

    /** After new bytes or a new screen, on the main thread. */
    var onContent: () -> Unit = {}

    /** An app shortcut (a Ctrl+Shift chord); true when the app took it. */
    var onShortcut: (KeyEvent) -> Boolean = { false }

    /** The bar's Ctrl changed. */
    var onKeyBar: (KeyBar) -> Unit = {}

    /** Null lets the view pick (`PaneFit.automatic`). */
    var fitOverride: PaneFit? = null
        set(value) {
            if (field != value) {
                field = value
                relayout()
            }
        }

    /** Gets every screen and output of the watched pane. */
    val screen = PaneScreen(onIdle = { post { contentChanged() } })

    private var snapshot = ScreenSnapshot.EMPTY
    private var composition = Composition()
    var keyBar = KeyBar()
        private set

    private val density = resources.displayMetrics.density
    private val scaledDensity = resources.displayMetrics.scaledDensity
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { typeface = Typeface.MONOSPACE }
    private val fill = Paint()
    private var cellWidth = 1f
    private var cellHeight = 1f
    private var ascent = 0f
    private var gridWidth = 0
    private var gridHeight = 0
    private var cursorWasVisible = true
    private var lastCols = 0
    private var lastRows = 0
    private val scroller = OverScroller(context)
    private var dragging = false

    init {
        isFocusable = true
        isFocusableInTouchMode = true
        contentDescription = "Pane"
        setBackgroundColor(Color.BLACK)
    }

    /** The screen as text, for accessibility and UI tests. */
    val screenText: String get() = screen.rows().joinToString("\n")

    // Layout and fit

    private val mode: PaneFit
        get() = fitOverride ?: PaneFit.automatic(width > height, largestFont(rowsLimit = true), largestFont(rowsLimit = false))

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        relayout()
    }

    private fun relayout() {
        if (width == 0 || height == 0 || snapshot.cols == 0) return
        val m = mode
        val size = m.fontSize(
            wholeFont = if (m == PaneFit.WHOLE) largestFont(rowsLimit = true) else 0f,
            widthFont = if (m == PaneFit.WHOLE) 0f else largestFont(rowsLimit = false),
        )
        setFont(size)
        gridWidth = ceil(cellWidth * snapshot.cols).toInt()
        gridHeight = ceil(cellHeight * snapshot.rows).toInt()
        clampScroll()
        followCursor()
        invalidate()
    }

    /** In sp. With [rowsLimit] false, only the width limits the font. */
    private fun largestFont(rowsLimit: Boolean): Float {
        val cols = max(snapshot.cols, 1)
        val rows = max(snapshot.rows, 1)
        fun fits(sp: Float): Boolean {
            setFont(sp)
            if (cellWidth * cols + 1 > width) return false
            return !rowsLimit || cellHeight * rows + 1 <= height
        }
        // The cell grows with the font, so search for the size.
        var low = 2f
        var high = 64f
        if (!fits(low)) return low
        while (high - low > 0.25f) {
            val mid = (low + high) / 2
            if (fits(mid)) low = mid else high = mid
        }
        return low
    }

    private fun setFont(sp: Float) {
        paint.textSize = sp * scaledDensity
        cellWidth = paint.measureText("W")
        val fm = paint.fontMetrics
        ascent = -fm.ascent
        cellHeight = ceil(fm.descent - fm.ascent)
    }

    // Content

    private fun contentChanged() {
        val next = screen.snapshot()
        val reset = next.cols != lastCols || next.rows != lastRows
        snapshot = next
        if (reset) {
            lastCols = next.cols
            lastRows = next.rows
            // A new grid starts at its top; the last pane's position means nothing here.
            scrollTo(0, 0)
            relayout()
        }
        if (reset || cursorWasVisible) followCursor()
        cursorWasVisible = cursorVisibleOnScreen()
        invalidate()
        onContent()
    }

    private fun cursorRect(): FloatArray {
        val x = snapshot.cursorColumn * cellWidth
        val y = snapshot.cursorRow * cellHeight
        return floatArrayOf(x, y, x + cellWidth, y + cellHeight)
    }

    private fun cursorVisibleOnScreen(): Boolean {
        val (l, t, r, b) = cursorRect().toList()
        return l >= scrollX && r <= scrollX + width && t >= scrollY && b <= scrollY + height
    }

    /** Brings the cursor cell into view, with a few cells around it, never while the user scrolls. */
    private fun followCursor() {
        if (dragging || !scroller.isFinished) return
        if (gridWidth <= width && gridHeight <= height) return
        val (l, t, r, b) = cursorRect().toList()
        val around = if (gridWidth > width) cellWidth * 4 else 0f
        var x = scrollX
        var y = scrollY
        if (l - around < x) x = (l - around).toInt()
        if (r + around > x + width) x = (r + around - width).roundToInt()
        if (t < y) y = t.toInt()
        if (b > y + height) y = (b - height).roundToInt()
        scrollTo(x, y)
        clampScroll()
        cursorWasVisible = true
    }

    private fun clampScroll() {
        val x = scrollX.coerceIn(0, max(0, gridWidth - width))
        val y = scrollY.coerceIn(0, max(0, gridHeight - height))
        if (x != scrollX || y != scrollY) scrollTo(x, y)
    }

    // Drawing

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val s = snapshot
        if (s.cols == 0) return
        // A grid narrower than the view is centered, as on iOS.
        val left = if (gridWidth < width) ((width - gridWidth) / 2).toFloat() else 0f
        canvas.withTranslation(left, 0f) { drawGrid(this, s) }
    }

    private fun drawGrid(canvas: Canvas, s: ScreenSnapshot) {
        for ((row, runs) in s.lines.withIndex()) {
            val top = row * cellHeight
            if (top + cellHeight < scrollY || top > scrollY + height) continue
            for (run in runs) drawRun(canvas, run, top)
        }
        if (s.cursorVisible && !composition.isComposing) {
            val (l, t, r, b) = cursorRect().toList()
            fill.color = CURSOR
            canvas.drawRect(l, t, r, b, fill)
        }
        if (composition.isComposing) drawComposition(canvas)
    }

    private fun drawRun(canvas: Canvas, run: tech.xdge.muxa.terminal.Run, top: Float) {
        val style = run.style
        var fg = Palette.foreground(style.foreground, style.bold)
        var bg = Palette.background(style.background)
        if (style.inverse) fg = bg.also { bg = fg }
        if (style.dim) fg = Palette.dim(fg)
        val x0 = run.column * cellWidth
        if (bg != Palette.BACKGROUND) {
            fill.color = bg
            canvas.drawRect(x0, top, x0 + run.text.length * cellWidth, top + cellHeight, fill)
        }
        if (style.hidden) return
        paint.color = fg
        paint.isFakeBoldText = style.bold
        paint.textSkewX = if (style.italic) -0.2f else 0f
        paint.isUnderlineText = style.underline
        // One cell per char, so wide characters and fallback fonts keep the grid.
        for ((i, c) in run.text.withIndex()) {
            if (c == ScreenSnapshot.WIDE_TAIL || c == ' ' || c == '\u0000') continue
            canvas.drawText(c.toString(), x0 + i * cellWidth, top + ascent, paint)
        }
        paint.isUnderlineText = false
    }

    /** The marked text over the cursor cell, underlined, kept inside the view. */
    private fun drawComposition(canvas: Canvas) {
        val text = composition.marked
        val w = paint.measureText(text)
        val (l, t, _, _) = cursorRect().toList()
        val x = min(l, max(0f, gridWidth - w))
        fill.color = Palette.BACKGROUND
        canvas.drawRect(x, t, x + w, t + cellHeight, fill)
        paint.color = Color.WHITE
        paint.isFakeBoldText = false
        paint.textSkewX = 0f
        paint.isUnderlineText = true
        canvas.drawText(text, x, t + ascent, paint)
        paint.isUnderlineText = false
    }

    // Touch: tap for the keyboard, drag to scroll, pinch to switch the fit.

    private val gestures = GestureDetector(
        context,
        object : GestureDetector.SimpleOnGestureListener() {
            override fun onDown(e: MotionEvent): Boolean {
                scroller.forceFinished(true)
                return true
            }

            override fun onSingleTapUp(e: MotionEvent): Boolean {
                showKeyboard()
                return true
            }

            override fun onScroll(e1: MotionEvent?, e2: MotionEvent, dx: Float, dy: Float): Boolean {
                dragging = true
                // One axis at a time, as the iOS scroll view locks its direction.
                if (kotlin.math.abs(dx) > kotlin.math.abs(dy)) scrollBy(dx.toInt(), 0) else scrollBy(0, dy.toInt())
                clampScroll()
                return true
            }

            override fun onFling(e1: MotionEvent?, e2: MotionEvent, vx: Float, vy: Float): Boolean {
                scroller.fling(scrollX, scrollY, -vx.toInt(), -vy.toInt(), 0, max(0, gridWidth - width), 0, max(0, gridHeight - height))
                postInvalidateOnAnimation()
                return true
            }
        },
    )

    private val pinch = ScaleGestureDetector(
        context,
        object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
            private var scale = 1f

            override fun onScaleBegin(d: ScaleGestureDetector): Boolean {
                scale = 1f
                return true
            }

            override fun onScale(d: ScaleGestureDetector): Boolean {
                scale *= d.scaleFactor
                return true
            }

            override fun onScaleEnd(d: ScaleGestureDetector) {
                PaneFit.pinched(scale)?.let(onPinch)
            }
        },
    )

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(event: MotionEvent): Boolean {
        pinch.onTouchEvent(event)
        if (!pinch.isInProgress) gestures.onTouchEvent(event)
        if (event.actionMasked == MotionEvent.ACTION_UP || event.actionMasked == MotionEvent.ACTION_CANCEL) {
            dragging = false
            if (scroller.isFinished) cursorWasVisible = cursorVisibleOnScreen()
        }
        return true
    }

    override fun computeScroll() {
        if (scroller.computeScrollOffset()) {
            scrollTo(scroller.currX, scroller.currY)
            postInvalidateOnAnimation()
        } else if (!dragging) {
            cursorWasVisible = cursorVisibleOnScreen()
        }
    }

    /** A mouse wheel or a two-finger trackpad scroll moves the view; it never types. */
    override fun onGenericMotionEvent(event: MotionEvent): Boolean {
        if (event.isFromSource(InputDevice.SOURCE_CLASS_POINTER) && event.actionMasked == MotionEvent.ACTION_SCROLL) {
            val step = cellHeight * 3
            scrollBy((-event.getAxisValue(MotionEvent.AXIS_HSCROLL) * step).toInt(), (-event.getAxisValue(MotionEvent.AXIS_VSCROLL) * step).toInt())
            clampScroll()
            cursorWasVisible = cursorVisibleOnScreen()
            return true
        }
        return super.onGenericMotionEvent(event)
    }

    // Keyboard

    /**
     * A new layout, such as a fold, can move this view out of the window and back in.
     * Android clears its focus just before it is detached; it takes the keyboard back when
     * it is attached again. A focus loss that leaves it attached, such as another field
     * taking the keyboard, stays.
     */
    private var justLostFocus = false
    private var reclaimFocus = false

    override fun onFocusChanged(gainFocus: Boolean, direction: Int, previouslyFocusedRect: android.graphics.Rect?) {
        super.onFocusChanged(gainFocus, direction, previouslyFocusedRect)
        if (gainFocus) return
        justLostFocus = true
        post { justLostFocus = false }
    }

    override fun onDetachedFromWindow() {
        reclaimFocus = justLostFocus || hasFocus()
        super.onDetachedFromWindow()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        // The input method still holds the connection of before the move: start a new one.
        if (reclaimFocus) {
            post {
                requestFocus()
                context.getSystemService(InputMethodManager::class.java)?.restartInput(this)
            }
        }
        reclaimFocus = false
    }

    fun showKeyboard() {
        requestFocus()
        context.getSystemService(InputMethodManager::class.java)?.showSoftInput(this, 0)
    }

    /** Takes focus without raising the on-screen keyboard: only useful with a hardware one. */
    fun focusIfKeyboard() {
        if (resources.configuration.hardKeyboardHidden == android.content.res.Configuration.HARDKEYBOARDHIDDEN_NO) requestFocus()
    }

    override fun onCheckIsTextEditor() = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        // Text, so input methods compose (Japanese), with nothing a terminal must not get:
        // no suggestions, no autocorrect, no full-screen editor.
        outAttrs.inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or EditorInfo.IME_FLAG_NO_FULLSCREEN or
            EditorInfo.IME_ACTION_NONE or EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING
        return PaneInputConnection()
    }

    /** A bar key ends any composition first, as Return would, so the order typed is kept. */
    fun tap(key: KeyBar.Key) {
        if (composition.isComposing) commitComposition()
        val (bar, output) = keyBar.tap(key)
        setKeyBar(bar)
        emit(output)
    }

    private fun setKeyBar(bar: KeyBar) {
        if (bar != keyBar) {
            keyBar = bar
            onKeyBar(bar)
        }
    }

    private fun emit(output: KeyOutput) {
        if (output != KeyOutput.None) onOutput(output)
    }

    /** What the input method or a key produced, with a pending bar Ctrl applied. */
    private fun type(output: KeyOutput) {
        val (bar, out) = keyBar.type(output)
        setKeyBar(bar)
        emit(out)
    }

    private fun commitComposition() {
        val (next, output) = composition.commit()
        composition = next
        type(output)
        invalidate()
    }

    /** Hardware keys reach the input method first; these keys must not. */
    override fun onKeyPreIme(keyCode: Int, event: KeyEvent): Boolean {
        if (composition.isComposing || event.action != KeyEvent.ACTION_DOWN) return super.onKeyPreIme(keyCode, event)
        val special = special(keyCode)
        // The input method would take these chords before the Activity sees them.
        if (event.isCtrlPressed && event.isShiftPressed && special == null && onShortcut(event)) return true
        val chord = event.isCtrlPressed || event.isAltPressed
        if (special == null && !chord) return super.onKeyPreIme(keyCode, event)
        return handleKey(event) || super.onKeyPreIme(keyCode, event)
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean =
        handleKey(event) || super.onKeyDown(keyCode, event)

    private fun handleKey(event: KeyEvent): Boolean {
        val mods = KeyMap.Modifiers(
            control = event.isCtrlPressed,
            alt = event.isAltPressed,
            shift = event.isShiftPressed,
            // Ctrl+Shift chords are the app's shortcuts (ADR 0013); Meta belongs to the system.
            shortcut = (event.isCtrlPressed && event.isShiftPressed && special(event.keyCode) == null) || event.isMetaPressed,
        )
        if (mods.shortcut) return false
        val chars = event.getUnicodeChar(0).takeIf { it > 0 }?.let { String(Character.toChars(it)) } ?: ""
        val shifted = event.getUnicodeChar(event.metaState and KeyEvent.META_SHIFT_MASK).takeIf { it > 0 }
            ?.let { String(Character.toChars(it)) } ?: chars
        val output = KeyMap.map(special(event.keyCode), if (mods.shift) shifted else chars, mods)
        if (output == KeyOutput.None) return false
        if (output is KeyOutput.Text) type(output) else emit(output)
        return true
    }

    private inner class PaneInputConnection : BaseInputConnection(this, false) {
        override fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
            val t = text.toString()
            // A kana keyboard composes even a lone letter: with Ctrl from the bar, that letter is the chord.
            if (keyBar.control && !composition.isComposing && t.isNotEmpty() && keyBar.type(KeyOutput.Text(t)).second is KeyOutput.Keys) {
                type(KeyOutput.Text(t))
                return true
            }
            composition = composition.mark(t, t.length, t.length)
            invalidate()
            return true
        }

        override fun finishComposingText(): Boolean {
            if (composition.isComposing) commitComposition()
            return true
        }

        override fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
            val (next, output) = composition.insert(text.toString())
            composition = next
            type(output)
            invalidate()
            return true
        }

        override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
            repeat(max(beforeLength, 1)) {
                val (next, output) = composition.deleteBackward()
                composition = next
                type(output)
            }
            invalidate()
            return true
        }

        override fun sendKeyEvent(event: KeyEvent): Boolean {
            if (event.action == KeyEvent.ACTION_DOWN) {
                if (event.keyCode == KeyEvent.KEYCODE_DEL) return deleteSurroundingText(1, 0)
                handleKey(event)
            }
            return true
        }

        override fun performContextMenuAction(id: Int): Boolean {
            if (id == android.R.id.paste) {
                val clip = context.getSystemService(android.content.ClipboardManager::class.java)?.primaryClip
                clip?.getItemAt(0)?.coerceToText(context)?.toString()?.let(onPaste)
                return true
            }
            return super.performContextMenuAction(id)
        }

        // The pane's text lives on the host; there is nothing here to read back.
        override fun getTextBeforeCursor(n: Int, flags: Int): CharSequence = ""
        override fun getTextAfterCursor(n: Int, flags: Int): CharSequence = ""
    }

    private companion object {
        val CURSOR = Color.argb(160, 220, 220, 220)

        fun special(keyCode: Int): KeyMap.Special? = when (keyCode) {
            KeyEvent.KEYCODE_DPAD_UP -> KeyMap.Special.Up
            KeyEvent.KEYCODE_DPAD_DOWN -> KeyMap.Special.Down
            KeyEvent.KEYCODE_DPAD_LEFT -> KeyMap.Special.Left
            KeyEvent.KEYCODE_DPAD_RIGHT -> KeyMap.Special.Right
            KeyEvent.KEYCODE_MOVE_HOME -> KeyMap.Special.Home
            KeyEvent.KEYCODE_MOVE_END -> KeyMap.Special.End
            KeyEvent.KEYCODE_PAGE_UP -> KeyMap.Special.PageUp
            KeyEvent.KEYCODE_PAGE_DOWN -> KeyMap.Special.PageDown
            KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> KeyMap.Special.Enter
            KeyEvent.KEYCODE_ESCAPE -> KeyMap.Special.Escape
            KeyEvent.KEYCODE_TAB -> KeyMap.Special.Tab
            KeyEvent.KEYCODE_DEL -> KeyMap.Special.Backspace
            KeyEvent.KEYCODE_FORWARD_DEL -> KeyMap.Special.DeleteForward
            KeyEvent.KEYCODE_INSERT -> KeyMap.Special.Insert
            in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12 -> KeyMap.Special.Function(keyCode - KeyEvent.KEYCODE_F1 + 1)
            else -> null
        }
    }
}
