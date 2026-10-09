package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class ConversationSearchTest {
    @Test fun repeatedTextKeepsExactMessageIdentityWithoutGuessing() {
        val events = listOf(Event("one","Agent","same words",""),Event("two","Agent","same words",""))
        val result = ConversationSearch.find(ConversationSearch.prepare(events),"SAME")
        assertEquals(listOf("one","two"),result.hits.map { it.eventId })
        assertEquals(events[1],ConversationSearch.resolve(events,"two"))
        assertNull(ConversationSearch.resolve(events,"missing"))
    }
    @Test fun fullKnownQuestionAnswerIsSearchableBeyondListPreview() {
        val answer = "x".repeat(400) + " important conclusion"
        val raw = "<send_user_message_question_reply>" + org.json.JSONObject().put("questionItemId","part").put("question","Which option?").put("answer",answer) + "</send_user_message_question_reply>"
        val event = Event("answer","You",raw,"",requestId="request")
        val prepared = ConversationSearch.prepare(listOf(event))
        assertEquals("You (answer)",prepared.single().role)
        assertEquals("answer",ConversationSearch.find(prepared,"important conclusion").hits.single().eventId)
        assertEquals(raw,event.text)
        assertEquals("request",event.requestId)
    }
    @Test fun boundedResultsDoNotInventFullHistoryTotals() {
        val messages = (0 until 130).map { SearchableMessage("id$it","Agent","word",0.0) }
        val result = ConversationSearch.find(messages,"word")
        assertEquals(130,result.matches)
        assertEquals(100,result.hits.size)
        assertEquals(0,ConversationSearch.find(messages,"").matches)
    }
}
