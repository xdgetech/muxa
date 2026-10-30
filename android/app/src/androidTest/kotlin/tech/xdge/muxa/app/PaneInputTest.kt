package tech.xdge.muxa.app

import android.view.inputmethod.EditorInfo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import tech.xdge.muxa.app.pane.TerminalPaneView
import tech.xdge.muxa.session.KeyBar
import tech.xdge.muxa.session.KeyOutput

/**
 * The pane's input connection, as an input method drives it. A UI test cannot drive
 * Gboard's Japanese conversion, so these calls stand in for it.
 */
@RunWith(AndroidJUnit4::class)
class PaneInputTest {
    private fun pane(): Pair<TerminalPaneView, MutableList<KeyOutput>> {
        val sent = mutableListOf<KeyOutput>()
        var view: TerminalPaneView? = null
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            view = TerminalPaneView(InstrumentationRegistry.getInstrumentation().targetContext).apply { onOutput = { sent += it } }
        }
        return view!! to sent
    }

    private fun onMain(block: () -> Unit) = InstrumentationRegistry.getInstrumentation().runOnMainSync(block)

    @Test
    fun aCompositionIsSentOnlyWhenCommitted() {
        val (view, sent) = pane()
        onMain {
            val ic = view.onCreateInputConnection(EditorInfo())
            ic.setComposingText("に", 1)
            ic.setComposingText("にほんご", 1)
            assertEquals(emptyList<KeyOutput>(), sent)
            // Conversion replaces the composition with the chosen text, once.
            ic.commitText("日本語", 1)
            assertEquals(listOf(KeyOutput.Text("日本語")), sent)
            // An input method that ends the composition as typed sends the kana.
            ic.setComposingText("かな", 1)
            ic.finishComposingText()
            assertEquals(listOf(KeyOutput.Text("日本語"), KeyOutput.Text("かな")), sent)
        }
    }

    @Test
    fun backspaceEditsTheCompositionFirst() {
        val (view, sent) = pane()
        onMain {
            val ic = view.onCreateInputConnection(EditorInfo())
            ic.setComposingText("にほ", 1)
            ic.deleteSurroundingText(1, 0)
            assertEquals(emptyList<KeyOutput>(), sent)
            ic.finishComposingText()
            ic.deleteSurroundingText(1, 0)
            assertEquals(listOf(KeyOutput.Text("に"), KeyOutput.Keys(listOf("BSpace"))), sent)
        }
    }

    @Test
    fun barCtrlMakesTheNextLetterAChordEvenFromAKanaKeyboard() {
        val (view, sent) = pane()
        onMain {
            view.tap(KeyBar.Key.CONTROL)
            val ic = view.onCreateInputConnection(EditorInfo())
            // A kana keyboard composes even a lone letter.
            ic.setComposingText("c", 1)
            assertEquals(listOf(KeyOutput.Keys(listOf("C-c"))), sent)
            ic.commitText("x", 1)
            assertEquals(listOf(KeyOutput.Keys(listOf("C-c")), KeyOutput.Text("x")), sent)
        }
    }
}
