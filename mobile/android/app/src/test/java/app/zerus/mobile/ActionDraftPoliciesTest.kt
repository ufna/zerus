package app.zerus.mobile

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

/** Exact confirmed action proof and ordered writer races; no native operations. */
class ActionDraftPoliciesTest {
    private val from = Target("computer", "codex/project/old", "run", "conversation", "workspace")
    private val to = from.copy(session = "codex/project/new")
    private val editor = Draft(from, "Unsent text")
    private val action = SessionAction("request", from, "rename", "{}", status = "completed", resultTarget = to)
    private fun state(action: SessionAction = this.action) = MessageState(drafts = listOf(editor), sessionActions = listOf(action))

    @Test fun sameConversationConfirmedRenameOrResumeMovesOnlyOrdinaryEditor() {
        val question = editor.copy(questionId = "question", questionHash = "hash")
        val saved = editor.copy(detachedId = "saved")
        for (operation in listOf("rename", "resume")) {
            val destination = if (operation == "resume") to.copy(run = "new-run") else to
            val proof = action.copy(operation = operation, resultTarget = destination)
            val moved = ActionDraftPolicies.promote(state(proof).copy(drafts = listOf(editor, question, saved)), proof)
            assertEquals(destination, moved.drafts.single { it.questionId.isBlank() && it.detachedId.isBlank() }.target)
            assertEquals(editor.generation, moved.drafts.first().generation)
            assertTrue(question in moved.drafts); assertTrue(saved in moved.drafts)
        }
    }
    @Test fun missingOrUncertainReceiptAndForeignOrNewConversationCannotMigrate() {
        val rejected = listOf(action.copy(status = "sending"), action.copy(status = "uncertain"), action.copy(operation = "fork"),
            action.copy(resultTarget = to.copy(conversation = "new-conversation")),
            action.copy(resultTarget = to.copy(connectionId = "other-workspace")),
            action.copy(resultTarget = to.copy(computerId = "other-computer")),
            action.copy(resultTarget = to.copy(archiveId = "archive")),
            action.copy(resultTarget = to.copy(agentId = "child", parentConversation = from.conversation)))
        rejected.forEach { assertEquals(editor, ActionDraftPolicies.promote(state(it), it).drafts.single()) }
        assertEquals(editor, ActionDraftPolicies.promote(state().copy(sessionActions = emptyList()), action).drafts.single())
    }
    @Test fun occupiedDestinationNeverReplacesItsEditorOrLockedDelivery() {
        for (status in listOf("editing", "sending", "uncertain")) {
            val occupied = Draft(to, "Destination text", status = status)
            val before = state().copy(drafts = listOf(editor, occupied))
            assertEquals(before, ActionDraftPolicies.promote(before, action))
        }
    }
    @Test fun lockedSourceAndDetachedOrQuestionOnlyDraftsRemainAtOriginalTarget() {
        for (draft in listOf(editor.copy(status = "sending"), editor.copy(status = "uncertain"),
            editor.copy(questionId = "question"), editor.copy(detachedId = "saved"))) {
            val before = state().copy(drafts = listOf(draft))
            assertEquals(before, ActionDraftPolicies.promote(before, action))
        }
    }
    @Test fun lateOldEditorRoutesToConfirmedDestinationWithoutResurrectingSource() {
        val promoted = ActionDraftPolicies.promote(state(), action)
        val changed = MessageChanges.text(promoted, editor.copy(text = "Typed during rename", revision = "later"))
        assertEquals(to, changed.drafts.single().target)
        assertEquals(editor.generation, changed.drafts.single().generation)
        assertEquals("Typed during rename", changed.drafts.single().text)
    }
    @Test fun suspendedWriterReplaysOldTargetEditOntoConfirmedCanonicalDestination() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        try {
            val entered = CompletableDeferred<Unit>(); val release = CompletableDeferred<Unit>()
            var first = true
            val actor = OrderedStatePersistence(state(), scope, {
                if (first) { first = false; entered.complete(Unit); release.await() }
            }, Dispatchers.Unconfined, debounceMillis = 1000)
            val barrier = async { actor.durable({ ActionDraftPolicies.promote(it, action) }, { current, _ -> ActionDraftPolicies.promote(current, action) }) }
            entered.await()
            actor.edit { MessageChanges.text(it, editor.copy(text = "Latest typing", revision = "latest")) }
            release.complete(Unit); barrier.await()
            val written = actor.flush()
            for (value in listOf(written, actor.state.value)) {
                assertEquals(to, value.drafts.single().target)
                assertEquals("Latest typing", value.drafts.single().text)
                assertEquals(editor.generation, value.drafts.single().generation)
                assertFalse(value.drafts.any { it.target == from })
            }
        } finally { scope.cancel() }
    }
}
