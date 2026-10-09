package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class ComposerReconcilerTest {
    @Test fun rapidTypingDoesNotReplaceImeBufferWithLaggingModelEchoes() {
        val sync = ComposerSync("composition")
        listOf("", "w", "wo", "wor", "work", "working").forEach { echo ->
            val update = ComposerReconciler.external(sync, "composition", echo)
            // No TextFieldState.edit is requested, so its newer text, cursor,
            // selection and composing span all remain owned by the IME.
            assertNull(update.replacement)
            assertSame(sync, update.sync)
        }
    }
    @Test fun pollAndSameGenerationAttachmentChangesNeverResetEditor() {
        assertNull(ComposerReconciler.external(ComposerSync("same"), "same", "Older saved text").replacement)
    }
    @Test fun explicitSendClearReplacesTextExactlyOnce() {
        val clear = ComposerReconciler.external(ComposerSync("sent"), "next", "")
        assertEquals("", clear.replacement)
        assertEquals(ComposerSync("next"), clear.sync)
        assertNull(ComposerReconciler.external(clear.sync, "next", "").replacement)
        assertNull(ComposerReconciler.external(clear.sync, "next", "Future typing echo").replacement)
    }
    @Test fun explicitRestoreAndHydrationCanReplaceComposition() {
        val restored = ComposerReconciler.external(ComposerSync("empty"), "restored", "Saved\ncomposition")
        assertEquals("Saved\ncomposition", restored.replacement)
        assertNull(ComposerReconciler.external(restored.sync, "restored", "Saved\ncomposition").replacement)
    }
}
