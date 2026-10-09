package app.zerus.mobile

import org.json.JSONObject
import java.math.RoundingMode
import java.text.DateFormat
import java.text.NumberFormat
import java.util.Currency
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import kotlin.math.ceil

enum class ContextTone { Muted, Normal, Warning, Critical }
data class ContextDetail(val label: String, val value: String)
data class ContextView(val summary: String, val tone: ContextTone, val cacheSummary: String,
    val cacheTone: ContextTone, val compactionSummary: String, val compacting: Boolean,
    val cold: Boolean, val recorded: Boolean, val details: List<ContextDetail>, val notes: List<String>,
    val compactReason: String, val clearReason: String)

/** Presentation of native SessionUsage/CacheStatus fields, without inferred limits or cache lifetimes. */
object ContextPresentation {
    private const val MAX_NUMBER = 9007199254740991.0
    private fun number(data: JSONObject?, key: String): Double? = (data?.opt(key) as? Number)?.toDouble()
        ?.takeIf { it.isFinite() && it >= 0 && it <= MAX_NUMBER }
    private fun expiry(raw: JSONObject?): Double? = number(raw?.optJSONObject("session_usage")?.optJSONObject("prompt_cache"), "expires_at")
        ?.takeIf { it > 0 && it <= 253402300799.0 }
    fun isCold(raw: JSONObject?, nowSeconds: Double = System.currentTimeMillis() / 1000.0): Boolean {
        val cache = raw?.optJSONObject("session_usage")?.optJSONObject("prompt_cache")
        return raw?.optJSONObject("cache_hint")?.string("status") == "cold" ||
            cache?.string("status") == "warm" && expiry(raw)?.let { it <= nowSeconds } == true
    }
    /** Cheap clock key: formatting and detail parsing need change only once per remaining minute. */
    fun countdownMinute(raw: JSONObject?, nowSeconds: Double): Long? {
        if (isCold(raw, nowSeconds)) return 0
        if (raw?.optJSONObject("session_usage")?.optJSONObject("prompt_cache")?.string("status") != "warm") return null
        return expiry(raw)?.let { ceil((it - nowSeconds) / 60).toLong().coerceAtLeast(0) }
    }

