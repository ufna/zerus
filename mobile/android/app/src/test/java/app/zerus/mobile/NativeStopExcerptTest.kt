package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class NativeStopExcerptTest {
    private val target = Target("computer", "session", "run", "conversation", "workspace")
    // 1,200 Unicode code points, but 1,800 UTF-16 code units.
    private val prefix = "🦀a".repeat(600)
    private val full = prefix + " continuation".repeat(50)
    private val clipped = prefix + "…"
    private fun hook(text: String = clipped, at: Double = 2.0, seq: Int = 1) = JSONObject()
        .put("type", "Stop").put("seq", seq).put("at", at).put("detail", text)
    private fun provider(text: String = full, at: Double = 3.0, id: String = "reply") = JSONObject()
        .put("type", "AgentMessage").put("message_id", id).put("at", at).put("detail", text)
    private fun inspect(hooks: List<JSONObject>, providers: List<JSONObject>) = JSONObject()
        .put("events", JSONArray(hooks)).put("provider_messages", JSONArray(providers))
    private fun parsed(hooks: List<JSONObject>, providers: List<JSONObject>) = NativeParser.events(inspect(hooks, providers))
    private fun separate(hooks: List<JSONObject>, providers: List<JSONObject>) =
        parsed(hooks, emptyList()) + parsed(emptyList(), providers)
    private fun cached(rows: List<Event>) = RollingConversationHistory.merge(target, target,
        EventHistoryCodec.decode(EventHistoryCodec.encode(rows)), emptyList())

    @Test fun nativeUnicodeClippingShapeMatchesLongReplyWithoutChangingItsTextOrIdentity() {
        assertEquals(1200, prefix.codePointCount(0, prefix.length))
        assertTrue(full.codePointCount(0, full.length) > 1500)
        assertFalse(full.contains(clipped))
        assertTrue(NativeStopExcerpt.matches(full, clipped))
        val rawHook = hook()
        val expected = parsed(emptyList(), listOf(provider())).single()
        assertEquals(listOf(expected), parsed(listOf(rawHook), listOf(provider())))
        assertEquals(clipped, rawHook.getString("detail"))
    }

    @Test fun previouslyCachedDuplicateIsRepairedWithoutIncomingRowsAndExportsReadAlias() {
        val rows = separate(listOf(hook()), listOf(provider()))
        assertEquals(2, rows.size)
        val result = cached(rows)
        assertEquals(listOf(rows[1]), result.events)
        assertEquals(mapOf(rows[0].id to rows[1].id), result.supersededIds)
    }

    @Test fun nonmatchingLongExcerptRemainsVisibleInBothPaths() {
        val unrelated = provider("Different " + full)
        val differentHook = hook("z".repeat(1200) + "…")
        assertEquals(2, parsed(listOf(differentHook), listOf(unrelated)).size)
        val result = cached(separate(listOf(differentHook), listOf(unrelated)))
        assertEquals(2, result.events.size)
        assertTrue(result.supersededIds.isEmpty())
    }

    @Test fun emptyOrEllipsisOnlyTextCannotSupersedeAReply() {
        listOf("", " \n\t", "…", " … ", "……").forEach { excerpt ->
            assertFalse(NativeStopExcerpt.matches("Prefix $excerpt suffix", excerpt))
            val rows = listOf(Event("journal:hook", "AgentMessage", excerpt, "", 2.0, nativeType = "Stop"),
                Event("provider:reply", "AgentMessage", "Prefix $excerpt suffix", "", 3.0, nativeType = "AgentMessage"))
            assertEquals(2, cached(rows).events.size)
        }
        assertEquals(2, parsed(listOf(hook("…")), listOf(provider("Reply … complete"))).size)
    }

    @Test fun shortLiteralEllipsisAndWrongLengthAreNotTreatedAsClipping() {
        assertFalse(NativeStopExcerpt.matches("Wait for the answer", "Wait…"))
        assertTrue(NativeStopExcerpt.matches("Wait… for the answer", "Wait…"))
        assertFalse(NativeStopExcerpt.matches(full, prefix.dropLast(1) + "…"))
        assertFalse(NativeStopExcerpt.matches(full, prefix + "a…"))
        assertEquals(2, parsed(listOf(hook("Wait…")), listOf(provider("Wait for the answer"))).size)
        assertEquals(2, cached(separate(listOf(hook("Wait…")), listOf(provider("Wait for the answer")))).events.size)
    }

    @Test fun strictTenSecondBoundaryRemainsInBothPaths() {
        listOf(11.999 to 1, 12.0 to 2, 12.001 to 2, -7.999 to 1, -8.0 to 2).forEach { (at, count) ->
            assertEquals(count, parsed(listOf(hook()), listOf(provider(at = at))).size)
            val result = cached(separate(listOf(hook()), listOf(provider(at = at))))
            assertEquals(count, result.events.size)
            assertEquals(count == 1, result.supersededIds.isNotEmpty())
        }
    }

    @Test fun multipleProvidersOrHooksCannotSelectAnArbitraryReplacement() {
        val cases = listOf(
            listOf(hook()) to listOf(provider(id = "one"), provider(id = "two")),
            listOf(hook(seq = 1), hook(seq = 2)) to listOf(provider()))
        cases.forEach { (hooks, providers) ->
            val count = hooks.size + providers.size
            assertEquals(count, parsed(hooks, providers).size)
            val result = cached(separate(hooks, providers))
            assertEquals(count, result.events.size)
            assertTrue(result.supersededIds.isEmpty())
        }
    }

    @Test fun nativeTypeAndExactChildScopeRemainRequired() {
        val wrongType = provider().put("type", "Stop")
        assertEquals(2, parsed(listOf(hook()), listOf(wrongType)).size)
        val childHook = hook().put("agent_id", "child")
        val foreign = provider().put("agent_id", "foreign")
        val raw = inspect(listOf(childHook), listOf(foreign)).put("agent_id", "child")
        assertEquals(clipped, NativeParser.events(raw).single().text)
        raw.put("provider_messages", JSONArray().put(provider().put("agent_id", "child")))
        assertEquals(full, NativeParser.events(raw).single().text)
        val rows = separate(listOf(hook()), listOf(provider()))
        assertEquals(2, cached(rows.map { if (it.id.startsWith("provider:")) it.copy(nativeType = "Stop") else it }).events.size)
        assertTrue(RollingConversationHistory.merge(target, target.copy(conversation = "other"), rows, emptyList()).events.isEmpty())
    }
}
