package app.zerus.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.util.Locale
import java.util.TimeZone

class ContextPresentationTest {
    private val now = 1800000000.0
    private fun view(raw: String = "{}", recorded: Boolean = false) = ContextPresentation.from(
        JSONObject(raw), recorded, now, Locale.US, TimeZone.getTimeZone("UTC"))
    private fun raw(used: Any, limit: Any? = null) = JSONObject().put("session_usage", JSONObject()
        .put("status", "ok").put("context", JSONObject().put("used", used).apply { if (limit != null) put("limit", limit) }))
    private fun value(view: ContextView, label: String) = view.details.find { it.label == label }?.value
    private fun live() = JSONObject("""{"run_id":"run","conversation_id":"conversation","state":"running","runtime_state":"live","process_state":"running","phase":"idle","activity":"idle"}""")

    @Test fun missingUsageAndCacheNeverInventMetricsOrWarmth() {
        val result = view()
        assertEquals("Context: Not reported", result.summary)
        assertEquals("Cache: Unknown", result.cacheSummary)
        assertEquals("Not reported", value(result, "Context window"))
        assertFalse(result.cold)
        assertEquals(ContextTone.Muted, result.tone)
    }

    @Test fun onlyNativeFiniteNonnegativeNumbersCountAndZeroIsValid() {
        listOf<Any>("123", -1, 9007199254740992.0).forEach { invalid ->
            assertEquals("Context: Not reported", view(raw(invalid, 1000).toString()).summary)
        }
        val zero = view(raw(0, 1000).toString())
        assertEquals("Context 0% (0/1k)", zero.summary)
        assertEquals("0", value(zero, "Context tokens"))
        assertEquals(ContextTone.Normal, zero.tone)
        // A mathematically overflowing ratio cannot become a fabricated infinity percentage.
        assertEquals(ContextTone.Muted, view(raw(9007199254740991.0, 1e-308).toString()).tone)
    }

    @Test fun unknownWindowCannotBeInferredFromProviderOrModel() {
        val data = raw(12345).put("agent", "codex").put("model", "model-with-an-unknown-window")
        val result = view(data.toString())
        assertEquals("Context 12.3k / Unknown window", result.summary)
        assertEquals("Not reported", value(result, "Window used"))
        assertEquals(ContextTone.Muted, result.tone)
        assertTrue(result.notes.any { it.contains("Percentage is unavailable") })
    }

    @Test fun thresholdsAreDesktopSeverityAndDoNotClaimNativeCompactionRequired() {
        assertEquals(ContextTone.Normal, view(raw(699, 1000).toString()).tone)
        val growing = view(raw(700, 1000).toString())
        assertEquals(ContextTone.Warning, growing.tone)
        assertTrue(growing.notes.any { it.startsWith("Context is growing") })
        val full = view(raw(900, 1000).toString())
        assertEquals(ContextTone.Critical, full.tone)
        assertTrue(full.notes.any { it.startsWith("Context is nearly full") })
        assertEquals("", full.compactionSummary)
        assertEquals(ContextTone.Muted, view(raw(900, 1000).toString(), recorded = true).tone)
    }

    @Test fun estimatedUsageAndNativeModelConfigurationAreExplicit() {
        val data = raw(1500, 2000)
        data.getJSONObject("session_usage").getJSONObject("context").put("estimated", true).put("limit_source", "model_config")
        val result = view(data.toString())
        assertEquals("Context ≈75% (1.5k/2k)", result.summary)
        assertEquals("Approx. 1,500", value(result, "Context tokens"))
        assertEquals("75.0%", value(result, "Window used"))
        assertEquals("Native model configuration", value(result, "Window source"))
    }

    @Test fun lastRequestCacheHitsNeverProveCurrentWarmthOrExpiry() {
        val result = view("""{"session_usage":{"status":"ok","prompt_cache":{"status":"unknown","source":"native_usage"},"last_request":{"input":100000,"cache_read":95000,"output":300}}}""")
        assertEquals("Cache: Unknown", result.cacheSummary)
        assertFalse(result.cold)
        assertEquals("95,000", value(result, "Last request cache read"))
        assertEquals("100,000", value(result, "Last request input"))
        assertEquals("Not reported", value(result, "Estimated cache expiry"))
        assertTrue(result.notes.any { it.contains("No countdown") })
    }

