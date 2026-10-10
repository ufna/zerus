package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject

/** Presentation ownership only; answers and delivery attempts remain in MessageState. */
internal data class QuestionDisplayState(
    val owner: String,
    val chosen: String = "",
    val expandedId: String = "",
    val expandedHash: String = "",
    val pages: Map<String, Int> = emptyMap(),
    val offsets: Map<String, Int> = emptyMap(),
) {
    fun forTarget(targetKey: String) = if (owner == targetKey) this else QuestionDisplayState(targetKey)
    val expanded get() = expandedId.isNotBlank()
    fun matches(question: Question) = expanded && question.id == expandedId && question.hash == expandedHash
    fun resolve(questions: List<Question>) = questions.singleOrNull(::matches)
    fun open(question: Question) = copy(chosen = QuestionPolicies.key(question), expandedId = question.id, expandedHash = question.hash)
    fun close() = copy(expandedId = "", expandedHash = "")
    fun page(question: Question) = pages[identity(question)] ?: 0
    fun offset(question: Question, page: Int) = offsets[identity(question, page)] ?: 0
    fun withPage(question: Question, page: Int) = copy(pages = bounded(pages, identity(question), page.coerceAtLeast(0)))
    fun withOffset(question: Question, page: Int, offset: Int) = copy(offsets = bounded(offsets, identity(question, page), offset.coerceAtLeast(0)))
    fun encode(): String = JSONObject().put("owner", owner).put("chosen", chosen)
        .put("expanded_id", expandedId).put("expanded_hash", expandedHash)
        .put("pages", JSONObject(pages)).put("offsets", JSONObject(offsets)).toString()

    companion object {
        private fun identity(question: Question, page: Int? = null) = JSONArray().put(question.id).put(question.hash).also {
            if (page != null) it.put(page)
        }.toString()
        private fun bounded(values: Map<String, Int>, key: String, value: Int) =
            (values - key + (key to value)).entries.toList().takeLast(64).associate { it.key to it.value }
        fun decode(value: String): QuestionDisplayState? = runCatching {
            require(value.length <= 256 * 1024)
            val json = JSONObject(value)
            fun entries(name: String): Map<String, Int> {
                val obj = json.optJSONObject(name) ?: return emptyMap()
                return obj.keys().asSequence().take(64).filter { it.length <= 4096 }.associateWith { obj.optInt(it).coerceAtLeast(0) }
            }
            QuestionDisplayState(json.getString("owner"), json.optString("chosen"), json.optString("expanded_id"),
                json.optString("expanded_hash"), entries("pages"), entries("offsets"))
        }.getOrNull()
    }
}
