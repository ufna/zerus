package app.zerus.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SessionActionTest {
    private val target=Target("computer","session","run","conversation","workspace")
    private val action=SessionAction("request",target,"resume","{}")
    private fun receipt()=JSONObject().put("request_id","request").put("state","completed").put("result",JSONObject()
        .put("request_id","request").put("name","session").put("run_id","run").put("conversation_id","conversation").put("status","completed"))
    @Test fun mismatchedNativeReceiptNeverUnblocksAction() {
        val receipt=receipt(); receipt.getJSONObject("result").put("run_id","other")
        assertTrue(SessionActionPolicies.result(action,receipt).blocksSending)
        receipt.getJSONObject("result").put("run_id","run");receipt.put("request_id","other")
        assertEquals("uncertain",SessionActionPolicies.result(action,receipt).status)
    }
    @Test fun originalReceiptAndAuthoritativeResultTargetRemainDistinct() {
        val receipt=receipt(); receipt.getJSONObject("result").put("result_target",JSONObject().put("name","renamed").put("run_id","new-run").put("conversation_id","conversation"))
        val result=SessionActionPolicies.result(action,receipt)
        assertEquals(target,result.target);assertEquals("new-run",result.resultTarget?.run);assertEquals("completed",result.status)
    }
    @Test fun recoveryPersistsAttemptWithoutReplayAndBlocksMessage() {
        val state=MessageCodec.state(MessageCodec.stateJson(MessageState(sessionActions=listOf(action))))
        assertEquals("request",state.sessionActions.single().requestId);assertEquals("uncertain",state.sessionActions.single().status)
        assertThrows(IllegalArgumentException::class.java) { state.enqueue(Draft(target,"text"),"send",1.0) }
    }
    @Test fun queueReceiptMustMatchSelectedImmutableQueueItem() {
        val queue="a".repeat(64)
        val action=action.copy(operation="send_now",arguments=JSONObject().put("queue_id",queue).toString())
        val receipt=receipt();receipt.getJSONObject("result").put("status","submitted").put("queue_id","b".repeat(64))
        assertEquals("uncertain",SessionActionPolicies.result(action,receipt).status)
        receipt.getJSONObject("result").put("queue_id",queue)
        assertEquals("completed",SessionActionPolicies.result(action,receipt).status)
    }
    @Test fun settingsCannotAcknowledgeUnrequestedOrChangedModel() {
        val action=action.copy(operation="settings",arguments=JSONObject().put("model","synthetic").toString())
        val receipt=receipt();receipt.getJSONObject("result").put("status","scheduled").put("model","different")
        assertEquals("uncertain",SessionActionPolicies.result(action,receipt).status)
        receipt.getJSONObject("result").put("model","synthetic").put("pending_settings_id","10000000-0000-4000-8000-000000000001")
        assertEquals("scheduled",SessionActionPolicies.result(action,receipt).status)
    }
    @Test fun arbitraryActionArgumentsAndMalformedQueueIdsRejected() {
        assertThrows(IllegalArgumentException::class.java) { SessionActionPolicies.arguments("pause",JSONObject().put("action","terminate")) }
        assertThrows(IllegalArgumentException::class.java) { SessionActionPolicies.arguments("send_now",JSONObject().put("queue_id","../other")) }
    }
}
