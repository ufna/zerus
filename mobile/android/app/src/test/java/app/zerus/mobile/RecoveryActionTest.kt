package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class RecoveryActionTest {
    private val target=Target("computer","codex/project/chat","run","conversation","workspace")
    private val job="55555555-5555-4555-8555-555555555555"
    private val id="66666666-6666-4666-8666-666666666666"
    private fun args(choice:String="now")=JSONObject().put("job_id",job).put("action",choice)
    private fun raw()=JSONObject().put("recovery",JSONObject().put("id",job).put("state","waiting").put("identity",JSONArray(listOf(target.run,target.conversation,"native-model","native-account","generation"))).put("not_before",99999999))
    @Test fun recoveryOnlyAcceptsExactWaitingJobAndNativePair() {
        assertEquals("",SessionActionPolicies.inspectReason(target,"recovery_action",args(),raw()))
        assertTrue(SessionActionPolicies.inspectReason(target,"recovery_action",args(),raw().apply { getJSONObject("recovery").put("id",id) }).isNotBlank())
        assertTrue(SessionActionPolicies.inspectReason(target,"recovery_action",args(),raw().apply { getJSONObject("recovery").put("state","dispatching") }).isNotBlank())
        assertTrue(SessionActionPolicies.inspectReason(target.copy(conversation="other"),"recovery_action",args(),raw()).isNotBlank())
        assertThrows(IllegalArgumentException::class.java) { SessionActionPolicies.arguments("recovery_action",args("arbitrary")) }
    }
    @Test fun retryScheduledIsAcknowledgementOfSameJobNotClaimOfAgentExecution() {
        val action=SessionAction(id,target,"recovery_action",args().toString())
        val native=JSONObject().put("request_id",id).put("name",target.session).put("run_id",target.run).put("conversation_id",target.conversation)
            .put("job_id",job).put("action","now").put("status","scheduled")
        val receipt=JSONObject().put("request_id",id).put("state","completed").put("result",native)
        assertEquals("completed",SessionActionPolicies.result(action,receipt).status)
        native.put("status","executed")
        assertEquals("uncertain",SessionActionPolicies.result(action,receipt).status)
        native.put("status","scheduled").put("job_id","other")
        assertEquals("uncertain",SessionActionPolicies.result(action,receipt).status)
    }
    @Test fun delayedNativeRecoveryEvidenceChangeCancelsStaleChoice() {
        val before=raw();val evidence=SessionActionPolicies.evidence("recovery_action",before,args())
        before.getJSONObject("recovery").put("not_before",100000000)
        assertNotEquals(evidence,SessionActionPolicies.evidence("recovery_action",before,args()))
    }
}
