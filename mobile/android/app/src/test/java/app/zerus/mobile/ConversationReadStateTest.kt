package app.zerus.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class ConversationReadStateTest {
    private val target=Target("computer","session","run","conversation","workspace")
    @Test fun openingAndViewportDoNotAcknowledgeMessages() {
        val state=ConversationReadState().viewport(target,"incoming",24)
        assertTrue(state.record(target)!!.seen.isEmpty())
        assertEquals(SavedConversationViewport("incoming",24),state.record(target)!!.viewport)
    }
    @Test fun viewedRowsAndAnchorSurviveRestartWithoutCrossConversationLeak() {
        val state=ConversationReadState().viewed(target,setOf("incoming:1","incoming:2")).viewport(target,"incoming:1",30)
        val restored=ConversationReadPolicies.decode(ConversationReadPolicies.encode(state))
        assertEquals(state,restored)
        assertNull(restored.record(target.copy(conversation="other")))
        assertNull(restored.record(target.copy(connectionId="other")))
        assertNull(restored.record(target.copy(agentId="child",parentConversation="conversation")))
        assertEquals(state.record(target),restored.record(target.copy(run="resumed")))
    }
    @Test fun capacityDoesNotEraseSeenRowsOrInventExactUnreadCounts() {
        val ids=(1..5000).map { "message:$it" }.toSet()
        val state=ConversationReadState().viewed(target,ids).viewed(target,setOf("new"))
        assertEquals(ids,state.record(target)!!.seen.toSet())
        assertTrue(state.record(target)!!.incomplete)
    }
    @Test fun toolsAndOwnRowsNeverAdvanceIncomingReadState() {
        assertFalse(ConversationReadPolicies.incoming(Event("own","You","text","",nativeType="UserPromptSubmit")))
        assertFalse(ConversationReadPolicies.incoming(Event("tool","Tool","text","",nativeType="ToolFinished")))
        assertTrue(ConversationReadPolicies.incoming(Event("reply","AgentMessage","text","",nativeType="AgentMessage")))
    }
    @Test fun encryptedIndexContainsOnlyNavigationDisplayFields() {
        val raw=JSONObject().put("state","running").put("account_id","secret-account").put("token","secret-token").put("mobile_capabilities",JSONObject().put("operations","send"))
        val session=Session(target,"Title","codex","busy","Project","Preview",0,raw)
        val row=ConversationIndex.capture(listOf(session),listOf(Machine("workspace","computer","Computer",true))).single()
        val encoded=ConversationIndex.encode(row).toString()
        assertFalse(encoded.contains("secret"));assertFalse(encoded.contains("capabilities"))
        val restored=ConversationIndex.decode(JSONObject(encoded)).session()
        assertTrue(restored.raw.optBoolean("last_known"));assertEquals(target,restored.target)
    }
}
