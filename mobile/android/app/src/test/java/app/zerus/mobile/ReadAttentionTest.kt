package app.zerus.mobile
import org.junit.Assert.*
import org.junit.Test
class ReadAttentionTest {
    private val target=Target("computer","codex/project/chat","run","conversation","workspace")
    @Test fun explicitReadUsesCapturedHeadAndNeverConsumesNewerArrivingReply() {
        val evidence=ReadAttentionEvidence(target,"evidence","epoch",10,true,emptySet())
        val initial=ConversationReadState().head(target,"epoch",10,10,true).review(target,true)
        val next=ReadAttentionPolicies.mark(initial,evidence)
        assertEquals(10,next.record(target)!!.readThrough);assertFalse(next.record(target)!!.reviewLater)
        val changed=initial.head(target,"epoch",11,11,true)
        assertEquals(changed,ReadAttentionPolicies.mark(changed,evidence))
    }
    @Test fun loadedOnlyMarkDoesNotClaimUnloadedMessagesReadAndReviewSurvivesRestart() {
        val state=ConversationReadState().review(target,true)
        val restored=ConversationReadPolicies.decode(ConversationReadPolicies.encode(state))
        assertTrue(restored.record(target)!!.reviewLater)
        val next=ReadAttentionPolicies.mark(restored,ReadAttentionEvidence(target,"","",null,false,setOf("loaded")))
        assertEquals(listOf("loaded"),next.record(target)!!.seen);assertEquals(0,next.record(target)!!.readThrough)
        assertFalse(next.record(target)!!.headComplete);assertFalse(next.record(target)!!.reviewLater)
    }
    @org.junit.Test fun partialCanonicalMarkUsesOnlyCapturedRowAndLeavesLaterReplyUnread() {
        val state=ConversationReadState().head(target,"epoch",600,600,true)
            .visible(target,listOf(Event("history:600","AgentMessage","reply","",nativeType="AgentMessage",historyEpoch="epoch",incomingSeq=600)))
            .head(target,"epoch",null,null,false)
        val captured=ReadAttentionEvidence(target,"captured","epoch",600,false,setOf("history:601"),601)
        val next=ReadAttentionPolicies.mark(state,captured).record(target)!!
        assertEquals(601L,next.readThrough);assertFalse(next.headComplete)
        assertFalse(ConversationReadPolicies.unread(next,Event("history:601","AgentMessage","reply","",nativeType="AgentMessage",historyEpoch="epoch",incomingSeq=601)))
        assertTrue(ConversationReadPolicies.unread(next,Event("history:602","AgentMessage","new reply","",nativeType="AgentMessage",historyEpoch="epoch",incomingSeq=602)))
        val reminder=state.review(target,true)
        val cleared=ReadAttentionPolicies.mark(reminder,captured.copy(loadedIds=emptySet(),loadedThrough=null)).record(target)!!
        assertEquals(600L,cleared.readThrough);assertFalse(cleared.reviewLater)
    }

}
