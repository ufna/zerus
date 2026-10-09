package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener

/** Display-only native reply summary. Raw history and delivery identity are never rewritten. */
object SessionPreview {
    private const val OPEN = "<send_user_message_question_reply>"
    private const val CLOSE = "</send_user_message_question_reply>"
    private const val IDE = "# Context from my IDE setup:\n"
    private const val REQUEST = "\n## My request for Codex:\n"
    private const val MAX_PARSE = 32_768
    private const val MAX_PREVIEW = 240
    fun render(original: String): String {
        if (original.length > MAX_PARSE) return bounded(original)
        var text = original.trim()
        if (text.startsWith(IDE)) {
            val marker = text.lastIndexOf(REQUEST)
            if (marker >= 0) text = text.substring(marker + REQUEST.length).trim()
        }
        if (!text.startsWith(OPEN)) return bounded(if (text != original.trim()) text else original)
        val body = text.substring(OPEN.length).trim(' ', '\t', '\r', '\n')
        if (!text.endsWith(CLOSE)) {
            if (body.startsWith("{") || body.startsWith("[")) {
                if (!text.contains(CLOSE) && text.endsWith('…')) {
                    partialAnswer(body)?.takeIf { it.isNotBlank() }?.let { return bounded("Answer: $it", "…") }
                    return "Question response"
                }
            }
            return bounded(text)
        }
        val json = text.substring(OPEN.length, text.length - CLOSE.length).trim(' ', '\t', '\r', '\n')
        if (!Cursor(json).complete()) return bounded(text)
        val values = runCatching {
            when (val parsed = JSONTokener(json).nextValue()) {
                is JSONObject -> listOf(parsed)
                is JSONArray -> (0 until parsed.length()).map { parsed.opt(it) }
                else -> emptyList()
            }
        }.getOrNull() ?: return bounded(text)
        if (values.isEmpty() || values.size > 16) return bounded(text)
        val answers = values.map { value ->
            val reply = value as? JSONObject ?: return bounded(text)
            if ((reply.opt("questionItemId") as? String).isNullOrEmpty() ||
                (reply.opt("question") as? String).isNullOrBlank()) return bounded(text)
            (reply.opt("answer") as? String)?.let(::emptyAnswer) ?: return bounded(text)
        }
        return bounded("Answer: " + answers.joinToString("; "))
    }
    private fun emptyAnswer(answer: String) = answer.ifBlank { "(empty answer)" }
    private fun partialAnswer(body: String): String? = runCatching {
        val cursor = Cursor(body)
        cursor.space()
        if (cursor.consume('[')) cursor.space()
        if (!cursor.consume('{')) return@runCatching null
        val key = cursor.string() ?: return@runCatching null
        if (JSONTokener(key).nextValue() != "answer" || !cursor.consume(':')) return@runCatching null
        val answer = cursor.string() ?: return@runCatching null
        cursor.space()
        if (cursor.peek() !in listOf(',', '}')) return@runCatching null
        JSONTokener(answer).nextValue() as? String
    }.getOrNull()
    private fun bounded(text: String, suffix: String = ""): String {
        val count = text.codePointCount(0, text.length)
        val limit = MAX_PREVIEW - if (suffix.isEmpty()) 0 else 1
        return if (count > limit) text.substring(0, text.offsetByCodePoints(0, MAX_PREVIEW - 1)) + "…" else text + suffix
    }
    /** Strict bounded JSON grammar; org.json alone also accepts malformed JavaScript-like inputs. */
    private class Cursor(private val input: String) {
        private var index = 0
        private val replyDepth = if (input.trimStart(' ', '\t', '\r', '\n').startsWith("[")) 1 else 0
        fun space() { while (index < input.length && input[index] in " \t\r\n") index++ }
        fun peek() = input.getOrNull(index)
        fun consume(char: Char): Boolean { space(); return if (peek() == char) { index++; true } else false }
        fun string(): String? {
            space(); val start = index
            if (peek() != '"') return null
            index++
            while (index < input.length) {
                when (val char = input[index++]) {
                    '"' -> return input.substring(start, index)
                    '\\' -> {
                        val escape = input.getOrNull(index++) ?: return null
                        if (escape == 'u') {
                            repeat(4) { val digit = input.getOrNull(index++) ?: return null; if (digit !in "0123456789abcdefABCDEF") return null }
                        } else if (escape !in "\"\\/bfnrt") return null
                    }
                    else -> if (char.code < 32) return null
                }
            }
            return null
        }
        fun complete(): Boolean { if (!value(0)) return false; space(); return index == input.length }
        private fun value(depth: Int): Boolean {
            if (depth > 16) return false
            space()
            when (peek()) {
                '"' -> return string() != null
                '{' -> {
                    index++; if (consume('}')) return true
                    do {
                        val key = string() ?: return false
                        if (!consume(':')) return false
                        space()
                        if (depth == replyDepth && JSONTokener(key).nextValue() in listOf("answer", "question", "questionItemId") && peek() != '"') return false
                        if (!value(depth + 1)) return false
                    } while (consume(','))
                    return consume('}')
                }
                '[' -> {
                    index++; if (consume(']')) return true
                    do { if (!value(depth + 1)) return false } while (consume(','))
                    return consume(']')
                }
                else -> {
                    val start = index
                    while (index < input.length && input[index] !in " \t\r\n,]}") index++
                    val token = input.substring(start, index)
                    return token in listOf("true", "false", "null") || NUMBER.matches(token)
                }
            }
        }
        companion object { val NUMBER = Regex("-?(0|[1-9][0-9]*)(\\.[0-9]+)?([eE][+-]?[0-9]+)?") }
    }
}
