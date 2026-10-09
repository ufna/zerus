package app.zerus.mobile

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.atomic.AtomicInteger

class ContextOperationsTest {
    private val target = Target("computer", "codex/project/session", "run", "conversation", "workspace")
    private val file = Attachment("10000000-0000-4000-8000-000000000001", "synthetic.txt", "text/plain", 4, "0".repeat(64))
    private val original = Draft(target, "Captured message", attachments = listOf(file), generation = "generation", revision = "revision")
    private val op = ContextOperation("compact-id", target, "compact_context", "completed", 1)
    private fun native(status: String = "completed") = JSONObject().put("name", target.session).put("run_id", target.run).put("conversation_id", target.conversation)
        .put("activity", "idle").put("phase", "idle").put("compact_context_request", JSONObject().put("request_id", op.requestId).put("run_id", target.run).put("conversation_id", target.conversation).put("status", status))
    @Test fun submittedAcknowledgementNeverCountsAsCompletedCompaction() {
        val receipt = JSONObject().put("request_id", op.requestId).put("state", "completed").put("result", JSONObject().put("request_id", op.requestId).put("name", target.session).put("run_id", target.run).put("conversation_id", target.conversation).put("status", "submitted"))
        assertEquals("submitted", ContextPolicies.result(op.copy(status = "sending"), receipt).status)
        assertNull(ContextPolicies.compactStatus(op, native("submitted")))
        assertNull(ContextPolicies.compactStatus(op, native().put("phase", "compacting")))
        assertNull(ContextPolicies.compactStatus(op, native().put("conversation_id", "different")))
        assertNull(ContextPolicies.compactStatus(op, native().also { it.getJSONObject("compact_context_request").put("request_id", "different") }))
        assertEquals("completed", ContextPolicies.compactStatus(op, native()))
    }
    @Test fun mismatchedNativeOrOuterReceiptRemainsUncertain() {
        val receipt = JSONObject().put("request_id", op.requestId).put("state", "completed").put("result", JSONObject().put("request_id", "foreign").put("name", target.session).put("run_id", target.run).put("conversation_id", target.conversation).put("status", "submitted"))
        assertEquals("uncertain", ContextPolicies.result(op, receipt).status)
        receipt.put("state", "failed").put("request_id", "foreign")
        assertEquals("uncertain", ContextPolicies.result(op, receipt).status)
    }
    @Test fun legacyAndInterruptedRecordsRecoverWithoutContinuationAuthorization() {
        val legacy = MessageCodec.state(JSONObject().put("drafts", org.json.JSONArray().put(MessageCodec.draftJson(original))))
        assertTrue(legacy.contextOperations.isEmpty())
        val pending = legacy.context(op.copy(status = "submitted"))
        val recovered = MessageCodec.state(MessageCodec.stateJson(pending))
        assertEquals("uncertain", recovered.contextOperations.single().status)
        assertEquals(original, recovered.drafts.single())
        assertThrows(IllegalArgumentException::class.java) { recovered.enqueue(original, "message-id", 1.0) }
    }
    private fun suspendedEnqueue(cancel: String) = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val entered = CompletableDeferred<Unit>(); val release = CompletableDeferred<Unit>(); val requests = AtomicInteger()
        val initial = MessageState(listOf(original), contextOperations = listOf(op))
        val writer = OrderedStatePersistence(initial, scope, { state -> if (state.outgoing.isNotEmpty() && state.outgoing.single().status == "sending") { entered.complete(Unit); release.await() } }, Dispatchers.Unconfined)
        val lease = CompactContinuation(op.requestId, original, 1, 0)
        val active = MutableStateFlow<CompactContinuation?>(lease)
        var navigation = 1L
        val send = scope.async {
            val written = writer.durable({ it.enqueue(original, "message-id", 1.0) }, { current, committed -> MessageChanges.enqueueAcknowledged(current, original, committed, "message-id") })
            val cleared = written.drafts.single()
            if (active.value?.requestId == lease.requestId && lease.permitsTransport(target, navigation, 1, cleared, writer.state.value.drafts.single())) requests.incrementAndGet()
            else writer.durable({ state -> MessageChanges.result(state, "message-id") { it.copy(status = "failed", error = "Not sent") } })
        }
        withTimeout(5_000) { entered.await() }
        when (cancel) {
            "cancel" -> active.value = null
            "navigation" -> { navigation = 2; active.value = null }
            "edit" -> { active.value = null; writer.edit { MessageChanges.text(it, original.copy(text = "New text", revision = "later")) } }
        }
        release.complete(Unit)
        withTimeout(5_000) { send.await(); writer.flush() }
        if (cancel.isEmpty()) assertEquals(1, requests.get()) else {
            assertEquals(0, requests.get()); assertEquals("failed", writer.durableState.outgoing.single().status)
            assertEquals(listOf(file), writer.durableState.outgoing.single().attachments)
            assertEquals(original.text, writer.durableState.outgoing.single().text)
        }
        if (cancel == "edit") { assertEquals("New text", writer.durableState.drafts.single().text); assertTrue(writer.durableState.drafts.single().attachments.isEmpty()) }
        scope.cancel()
    }
    @Test fun cancellationWhileOutboxWriteSuspendedNeverPosts() = suspendedEnqueue("cancel")
    @Test fun navigationWhileOutboxWriteSuspendedNeverPosts() = suspendedEnqueue("navigation")
    @Test fun typingWhileOutboxWriteSuspendedKeepsNewTextAndNeverPosts() = suspendedEnqueue("edit")
    @Test fun unchangedLiveIntentCanPostExactlyOnceAfterDurableEnqueue() = suspendedEnqueue("")
    @Test fun failedOutgoingPersistenceKeepsDraftAndDoesNotReleaseTransport() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val initial = MessageState(listOf(original), contextOperations = listOf(op))
        val writer = OrderedStatePersistence(initial, scope, { error("Synthetic disk failure") }, Dispatchers.Unconfined)
        val posts = AtomicInteger()
        runCatching { writer.durable({ it.enqueue(original, "message-id", 1.0) }); posts.incrementAndGet() }
        assertEquals(0, posts.get()); assertEquals(initial, writer.state.value); assertEquals(initial, writer.durableState)
        scope.cancel()
    }
    @Test fun confirmedClearPromotionRoutesOldEditorDuringSuspendedWriteWithoutRecreatingOldDraft() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val entered = CompletableDeferred<Unit>(); val release = CompletableDeferred<Unit>()
        val clear = ContextOperation("clear-id", target, "clear_context", "completed", 1)
        val to = target.copy(conversation = "new-conversation")
        val initial = MessageState(listOf(original), contextOperations = listOf(clear))
        val writer = OrderedStatePersistence(initial, scope, { state -> if (state.drafts.any { it.target == to && it.text == original.text }) { entered.complete(Unit); release.await() } }, Dispatchers.Unconfined)
        writer.edit { ContextPolicies.promoteClearDraft(it, clear, to) }
        val flush = scope.async { writer.flush() }
        withTimeout(5_000) { entered.await() }
        writer.edit { MessageChanges.text(it, original.copy(text = "Latest text", revision = "latest")) }
        release.complete(Unit)
        withTimeout(5_000) { flush.await(); writer.flush() }
        assertEquals(to, writer.state.value.drafts.single().target); assertEquals(to, writer.durableState.drafts.single().target)
        assertEquals("Latest text", writer.durableState.drafts.single().text); assertEquals(listOf(file), writer.durableState.drafts.single().attachments)
        assertEquals(original.generation, writer.durableState.drafts.single().generation)
        scope.cancel()
    }
    @Test fun clearPromotionRequiresConfirmedProofAndEmptyDestinationAndPreservesOtherRecords() {
        val clear = ContextOperation("clear-id", target, "clear_context", "completed", 1)
        val to = target.copy(conversation = "new-conversation")
        val question = Draft(target, "Answer draft", questionId = "q", questionHash = "hash")
        val outgoing = OutgoingMessage("send-id", target, "Old sent text", listOf(file), 1.0, "submitted")
        val state = MessageState(listOf(original, question), listOf(outgoing), contextOperations = listOf(clear))
        assertEquals(state, ContextPolicies.promoteClearDraft(state, clear.copy(status = "submitted"), to))
        assertEquals(state, ContextPolicies.promoteClearDraft(state, clear, to.copy(run = "different")))
        val occupied = state.copy(drafts = state.drafts + Draft(to, "Already here"))
        assertEquals(occupied, ContextPolicies.promoteClearDraft(occupied, clear, to))
        val moved = ContextPolicies.promoteClearDraft(state, clear, to)
        assertEquals(question, moved.drafts.find { it.questionId == "q" }); assertEquals(outgoing, moved.outgoing.single())
        assertTrue(ContextPolicies.clearMoved(moved, target, to)); assertFalse(ContextPolicies.clearMoved(moved, target, to.copy(conversation = "unverified")))
    }
}