    fun from(raw: JSONObject?, recorded: Boolean, nowSeconds: Double = System.currentTimeMillis() / 1000.0,
        locale: Locale = Locale.getDefault(), zone: TimeZone = TimeZone.getDefault()): ContextView {
        val usage = raw?.optJSONObject("session_usage")
        val context = usage?.optJSONObject("context")
        val cache = usage?.optJSONObject("prompt_cache")
        val hint = raw?.optJSONObject("cache_hint")
        val used = number(context, "used")
        val limit = number(context, "limit")?.takeIf { it > 0 }
        val percent = (if (used != null && limit != null) used / limit * 100 else null)?.takeIf { it.isFinite() }
        val approximate = context?.optBoolean("estimated") == true
        fun decimal(value: Double, places: Int) = NumberFormat.getNumberInstance(locale).apply {
            minimumFractionDigits = places; maximumFractionDigits = places; roundingMode = RoundingMode.HALF_UP
        }.format(value)
        fun tokens(value: Double?) = value?.let { decimal(it.toLong().toDouble(), 0) } ?: "Not reported"
        fun short(value: Double): String {
            if (value < 1000) return tokens(value)
            val scaled = value / if (value >= 1000000) 1000000 else 1000
            val text = NumberFormat.getNumberInstance(locale).apply { maximumFractionDigits = 1; roundingMode = RoundingMode.HALF_UP }.format(scaled)
            return text + if (value >= 1000000) "M" else "k"
        }
        fun timestamp(value: Double?) = value?.takeIf { it > 0 && it <= 253402300799.0 }?.let {
            DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.MEDIUM, locale).apply { timeZone = zone }.format(Date((it * 1000).toLong()))
        } ?: "Not reported"
        val summary = when {
            used == null -> "Context: Not reported"
            percent != null -> "Context ${if (approximate) "≈" else ""}${decimal(percent, 0)}% (${short(used)}/${short(limit!!)})"
            else -> "Context ${if (approximate) "≈" else ""}${short(used)} / Unknown window"
        }
        val tone = when {
            recorded || percent == null -> ContextTone.Muted
            percent >= 90 -> ContextTone.Critical
            percent >= 70 -> ContextTone.Warning
            else -> ContextTone.Normal
        }
        val cold = isCold(raw, nowSeconds)
        val deadline = expiry(raw)
        val cacheStatus = cache?.string("status").orEmpty()
        val remaining = deadline?.minus(nowSeconds)
        val cacheSummary = when {
            cold -> "Cold cache"
            cacheStatus == "warm" && remaining != null && remaining > 0 -> "Cache ~${ceil(remaining / 60).toLong()}m"
            cacheStatus == "warm" -> "Warm cache / Expiry unknown"
            else -> "Cache: Unknown"
        }
        val details = mutableListOf(
            ContextDetail("Context tokens", (if (approximate && used != null) "Approx. " else "") + tokens(used)),
            ContextDetail("Context window", tokens(limit)),
            ContextDetail("Window used", percent?.let { decimal(it, 1) + "%" } ?: "Not reported"))
        if (context?.string("limit_source") == "model_config") details += ContextDetail("Window source", "Native model configuration")
        usage?.string("model")?.takeIf { it.isNotBlank() }?.let { details += ContextDetail("Model", it) }
        details += ContextDetail("Provider cache", when {
            hint?.string("status") == "cold" -> "Cold (reported by native agent)"
            cold -> "Estimated lifetime ended"
            cacheStatus == "warm" -> "Warm (native usage estimate)"
            else -> "Unknown"
        })
        details += ContextDetail("Estimated cache expiry", timestamp(deadline))
        number(cache, "ttl_seconds")?.let { details += ContextDetail("Reported cache TTL", tokens(it) + " s") }
        number(cache, "observed_at")?.takeIf { it > 0 }?.let { details += ContextDetail("Cache observation", timestamp(it)) }
        cache?.string("source")?.takeIf { it.isNotBlank() }?.let { details += ContextDetail("Cache evidence", it.replace('_', ' ')) }
        val last = usage?.optJSONObject("last_request")
        listOf("input" to "Last request input", "cache_read" to "Last request cache read", "cache_write" to "Last request cache write",
            "output" to "Last request output", "reasoning" to "Last request reasoning").forEach { (key, label) ->
            (number(last, key) ?: number(cache, key))?.let { details += ContextDetail(label, tokens(it)) }
        }
        val notes = mutableListOf<String>()
        if (recorded) notes += "Last known values. Live context and cache state are not verified."
        if (used == null) notes += "The provider has not reported context usage for this conversation yet."
        else if (limit == null) notes += "The provider has not reported the window size. Percentage is unavailable."
        if (approximate) notes += "Approximate context size reported by the provider."
        if (percent != null && percent >= 90) notes += "Context is nearly full. Native compaction may reduce history but can omit details and uses tokens."
        else if (percent != null && percent >= 70) notes += "Context is growing. Consider native compaction if you no longer need the full history."
        if (hint?.string("status") == "cold") notes += "The agent reports a cold cache${number(hint, "tokens")?.takeIf { it > 0 }?.let { " (${tokens(it)} tokens)" }.orEmpty()}. Continuing rebuilds cached context."
        else if (cold) notes += "The estimated cache lifetime has ended. Continuing may rebuild cached context."
        if (hint?.string("status") == "saving_hint") notes += "The native agent suggests /clear for a new task${number(hint, "tokens")?.let { " to save ${tokens(it)} context tokens" }.orEmpty()}. This does not prove the cache is cold."
        if (deadline == null) notes += "The agent has not reported a cache expiry. No countdown is available."
        notes += "Prompt, model or tool changes can invalidate cache earlier. Last-request cache hits do not prove current warmth."
        if (usage?.string("status") == "ok") {
            val totals = usage.optJSONObject("totals")
            details += ContextDetail("Total tokens", tokens(number(totals, "total")))
            listOf("input" to "Input (including cache)", "uncached_input" to "Uncached input", "cache_read" to "Cache read",
                "cache_write" to "Cache write", "output" to "Output", "reasoning" to "Reasoning (in output)",
                "web_searches" to "Web searches", "web_fetches" to "Web fetches").forEach { (key, label) ->
                number(totals, key)?.let { details += ContextDetail(label, tokens(it)) }
            }
            number(usage, "requests")?.let { details += ContextDetail("Recorded requests", tokens(it)) }
            val cost = number(usage, "cost_usd")
            details += ContextDetail(if (cost == null) "Cost" else "Native cost estimate", cost?.let {
                NumberFormat.getCurrencyInstance(locale).apply { currency = Currency.getInstance("USD"); minimumFractionDigits = 4; maximumFractionDigits = 4 }.format(it)
            } ?: "Not reported")
            if (cost != null) notes += when {
                usage.optBoolean("cost_incomplete") -> "The provider reports incomplete cost data."
                usage.optBoolean("cost_stale") -> "Last saved native cost snapshot. Newer requests are not included yet."
                else -> "Provider estimate from its latest saved snapshot; may differ from the bill."
            }
            val extras = usage.optJSONObject("extras")
            listOf("api_ms" to "API time", "tool_ms" to "Tool time").forEach { (key, label) ->
                number(extras, key)?.let { details += ContextDetail(label, decimal(it / 1000, 1) + " s") }
            }
            listOf("lines_added" to "Lines added", "lines_removed" to "Lines removed").forEach { (key, label) ->
                number(extras, key)?.let { details += ContextDetail(label, tokens(it)) }
            }
            notes += if (usage.optBoolean("partial")) "Reading session history. Totals are incomplete."
                else "This conversation only. Subagent usage is separate. Cache is included in input; reasoning is included in output."
        } else notes += "No native usage totals reported for this conversation yet."
        val compactRequest = raw?.optJSONObject("compact_context_request")?.takeIf {
            it.string("run_id") == raw?.string("run_id") && it.string("conversation_id") == raw?.string("conversation_id")
        }
        val compacting = raw?.string("phase") == "compacting" && raw.string("runtime_state") == "live" &&
            raw.string("process_state") == "running" && raw.string("state") != "archived"
        val compactionSummary = when {
            compacting -> "Compacting context…"
            compactRequest?.string("status") == "submitted" -> "Compaction requested"
            compactRequest?.string("status") == "compacting" -> "Compaction in progress"
            compactRequest?.string("status") == "completed" -> "Last compaction completed"
            compactRequest?.string("status") == "unchanged" -> "No history compacted"
            compactRequest?.string("status") == "failed" -> "Compaction failed"
            compactRequest?.string("status") == "cancelled" -> "Compaction cancelled"
            compactRequest?.string("status") == "uncertain" -> "Compaction unconfirmed"
            else -> ""
        }
        if (compactionSummary.isNotBlank()) details += ContextDetail("Native compaction", compactionSummary)
        number(raw, "compaction_started")?.takeIf { it > 0 }?.let { details += ContextDetail("Compaction started", timestamp(it)) }
        compactRequest?.let { request -> number(request, "completed_at")?.let { details += ContextDetail("Compaction completed", timestamp(it)) } }
        return ContextView(summary, tone, cacheSummary, if (recorded) ContextTone.Muted else if (cold) ContextTone.Critical else if (cacheStatus == "warm") ContextTone.Normal else ContextTone.Muted,
            compactionSummary, compacting && !recorded, cold, recorded, details, notes,
            actionReason(raw, recorded, "compact_context"), actionReason(raw, recorded, "clear_context"))
    }

    private fun actionReason(raw: JSONObject?, recorded: Boolean, operation: String): String = when {
        raw?.string("state") == "archived" || !raw?.string("archive_id").isNullOrBlank() -> "Archived conversations are read-only."
        recorded || raw == null -> "Refresh to verify the session before changing context."
        raw.string("runtime_state") != "live" || raw.string("process_state") != "running" -> "The native agent is not running."
        raw.string("run_id").isBlank() || raw.string("conversation_id").isBlank() -> "Wait for the native conversation to be initialized."
        raw.string("phase") != "idle" || raw.string("activity") != "idle" -> "Wait until the session is idle."
        raw.optBoolean(operation + "_supported") -> ""
        raw.string(operation + "_reason").isNotBlank() -> raw.string(operation + "_reason")
        else -> if (operation == "compact_context") "Native compaction is not available for this session." else "Native context clearing is not available for this session."
    }

    fun color(tone: ContextTone, dark: Boolean): Long = when (tone) {
        ContextTone.Muted -> if (dark) 0xFFA1ADBB else 0xFF647386
        ContextTone.Normal -> if (dark) 0xFF72CDB2 else 0xFF237A62
        ContextTone.Warning -> if (dark) 0xFFF0A35B else 0xFFBD6519
        ContextTone.Critical -> if (dark) 0xFFF07878 else 0xFFCE3D47
    }
    fun cacheColor(tone: ContextTone, dark: Boolean): Long = if (tone == ContextTone.Critical)
        if (dark) 0xFFFF9CA8 else 0xFFB52D48 else color(tone, dark)
}
