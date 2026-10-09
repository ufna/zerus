package app.zerus.mobile

import org.json.JSONObject

/** Private display rows supplement an encrypted cache, never native mutation evidence. */
object ConversationDisplayCache {
    private const val FIELD = "zerus_mobile_history"
    private val streams = listOf("events", "message_events", "provider_messages", "attachment_messages")
    data class Decoded(val raw: JSONObject, val events: List<Event>, val truncated: Boolean)
    fun encode(target: Target, raw: JSONObject, events: List<Event>, truncated: Boolean): JSONObject {
        val value = JSONObject(raw.toString())
        streams.forEach(value::remove)
        val metadataBytes = value.toString().toByteArray(Charsets.UTF_8).size
        // Leave room for the versioned wrapper and escaping. Mandatory native metadata is preserved.
        val budget = (1_048_576 - metadataBytes - 4096).coerceIn(2, 700 * 1024)
        val retained = RollingConversationHistory.merge(target, target, emptyList(), events, maxBytes = budget)
        return value.put(FIELD, JSONObject().put("schema", 1).put("events", EventHistoryCodec.encode(retained.events))
            .put("truncated", truncated || retained.truncated))
    }
    fun decode(raw: JSONObject): Decoded {
        val value = JSONObject(raw.toString())
        val history = value.optJSONObject(FIELD)
        value.remove(FIELD)
        val rows = if (history?.optInt("schema") == 1) runCatching {
            EventHistoryCodec.decode(history.getJSONArray("events"))
        }.getOrNull() else null
        return Decoded(value, rows ?: NativeParser.events(value), history?.optBoolean("truncated") == true ||
            value.optBoolean("mobile_history_truncated"))
    }
}
