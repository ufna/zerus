package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SessionPreviewTest {
    private val open = "<send_user_message_question_reply>"
    private val close = "</send_user_message_question_reply>"
    private fun reply(answer: String) = JSONObject().put("questionItemId", "synthetic-question").put("question", "Choose an approach").put("answer", answer)
    private fun envelope(json: String) = open + json + close
    @Test fun completeObjectAndArrayShowHumanAnswers() {
        assertEquals("Answer: Continue", SessionPreview.render(envelope(reply("Continue").toString())))
        assertEquals("Answer: First; Second", SessionPreview.render(envelope(JSONArray().put(reply("First")).put(reply("Second")).toString())))
    }
    @Test fun emptyAnswerRemainsExplicit() {
        assertEquals("Answer: (empty answer)", SessionPreview.render(envelope(reply("").toString())))
        assertEquals("Answer: (empty answer)", SessionPreview.render(envelope(reply(" \n\t").toString())))
        assertEquals("Question response", SessionPreview.render(open + "[{\"answer\":\"\",…"))
    }
    @Test fun exactIdeContextUnwrapsOnlyItsRequest() {
        val prefix = "# Context from my IDE setup:\nOpen files: synthetic.kt\n## My request for Codex:\n"
        assertEquals("Answer: Proceed", SessionPreview.render(prefix + envelope(reply("Proceed").toString())))
        assertEquals("Ordinary request", SessionPreview.render(prefix + "Ordinary request"))
        assertEquals("# Context from my IDE setup:\nNo request marker", SessionPreview.render("# Context from my IDE setup:\nNo request marker"))
    }
    @Test fun ordinaryUnknownTagsAndEmbeddedOrFencedExamplesRemainLiteral() {
        for (text in listOf("Use <button> literally", "<other>example</other>", "Example: $open{}$close", "```xml\n$open{}$close\n```", "The answer key appears inside code")) {
            assertEquals(text, SessionPreview.render(text))
        }
    }
    @Test fun completeMalformedAndWrongShapeEnvelopesRemainLiteral() {
        for (json in listOf("{}", "[]", "42", "{\"questionItemId\":\"q\",\"question\":\"Q\",\"answer\":42}",
            "{'questionItemId':'q','question':'Q','answer':'bad'}", "{\"questionItemId\":\"q\",\"question\":\"Q\",\"answer\":\"bad\",}",
            "[${reply("bad")},]", "${reply("bad")} trailing", "{\"questionItemId\":\"q\",\"question\":\"Q\",\"answer\":1e9999999999}",
            "{\"questionItemId\":\"q\",\"question\":\"Q\",\"answer\":\"\\u٠٠٤١\"}")) {
            val text = envelope(json)
            assertEquals(text, SessionPreview.render(text))
        }
    }
    @Test fun clippedFirstAnswerSupportsObjectAndArray() {
        assertEquals("Answer: Continue…", SessionPreview.render(open + "[{\"answer\":\"Continue\",\"question\":\"long…"))
        assertEquals("Answer: Continue…", SessionPreview.render(open + "{\"answer\":\"Continue\"}…"))
    }
    @Test fun clippedStringDecodesEscapedQuotesSlashesUnicodeAndNewlines() {
        val answer = "Use \"quoted\" / \\ path\nNext 😀"
        val text = open + "[{\"answer\":" + JSONObject.quote(answer) + ",\"question\":\"long…"
        assertEquals("Answer: $answer…", SessionPreview.render(text))
        assertEquals("Answer: café…", SessionPreview.render(open + "[{\"answer\":\"caf\\u00e9\",…"))
        assertEquals("Answer: /…", SessionPreview.render(open + "[{\"answer\":\"\\/\",…"))
    }
    @Test fun partialRequiresNativeTrailingEllipsisAndFirstAnswerProperty() {
        val unclosed = open + "[{\"answer\":\"Continue\",unfinished"
        assertEquals(unclosed, SessionPreview.render(unclosed))
        assertEquals("Question response", SessionPreview.render(open + "[{\"question\":\"answer: yes\",\"answer\":\"Continue\",…"))
        assertEquals("Question response", SessionPreview.render(open + "[{\"other\":\"answer\",…"))
    }
    @Test fun incompleteOrInvalidFirstStringDoesNotGuessAnAnswer() {
        for (body in listOf("[{\"answer\":\"unfinished…", "[{\"answer\":\"bad\\…", "[{\"answer\":\"bad\\u00…", "[{\"answer\":42,…", "[{\"answer\":\"bad\" unexpected…")) {
            assertEquals("Question response", SessionPreview.render(open + body))
        }
    }
    @Test fun boundedOutputPreservesWholeUnicodeCodePoints() {
        val answer = "😀".repeat(500)
        val rendered = SessionPreview.render(envelope(reply(answer).toString()))
        assertEquals(240, rendered.codePointCount(0, rendered.length))
        assertTrue(rendered.endsWith("…"))
        assertFalse(rendered.dropLast(1).last().isHighSurrogate())
        val oversized = envelope(reply("x".repeat(40_000)).toString())
        assertTrue(SessionPreview.render(oversized).codePointCount(0, SessionPreview.render(oversized).length) <= 240)
    }
    @Test fun parserAndSearchUseRenderedPreviewWithoutChangingRawPromptOrIdentity() {
        val rawText = envelope(reply("Proceed carefully").toString())
        val row = JSONObject().put("name", "session").put("run_id", "run").put("conversation_id", "conversation").put("prompt", rawText)
        val machine = JSONObject().put("id", "computer").put("snapshot", JSONObject().put("sessions", JSONArray().put(row)))
        val connection = Connection("workspace", "Test", "https://gateway.example", "synthetic")
        val session = NativeParser.sessions(connection, machine).single()
        assertEquals("Answer: Proceed carefully", session.preview)
        assertEquals(rawText, session.raw.getString("prompt"))
        assertEquals(Target("computer", "session", "run", "conversation", "workspace"), session.target)
        assertEquals(1, SessionFilters.scoped(listOf(session), emptyList(), emptySet(), "Proceed carefully").size)
        assertTrue(SessionFilters.scoped(listOf(session), emptyList(), emptySet(), "questionItemId").isEmpty())
    }
}
