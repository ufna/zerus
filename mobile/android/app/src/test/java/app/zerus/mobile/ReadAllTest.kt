package app.zerus.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class ReadAllTest {
    private val target=Target("computer","session","run","conversation","workspace")
    private fun evidence(t:Target=target,head:Long=10)=ReadAttentionEvidence(t,"fingerprint","epoch",head,true,emptySet())
    private fun capture(e:ReadAttentionEvidence)=ReadAllCapture(e.target,e)
    private fun session(t:Target=target,raw:JSONObject=JSONObject().put("reply_id","reply"))=Session(t,"Title","codex","idle","Project","reply",0,raw)
    @Test fun batchScopeDeduplicatesWithoutTouchingOtherConversationsOrViewport() {
        val other=target.copy(conversation="other");val outside=target.copy(connectionId="outside")
        val state=ConversationReadState().head(target,"epoch",10,10,true).review(target,true).viewport(target,"anchor",7)
            .head(other,"epoch",10,10,true).head(outside,"epoch",30,30,true)
        val rows=listOf(capture(evidence()),capture(evidence()),capture(evidence(other)))
        val next=ReadAllPolicies.apply(state,rows,rows)
        assertEquals(ReadAllResult(2,0,0),next.result)
        assertEquals(10L,next.state.record(target)!!.readThrough);assertFalse(next.state.record(target)!!.reviewLater)
        assertEquals(state.record(target)!!.viewport,next.state.record(target)!!.viewport)
        assertEquals(state.record(outside),next.state.record(outside))
    }
    @Test fun changedHeadEpochFingerprintAndRunAreSkipped() {
        val initial=ConversationReadState().head(target,"epoch",10,10,true).review(target,true)
        val rows=listOf(capture(evidence()))
        for(changed in listOf(initial.head(target,"epoch",11,11,true),initial.head(target,"new-epoch",10,10,true))) {
            val next=ReadAllPolicies.apply(changed,rows,rows)
            assertEquals(ReadAllResult(0,1,0),next.result);assertEquals(changed,next.state)
        }
        for(current in listOf(listOf(capture(evidence().copy(fingerprint="new"))),listOf(capture(evidence(target.copy(run="new-run")))))) {
            assertEquals(ReadAllResult(0,1,0),ReadAllPolicies.apply(initial,rows,current).result)
        }
    }
    @Test fun changedRowDoesNotPreventIndependentRowFromBeingMarked() {
        val other=target.copy(conversation="other")
        val state=ConversationReadState().head(target,"epoch",11,11,true).review(target,true)
            .head(other,"epoch",10,10,true).review(other,true)
        val rows=listOf(capture(evidence()),capture(evidence(other)))
        val next=ReadAllPolicies.apply(state,rows,rows)
        assertEquals(ReadAllResult(1,1,0),next.result)
        assertEquals(state.record(target),next.state.record(target))
        assertEquals(10L,next.state.record(other)!!.readThrough);assertFalse(next.state.record(other)!!.reviewLater)
    }
    @Test fun partialLoadedRepliesKeepUnknownAndEmptyReminderUnclaimed() {
        val unknown=target.copy(conversation="unknown")
        val reminder=target.copy(conversation="reminder")
        val state=ConversationReadState().review(target,true).review(reminder,true)
        val loaded=ReadAttentionEvidence(target,"","",null,false,setOf("loaded"))
        val empty=ReadAttentionEvidence(reminder,"","",null,false,emptySet())
        val rows=listOf(capture(loaded),ReadAllCapture(unknown,null),capture(empty))
        val next=ReadAllPolicies.apply(state,rows,rows)
        assertEquals(ReadAllResult(1,2,1),next.result)
        assertEquals(listOf("loaded"),next.state.record(target)!!.seen);assertEquals(0L,next.state.record(target)!!.readThrough)
        assertFalse(next.state.record(target)!!.reviewLater);assertTrue(next.state.record(reminder)!!.reviewLater)
        assertNull(next.state.record(unknown));assertFalse(ReadAllPolicies.available(empty))
    }
    @Test fun partialCanonicalUsesCapturedLoadedWatermarkAndNewerRepliesRemainUnread() {
        val state=ConversationReadState().head(target,"epoch",10,10,false)
        val loaded=evidence().copy(authoritative=false,loadedIds=setOf("loaded"),loadedThrough=8)
        val rows=listOf(capture(loaded));val next=ReadAllPolicies.apply(state,rows,rows)
        val record=next.state.record(target)!!
        assertEquals(ReadAllResult(1,0,1),next.result);assertEquals(8L,record.readThrough)
        assertTrue(ConversationReadPolicies.unread(record,Event("new","AgentMessage","reply","",nativeType="AgentMessage",historyEpoch="epoch",incomingSeq=11)))
        assertEquals(ReadAllResult(0,1,0),ReadAllPolicies.apply(state.head(target,"new",10,10,false),rows,rows).result)
    }
    @Test fun batchMarkersAndRemindersPersistAcrossEncodingAndNewHeadsStayUnread() {
        val state=ConversationReadState().head(target,"epoch",10,10,true).review(target,true).viewport(target,"anchor",23)
        val rows=listOf(capture(evidence()));val next=ReadAllPolicies.apply(state,rows,rows).state
        val restored=ConversationReadPolicies.decode(ConversationReadPolicies.encode(next))
        assertEquals(next,restored)
        val newHead=restored.head(target,"epoch",11,11,true).record(target)!!
        assertEquals(10L,newHead.readThrough);assertFalse(newHead.reviewLater);assertEquals("anchor",newHead.viewport!!.eventId)
    }
    @Test fun headCandidatesIncludeUnvisitedReplyEvidenceAndRecordedChatsButExcludeArchivesAndUnknown() {
        val recorded=target.copy(conversation="recorded")
        val state=ConversationReadState().review(recorded,true)
        val unknown=target.copy(conversation="unknown")
        val archived=target.copy(conversation="archive",archiveId="archive-id")
        val rows=listOf(session(),session(recorded,JSONObject()),session(unknown,JSONObject()),session(archived),session(target.copy(conversation="",run="")))
        assertEquals(listOf(target,recorded),ReadAllHeadCandidates.candidates(rows,state).map { it.target })
        assertNotEquals(ReadAllHeadCandidates.hint(session()),ReadAllHeadCandidates.hint(session(target.copy(run="new-run"))))
    }
    @Test fun metadataWarmupRespectsRecordCapAndDistinctConversationKeys() {
        val records=(0 until 499).map { ConversationReadRecord(target.copy(conversation="recorded-$it")) }
        val rows=listOf(session(),session(),session(target.copy(conversation="second")),session(records.first().target,JSONObject()))
        assertEquals(listOf(target,records.first().target),ReadAllHeadCandidates.candidates(rows,ConversationReadState(records)).map { it.target })
        val full=ConversationReadState(records+ConversationReadRecord(target.copy(conversation="last")))
        assertEquals(listOf(records.first().target),ReadAllHeadCandidates.candidates(rows,full).map { it.target })
    }
}
