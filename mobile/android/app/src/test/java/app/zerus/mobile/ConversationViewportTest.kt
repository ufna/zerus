package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class ConversationViewportTest {
    private val keys = listOf("status", "event:first", "event:last", "tail")
    @Test fun firstOpenGoesDirectlyToTail() {
        val request = ConversationViewport.content(ConversationFollowState(), keys, "", null)
        assertEquals(keys.lastIndex, request.index)
        assertTrue(request.state.opened)
        assertTrue(request.state.following)
    }
    @Test fun reopeningRestoresExactMessageAndOffsetAcrossPrependedRows() {
        val anchor = ConversationAnchor("event:first", 42, false)
        val request = ConversationViewport.content(ConversationFollowState(following = false, anchor = anchor),
            listOf("earlier", "status", "event:first", "event:last", "tail"), "", null)
        assertEquals(2, request.index)
        assertEquals(42, request.offset)
        assertFalse(request.state.following)
    }
    @Test fun missingExpiredAnchorFallsBackToLatestRatherThanWrongOldIndex() {
        val request = ConversationViewport.content(ConversationFollowState(anchor = ConversationAnchor("expired", 42, false)), keys, "", null)
        assertEquals(keys.lastIndex, request.index)
        assertEquals(0, request.offset)
        assertTrue(request.state.following)
    }
    @Test fun IncomingMessagesDoNotMoveUserReadingOlderHistory() {
        val reading = ConversationFollowState(opened = true, following = false)
        assertNull(ConversationViewport.content(reading, keys + "new-tail", "", null).index)
        assertEquals(reading, ConversationViewport.content(reading, keys, "", null).state)
    }
    @Test fun ownSendForcesItsOptimisticMessageIntoViewEvenWhenReadingOlderHistory() {
        val reading = ConversationFollowState(opened = true, following = false, ownSendId = "old")
        val request = ConversationViewport.content(reading, keys, "new", "event:last")
        assertEquals(2, request.index)
        assertTrue(request.state.following)
        assertEquals("new", request.state.ownSendId)
    }
    @Test fun pendingOwnSendSignalWaitsForItsDurableOptimisticRow() {
        val reading = ConversationFollowState(opened = true, following = false, ownSendId = "old")
        val pending = ConversationViewport.content(reading, keys, "new", "not-yet-present")
        assertNull(pending.index)
        assertEquals("old", pending.state.ownSendId)
        assertNotNull(ConversationViewport.content(pending.state, keys + "not-yet-present", "new", "not-yet-present").index)
    }
    @Test fun jumpAndUserScrollSetFollowingIntentExplicitly() {
        val following = ConversationViewport.latest(ConversationFollowState(opened = true, following = false))
        assertEquals(keys.lastIndex, ConversationViewport.content(following, keys, "", null).index)
        assertFalse(ConversationViewport.userScrolled(following, false).following)
        assertTrue(ConversationViewport.userScrolled(following, true).following)
    }
    @Test fun shortRestoredConversationFollowsWhenTailIsAlreadyFullyVisible() {
        val reading = ConversationFollowState(opened = true,following = false)
        assertTrue(ConversationViewport.settled(reading,true,false).following)
        assertFalse(ConversationViewport.settled(reading,true,true).following)
        assertFalse(ConversationViewport.settled(reading,false,false).following)
        assertFalse(ConversationViewport.settled(reading.copy(opened = false),true,false).following)
    }
    @Test fun emptyContentDoesNotConsumeFirstOpenOrSendSignal() {
        val waiting = ConversationFollowState()
        assertEquals(ConversationScroll(waiting), ConversationViewport.content(waiting, emptyList(), "new", "new"))
    }
}
