package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener

data class PresentedQuestionReply(val id: String, val question: String, val answer: String)

/** Display-only parsing. Callers retain the original Event for receipts and delivery matching. */
object QuestionReplyPresentation {
    private const val OPEN = "<send_user_message_question_reply>"
    private const val CLOSE = "</send_user_message_question_reply>"
    private const val IDE = "# Context from my IDE setup:\n"
    private const val REQUEST = "\n## My request for Codex:\n"
    private const val MAX_PARSE = 256 * 1024

    fun isOwnRole(role: String) = role == "You" || role == "You (answer)"

    fun parse(original: String): List<PresentedQuestionReply>? {
        if (original.length > MAX_PARSE) return null
        var text = original.trim()
        if (text.startsWith(IDE)) {
            val marker = text.lastIndexOf(REQUEST)
            if (marker < 0) return null
            text = text.substring(marker + REQUEST.length).trim()
        }
        if (!text.startsWith(OPEN) || !text.endsWith(CLOSE)) return null
        val body = text.substring(OPEN.length, text.length - CLOSE.length)
        if (!Grammar(body).complete()) return null
        val values = runCatching {
            when (val value = JSONTokener(body).nextValue()) {
                is JSONObject -> listOf(value)
                is JSONArray -> (0 until value.length()).map { value.opt(it) }
                else -> emptyList()
            }
        }.getOrNull() ?: return null
        if (values.isEmpty()) return null
        return values.map { value ->
            val reply = value as? JSONObject ?: return null
            val id = reply.opt("questionItemId") as? String ?: return null
            val question = reply.opt("question") as? String ?: return null
            val answer = reply.opt("answer") as? String ?: return null
            if (id.isEmpty() || question.isBlank()) return null
            PresentedQuestionReply(id, question, answer)
        }
    }

    // Android's JSONTokener accepts JavaScript-like malformed input; check JSON grammar first.
    private class Grammar(private val input: String) {
        private var at = 0
        private val replyDepth = if (input.trimStart(' ', '\t', '\r', '\n').startsWith('[')) 1 else 0
        private fun space() { while (at < input.length && input[at] in " \t\r\n") at++ }
        private fun take(c: Char): Boolean { space(); return if (input.getOrNull(at) == c) { at++; true } else false }
        private fun string(): Boolean {
            if (!take('"')) return false
            while (at < input.length) when (val c = input[at++]) {
                '"' -> return true
                '\\' -> {
                    val escape = input.getOrNull(at++) ?: return false
                    if (escape == 'u') repeat(4) {
                        val digit = input.getOrNull(at++) ?: return false
                        if (digit !in "0123456789abcdefABCDEF") return false
                    } else if (escape !in "\"\\/bfnrt") return false
                }
                else -> if (c.code < 32) return false
            }
            return false
        }
        private fun value(depth: Int): Boolean {
            if (depth > 16) return false
            space()
            return when (input.getOrNull(at)) {
                '"' -> string()
                '{' -> {
                    at++; if (take('}')) return true
                    do {
                        space(); val start = at
                        if (!string()) return false
                        val key = JSONTokener(input.substring(start, at)).nextValue()
                        if (!take(':')) return false
                        space()
                        if (depth == replyDepth && key in listOf("answer", "question", "questionItemId") && input.getOrNull(at) != '"') return false
                        if (!value(depth + 1)) return false
                    } while (take(','))
                    take('}')
                }
                '[' -> {
                    at++; if (take(']')) return true
                    do { if (!value(depth + 1)) return false } while (take(','))
                    take(']')
                }
                else -> {
                    val start = at
                    while (at < input.length && input[at] !in " \t\r\n,]}") at++
                    val token = input.substring(start, at)
                    token in listOf("true", "false", "null") || NUMBER.matches(token)
                }
            }
        }
        fun complete(): Boolean { if (!value(0)) return false; space(); return at == input.length }
        companion object { val NUMBER = Regex("-?(0|[1-9][0-9]*)(\\.[0-9]+)?([eE][+-]?[0-9]+)?") }
    }
}
