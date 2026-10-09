package app.zerus.mobile

import kotlinx.coroutines.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class ScheduledSettingsAttemptTest {
    private val target=Target("computer","session","run","conversation","workspace")
    private val source=SessionAction("original",target,"settings",JSONObject().put("model","synthetic").put("effort","high").toString(),status="scheduled",pendingId="pending")
    private val attempt=SessionAction("attempt",target,"settings",JSONObject().put("model","synthetic").put("effort","high").put("expected_pending_id","pending").toString())
    @Test fun suspendedBackgroundApplyPersistsOneAttemptWithoutChangingAnotherComposer()=runBlocking {
        val other=target.copy(session="other",conversation="other-conversation")
        val draft=Draft(other,"Keep typing",attachments=emptyList())
        val initial=MessageState(drafts=listOf(draft),sessionActions=listOf(source))
        val entered=CompletableDeferred<Unit>();val release=CompletableDeferred<Unit>()
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.Default)
        val writer=OrderedStatePersistence(initial,scope,{ entered.complete(Unit);release.await() },uiDispatcher=Dispatchers.Unconfined)
        val pending=async { writer.durable({ ScheduledSettingsAttempt.begin(it,source,attempt) }) }
        entered.await()
        writer.edit { MessageChanges.text(it,draft.copy(text="Newest text",revision="new")) }
        release.complete(Unit);pending.await();writer.flush()
        assertEquals("Newest text",writer.state.value.drafts.single().text)
        assertEquals(other,writer.state.value.drafts.single().target)
        assertEquals("attempt",writer.state.value.sessionActions.first { it.requestId == "original" }.applyAttemptId)
        assertThrows(IllegalStateException::class.java) { ScheduledSettingsAttempt.begin(writer.state.value,source,attempt.copy(requestId="second")) }
        scope.cancel()
    }
    @Test fun changedPendingOrRequestedModelCannotStartAttempt() {
        assertThrows(IllegalArgumentException::class.java) { ScheduledSettingsAttempt.begin(MessageState(sessionActions=listOf(source)),source,attempt.copy(arguments=JSONObject(attempt.arguments).put("expected_pending_id","changed").toString())) }
        assertThrows(IllegalArgumentException::class.java) { ScheduledSettingsAttempt.begin(MessageState(sessionActions=listOf(source)),source,attempt.copy(arguments=JSONObject(attempt.arguments).put("model","unseen").toString())) }
    }
    @Test fun originalOnceProofSurvivesTerminalReceiptPruning() {
        var state=ScheduledSettingsAttempt.begin(MessageState(sessionActions=listOf(source)),source,attempt)
        repeat(150) { state=state.action(attempt.copy(requestId="unrelated-$it",status="completed")) }
        assertEquals("attempt",state.sessionActions.first { it.requestId == "original" }.applyAttemptId)
        assertThrows(IllegalStateException::class.java) { ScheduledSettingsAttempt.begin(state,source,attempt.copy(requestId="second")) }
    }
}
