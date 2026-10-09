package app.zerus.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class HistoryHeadHintsTest {
    private val target=Target("computer","codex/example/session","run","conversation","workspace")
    private fun raw()=JSONObject().put("last_event_at",1791532438.439237).put("turn_started",1.0)
        .put("last_response_at",1791532438.439237).put("phase","idle").put("prompt","synthetic prompt")
    @Test fun nativeDoubleAndClonedIntegerDoNotScheduleAnotherHeadProbe() {
        val native=raw();val cloned=JSONObject(native.toString())
        assertEquals(1.0,(native.get("turn_started") as Number).toDouble(),0.0)
        assertEquals(1,cloned.getInt("turn_started"))
        assertEquals(HistoryHeadHints.signature("preview",native),HistoryHeadHints.signature("preview",cloned))
        assertNotEquals(HistoryHeadHints.signature("preview",native),HistoryHeadHints.signature("new reply",cloned))
        cloned.put("last_response_at",1791532439.0)
        assertNotEquals(HistoryHeadHints.signature("preview",native),HistoryHeadHints.signature("preview",cloned))
    }
    @Test fun capturedReadChoiceSurvivesNumericDisplayCloneButRejectsNewReplyEvidence() {
        val native=raw();val cloned=JSONObject(native.toString())
        val record=ConversationReadRecord(target,epoch="epoch",readThrough=600,headIncoming=602,headComplete=true)
        assertEquals(HistoryHeadHints.readFingerprint(target,native,record),HistoryHeadHints.readFingerprint(target,cloned,record))
        assertNotEquals(HistoryHeadHints.readFingerprint(target,native,record),HistoryHeadHints.readFingerprint(target,cloned,record.copy(headIncoming=603)))
    }
    @Test fun cachedReadWindowWaitsForActualFirstUnreadRowWithoutBlockingPosition() {
        val record=ConversationReadRecord(target,epoch="epoch",readThrough=600,headIncoming=602,headComplete=true,established=true)
        fun row(sequence:Long)=Event("history:$sequence","AgentMessage","reply","",nativeType="AgentMessage",historyId="$sequence",historyEpoch="epoch",incomingSeq=sequence)
        val waiting=OpeningUnreadBoundary.decide(true,record,listOf(row(599),row(600)))
        assertFalse(waiting.captured)
        val actual=OpeningUnreadBoundary.decide(true,record,listOf(row(600),row(601),row(602)))
        assertTrue(actual.captured);assertEquals("history:601",actual.eventId)
        // The caller latches a captured decision; subsequent read progress does not move it.
        assertEquals("history:601",actual.eventId)
        assertTrue(OpeningUnreadBoundary.decide(true,record.copy(established=false),listOf(row(601))).captured)
    }
    @Test fun cachedUnreadHeadIsALowerBoundEvenBeforeAnyPageLoadsAfterRestart() {
        val record=ConversationReadRecord(target,epoch="epoch",readThrough=600,headIncoming=602,headComplete=true)
        assertEquals(2,ConversationReadPolicies.lowerBound(record,null))
        assertNull(ConversationReadPolicies.lowerBound(null,null))
        assertNull(ConversationReadPolicies.lowerBound(record.copy(epoch=""),null))
    }
}