    @Test fun warmNativeDeadlineHasMinuteCountdownAndExactExpiryCrossing() {
        val data = JSONObject().put("session_usage", JSONObject().put("prompt_cache", JSONObject()
            .put("status", "warm").put("expires_at", now + 121).put("ttl_seconds", 300).put("observed_at", now - 179)))
        assertEquals("Cache ~3m", view(data.toString()).cacheSummary)
        assertEquals(3L, ContextPresentation.countdownMinute(data, now))
        assertEquals(2L, ContextPresentation.countdownMinute(data, now + 1))
        assertEquals(1L, ContextPresentation.countdownMinute(data, now + 120))
        assertFalse(ContextPresentation.isCold(data, now + 120))
        assertTrue(ContextPresentation.isCold(data, now + 121))
        assertEquals(0L, ContextPresentation.countdownMinute(data, now + 121))
        assertEquals("300 s", value(view(data.toString()), "Reported cache TTL"))
        data.getJSONObject("session_usage").getJSONObject("prompt_cache").remove("expires_at")
        assertEquals("Warm cache / Expiry unknown", view(data.toString()).cacheSummary)
        assertFalse(ContextPresentation.isCold(data, now + 100000))
        assertNull(ContextPresentation.countdownMinute(data, now))
    }

    @Test fun savingHintDoesNotProveColdButExplicitNativeColdOutranksWarmEstimate() {
        val data = JSONObject("""{"cache_hint":{"status":"saving_hint","tokens":50000},"session_usage":{"prompt_cache":{"status":"warm"}}}""")
        assertFalse(ContextPresentation.isCold(data, now))
        assertTrue(view(data.toString()).notes.any { it.contains("does not prove the cache is cold") })
        data.getJSONObject("cache_hint").put("status", "cold")
        data.getJSONObject("session_usage").getJSONObject("prompt_cache").put("expires_at", now + 300)
        val result = view(data.toString())
        assertTrue(result.cold)
        assertEquals("Cold cache", result.cacheSummary)
        assertEquals("Cold (reported by native agent)", value(result, "Provider cache"))
        assertEquals(0xFFFF9CA8, ContextPresentation.cacheColor(result.cacheTone, true))
    }

    @Test fun staleAndArchivedValuesRemainReadableButCannotOfferActions() {
        val data = live().put("compact_context_supported", true).put("clear_context_supported", true)
        val current = view(data.toString())
        assertEquals("", current.compactReason)
        assertEquals("", current.clearReason)
        val recorded = view(data.toString(), recorded = true)
        assertTrue(recorded.compactReason.contains("Refresh"))
        assertTrue(recorded.notes.any { it.startsWith("Last known") })
        data.put("state", "archived").put("archive_id", "saved-archive")
        assertTrue(view(data.toString()).clearReason.contains("read-only"))
    }

    @Test fun unavailableActionsExplainKnownBusyStateWithoutProviderGuesses() {
        val data = live().put("phase", "working").put("activity", "busy")
        assertEquals("Wait until the session is idle.", view(data.toString()).compactReason)
        data.put("phase", "idle").put("activity", "idle")
        assertEquals("Native compaction is not available for this session.", view(data.toString()).compactReason)
        data.put("compact_context_reason", "Native provider-specific reason")
        assertEquals("Native provider-specific reason", view(data.toString()).compactReason)
    }

    @Test fun onlyMatchingNativeCompactionReceiptDescribesThisConversation() {
        val data = live().put("compact_context_request", JSONObject()
            .put("run_id", "run").put("conversation_id", "conversation").put("status", "submitted"))
        val submitted = view(data.toString())
        assertEquals("Compaction requested", submitted.compactionSummary)
        assertFalse(submitted.compacting)
        data.getJSONObject("compact_context_request").put("status", "completed")
        assertEquals("Last compaction completed", view(data.toString()).compactionSummary)
        data.getJSONObject("compact_context_request").put("conversation_id", "another-conversation")
        assertEquals("", view(data.toString()).compactionSummary)
        data.put("phase", "compacting").put("activity", "busy").put("compaction_started", now - 10)
        assertTrue(view(data.toString()).compacting)
        assertFalse(view(data.toString(), recorded = true).compacting)
        data.put("runtime_state", "saved")
        assertFalse(view(data.toString()).compacting)
    }

    @Test fun usageDetailsRetainNativeZeroCostTotalsAndIncompleteEvidence() {
        val result = view("""{"session_usage":{"status":"ok","model":"native-model","totals":{"total":0,"input":0,"cache_read":0,"output":0},"requests":0,"cost_usd":0,"cost_incomplete":true,"partial":true,"extras":{"api_ms":1200,"tool_ms":500}}}""")
        assertEquals("native-model", value(result, "Model"))
        assertEquals("0", value(result, "Total tokens"))
        assertEquals("0", value(result, "Recorded requests"))
        assertEquals("$0.0000", value(result, "Native cost estimate"))
        assertEquals("1.2 s", value(result, "API time"))
        assertTrue(result.notes.any { it.contains("incomplete cost") })
        assertTrue(result.notes.any { it.contains("Totals are incomplete") })
    }
}
