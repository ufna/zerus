package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class QuestionReplyPresentationTest {
    private fun item(answer: String = "literal <tag>\n```code```") = JSONObject()
        .put("questionItemId", "native-1").put("question", "Keep both paths?").put("answer", answer)
    private fun envelope(body: String) = "<send_user_message_question_reply>\n$body\n</send_user_message_question_reply>"

    @Test fun completeObjectPreservesLiteralText() {
        val result = QuestionReplyPresentation.parse(envelope(item().toString()))!!.single()
        assertEquals("Keep both paths?", result.question)
        assertEquals("literal <tag>\n```code```", result.answer)
    }
    @Test fun completeArrayPreservesAllAnswersWithoutPreviewLimit() {
        val long = "answer ".repeat(10000)
        val result = QuestionReplyPresentation.parse(envelope(JSONArray().put(item(long)).put(item("")).toString()))!!
        assertEquals(2, result.size)
        assertEquals(long, result[0].answer)
        assertEquals("", result[1].answer)
    }
    @Test fun nativeIdePrefixUsesFinalRequestMarkerOnly() {
        assertNotNull(QuestionReplyPresentation.parse("# Context from my IDE setup:\ncontext\n## My request for Codex:\n" + envelope(item().toString())))
        assertNull(QuestionReplyPresentation.parse("# Context from my IDE setup:\n" + envelope(item().toString())))
        assertNull(QuestionReplyPresentation.parse("unrelated preface\n" + envelope(item().toString())))
    }
    @Test fun embeddedExampleOrTruncatedEnvelopeRemainsLiteral() {
        assertNull(QuestionReplyPresentation.parse("Example: " + envelope(item().toString())))
        assertNull(QuestionReplyPresentation.parse(envelope(item().toString()) + " trailing"))
        assertNull(QuestionReplyPresentation.parse(envelope(item().toString()).dropLast(10) + "…"))
    }
    @Test fun oneMalformedArrayMemberRejectsEntirePresentation() {
        assertNull(QuestionReplyPresentation.parse(envelope(JSONArray().put(item()).put(JSONObject().put("answer", "x")).toString())))
        assertNull(QuestionReplyPresentation.parse(envelope("[]")))
        assertNull(QuestionReplyPresentation.parse(envelope("null")))
    }
    @Test fun requiredFieldsHaveExactStringTypes() {
        listOf(item().put("answer", true), item().put("question", 1), item().put("questionItemId", ""), item().put("question", "  ")).forEach {
            assertNull(QuestionReplyPresentation.parse(envelope(it.toString())))
        }
    }
    @Test fun malformedJavascriptSyntaxIsNotJson() {
        listOf("{'questionItemId':'i','question':'q','answer':'a'}", "{questionItemId:\"i\",question:\"q\",answer:\"a\"}",
            "{\"questionItemId\":\"i\",\"question\":\"q\",\"answer\":\"a\",}").forEach {
            assertNull(QuestionReplyPresentation.parse(envelope(it)))
        }
    }
    @Test fun hugeExponentCannotBeCoercedIntoRequiredText() {
        listOf("answer", "question", "questionItemId").forEach { key ->
            val json = listOf("answer", "question", "questionItemId").joinToString(prefix = "{", postfix = "}") {
                "\"$it\":" + if (it == key) "1e999999" else "\"literal\""
            }
            assertNull(QuestionReplyPresentation.parse(envelope(json)))
        }
    }
    @Test fun oversizedHistoryRemainsLiteralRatherThanPreviewTruncated() {
        assertNull(QuestionReplyPresentation.parse(envelope(item("x".repeat(256 * 1024)).toString())))
    }
    @Test fun presentationDoesNotRewriteEventOrItsDeliveryIdentity() {
        val original = Event("e", "You", envelope(item().toString()), "", requestId = "receipt", nativeType = "UserPromptSubmit")
        assertNotNull(QuestionReplyPresentation.parse(original.text))
        assertEquals("You", original.role)
        assertEquals("receipt", original.requestId)
        assertEquals("UserPromptSubmit", original.nativeType)
        assertEquals(envelope(item().toString()), original.text)
    }
    @Test fun onlyExplicitUserRolesUseUserBubble() {
        assertTrue(QuestionReplyPresentation.isOwnRole("You"))
        assertTrue(QuestionReplyPresentation.isOwnRole("You (answer)"))
        assertFalse(QuestionReplyPresentation.isOwnRole("You (malformed)"))
        assertFalse(QuestionReplyPresentation.isOwnRole("Agent"))
    }
}
