package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class QuestionDisplayStateTest {
    private fun question(id: String = "question", hash: String = "hash") =
        Question(id, hash, emptyList(), true, true, true, "")

    @Test fun rotationRestoresExactOwnerQuestionPartAndScroll() {
        val question = question()
        val state = QuestionDisplayState("workspace/computer/session/run/conversation").open(question)
            .withPage(question, 2).withOffset(question, 2, 347)
        val restored = QuestionDisplayState.decode(state.encode())!!
        assertEquals(state, restored)
        assertEquals(question, restored.resolve(listOf(question)))
        assertEquals(2, restored.page(question))
        assertEquals(347, restored.offset(question, 2))
        assertEquals(0, restored.offset(question, 1))
        assertEquals(restored, restored.forTarget(state.owner))
        assertFalse(restored.forTarget("other run").expanded)
        assertTrue(restored.forTarget("other run").pages.isEmpty())
    }

    @Test fun missingChangedAndAmbiguousQuestionNeverRebindToAnotherRequest() {
        val original = question()
        val restored = QuestionDisplayState.decode(QuestionDisplayState("owner").open(original).encode())!!
        assertNull(restored.resolve(emptyList()))
        assertNull(restored.resolve(listOf(question(hash = "changed"))))
        assertNull(restored.resolve(listOf(question(id = "other"))))
        assertNull(restored.resolve(listOf(original, original)))
        assertTrue(restored.expanded)
        assertFalse(restored.close().expanded)
    }

    @Test fun transferAndCollapsePreserveEachExactPartWithoutDelimiterCollisions() {
        val a = question("a:b", "c")
        val b = question("a", "b:c")
        val state = QuestionDisplayState("owner").open(a).withPage(a, 1).withOffset(a, 1, 99)
            .close().open(b).withPage(b, 3).withOffset(b, 3, 155).close().open(a)
        assertEquals(1, state.page(a))
        assertEquals(99, state.offset(a, 1))
        assertEquals(3, state.page(b))
        assertEquals(155, state.offset(b, 3))
    }

    @Test fun retentionIsBoundedAndCorruptSavedStateFailsClosed() {
        var state = QuestionDisplayState("owner")
        repeat(100) { i -> state = state.withPage(question("$i"), i).withOffset(question("$i"), i, i) }
        assertEquals(64, state.pages.size)
        assertEquals(64, state.offsets.size)
        assertEquals(state, QuestionDisplayState.decode(state.encode()))
        assertNull(QuestionDisplayState.decode("invalid"))
        assertNull(QuestionDisplayState.decode("{}"))
    }
}
