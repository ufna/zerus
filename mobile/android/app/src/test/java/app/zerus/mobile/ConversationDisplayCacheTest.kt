package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class ConversationDisplayCacheTest {
    private val target = Target("node", "sample", "run", "conversation", "workspace")
    private fun raw() = JSONObject().put("name", "sample").put("run_id", "run").put("conversation_id", "conversation")
        .put("events", JSONArray()).put("pending_questions", JSONArray().put(JSONObject().put("question_id", "kept")))
    @Test fun retainedHistorySurvivesColdCacheThenTailRefresh() {
        val original = Event("journal:1", "You", "Earlier message", "", 1.0)
        val cached = ConversationDisplayCache.encode(target, raw(), listOf(original), false)
        val restored = ConversationDisplayCache.decode(cached)
        val refreshed = RollingConversationHistory.merge(target, target, restored.events, listOf(Event("journal:180", "Agent", "New reply", "", 180.0)))
        assertEquals(listOf("journal:1", "journal:180"), refreshed.events.map { it.id })
        assertEquals("kept", restored.raw.getJSONArray("pending_questions").getJSONObject(0).getString("question_id"))
        assertFalse(restored.raw.has("zerus_mobile_history"))
    }
    @Test fun displayBudgetDropsWholeOldestRowsAndReportsTruncation() {
        val raw = raw()
        val rows = (1..100).map { Event("event:$it", "You", "x".repeat(20_000), "", it.toDouble()) }
        val cache = ConversationDisplayCache.encode(target, raw, rows, false)
        assertTrue(cache.toString().toByteArray().size <= 1_048_576)
        val restored = ConversationDisplayCache.decode(cache)
        assertTrue(restored.truncated)
        assertEquals("event:100", restored.events.last().id)
        assertTrue(raw.has("events"))
        assertFalse(raw.has("zerus_mobile_history"))
    }
    @Test fun corruptDisplaySupplementCannotBecomeMutationEvidence() {
        val cached = raw().put("zerus_mobile_history", JSONObject().put("schema", 1).put("events", JSONArray().put("invalid")))
        val restored = ConversationDisplayCache.decode(cached)
        assertTrue(restored.events.isEmpty())
        assertEquals("run", restored.raw.getString("run_id"))
        assertFalse(restored.raw.has("zerus_mobile_history"))
    }
}
