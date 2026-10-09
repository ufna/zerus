package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class NativeEventIdentityTest {
    private fun row(text: String, at: Int, id: String = "") = JSONObject().put("type", "AgentMessage").put("at", at)
        .put("detail", text).also { if (id.isNotBlank()) it.put("message_id", id) }
    private fun history(vararg rows: JSONObject) = JSONObject().put("provider_messages", JSONArray(rows.toList()))
    @Test fun prependingAndGrowingHistoryPreservesUnchangedFallbackKeys() {
        val first = row("First", 2); val second = row("Second", 3)
        val before = NativeParser.events(history(first, second))
        val after = NativeParser.events(history(row("Older", 1), first, second, row("Newer", 4)))
        assertEquals(before.map { it.id }, after.filter { it.text in listOf("First", "Second") }.map { it.id })
    }
    @Test fun identicalTextTimestampRowsRemainDistinctAndKeepKeysWhenUnrelatedRowsArrive() {
        val repeated = row("Repeated", 2)
        val before = NativeParser.events(history(repeated, repeated))
        assertEquals(2, before.size); assertEquals(2, before.map { it.id }.distinct().size)
        val after = NativeParser.events(history(row("Older", 1), repeated, repeated))
        assertEquals(before.map { it.id }, after.filter { it.text == "Repeated" }.map { it.id })
    }
    @Test fun distinctNativeIdsWithSameTextAndTimeAreNeverDeduplicated() {
        val rows = NativeParser.events(history(row("Repeated", 2, "one"), row("Repeated", 2, "two")))
        assertEquals(2, rows.size); assertNotEquals(rows[0].id, rows[1].id)
    }
    @Test fun nativeMessageAndJournalSequenceIdsSurviveContentUpdates() {
        val initial = row("Partial", 2, "one"); val updated = row("Partial completed", 2, "one")
        assertEquals(NativeParser.events(history(initial)).single().id, NativeParser.events(history(updated)).single().id)
        val first = JSONObject().put("type", "UserPromptSubmit").put("seq", 12).put("at", 1).put("detail", "Original")
        val raw = JSONObject().put("events", JSONArray().put(first))
        val id = NativeParser.events(raw).single().id
        first.put("detail", "Updated")
        assertEquals(id, NativeParser.events(raw).single().id)
    }
    @Test fun explicitDuplicateNativeRecordIsCollapsedWithoutCollapsingOtherMessages() {
        val duplicate = row("Same record", 2, "one")
        assertEquals(2, NativeParser.events(history(duplicate, duplicate, row("Same record", 2, "two"))).size)
    }
    @Test fun exactChildInspectionAcceptsOnlyItsOwnExplicitOrNormalizedRows() {
        val own = row("Child reply", 2, "child-one").put("agent_id", "child")
        val normalized = row("Normalized reply", 3, "child-two")
        val parent = row("Parent reply", 4, "parent").put("agent_id", "main")
        val foreign = row("Other reply", 5, "foreign").put("agent_id", "other-child")
        val child = history(own, normalized, parent, foreign).put("agent_id", "child")
        assertEquals(listOf("Child reply", "Normalized reply"), NativeParser.events(child).map { it.text })
        assertEquals(listOf("Normalized reply", "Parent reply"), NativeParser.events(history(own, normalized, parent, foreign)).map { it.text })
    }
    @Test fun foreignChildProviderCannotSuppressExactChildHookStop() {
        val stop = JSONObject().put("type", "Stop").put("seq", 4).put("at", 2).put("detail", "Excerpt").put("agent_id", "child")
        val foreign = row("Excerpt complete", 3, "foreign").put("agent_id", "other-child")
        val raw = history(foreign).put("agent_id", "child").put("events", JSONArray().put(stop))
        assertEquals("Excerpt", NativeParser.events(raw).single().text)
        raw.put("provider_messages", JSONArray().put(foreign.put("agent_id", "child")))
        assertEquals("Excerpt complete", NativeParser.events(raw).single().text)
    }
    @Test fun canonicalPagesPreserveServerOrderAndOriginalProvenance() {
        val first = JSONObject().put("type","UserPromptSubmit").put("seq",3).put("at",9).put("detail","First canonical")
            .put("history_id","one").put("history_stream","journal").put("history_cursor","opaque-one").put("detail_truncated",true)
        val second = JSONObject().put("type","AgentMessage").put("message_id","reply").put("source","codex_transcript").put("at",1).put("detail","Second canonical")
            .put("history_id","two").put("history_stream","provider").put("incoming_seq",7)
        val raw = JSONObject().put("history_epoch","epoch").put("events",JSONArray().put(first).put(second))
        val rows = NativeParser.events(raw)
        assertEquals(listOf("history:one","history:two"),rows.map { it.id })
        assertEquals("journal:journal:You:native:3",rows[0].originalId)
        assertEquals("provider:codex_transcript:AgentMessage:native:reply",rows[1].originalId)
        assertEquals(7L,rows[1].incomingSeq)
        assertEquals("opaque-one",rows[0].historyCursor); assertTrue(rows[0].detailTruncated)
    }
    @Test fun repeatedAmbiguousStopExcerptsAreNotSilentlyRemoved() {
        val first = JSONObject().put("type","Stop").put("seq",1).put("at",2).put("detail","Repeated")
        val second = JSONObject(first.toString()).put("seq",2)
        val raw = history(row("Repeated full",2,"reply")).put("events",JSONArray().put(first).put(second))
        assertEquals(3,NativeParser.events(raw).size)
    }

}
