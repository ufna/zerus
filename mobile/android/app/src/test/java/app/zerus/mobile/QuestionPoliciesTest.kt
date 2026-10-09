package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class QuestionPoliciesTest {
    private val target = Target("computer", "session", "run", "conversation", "workspace")
    private val prompt = Prompt("q", "Question", listOf(Choice("first", "First", ""), Choice("second", "Second", "")), false, true, false)
    private fun question(id: String = "request", optional: Boolean = false, canAnswer: Boolean = true, at: Double = 1.0) =
        Question(id, "hash-$id", listOf(prompt), canAnswer, true, optional, "", createdAt = at, run = target.run, conversation = target.conversation)
    private fun submitted(): Question {
        val q = question(optional = true).copy(source = "codex_async")
        return q.copy(delivery = "submitted", answerDelivery = NativeQuestionDelivery("uuid", q.id, q.hash,
            target.run, target.conversation, "submitted", listOf(NativeQuestionAnswer("q", listOf("second"), ""))))
    }
    @Test fun parserPreservesFullDisclosureAndDefaultsOtherFalse() {
        val body = "dangerous command\n".repeat(300)
        val raw = JSONObject().put("pending_questions", JSONArray().put(JSONObject()
            .put("question_id", "id").put("question_hash", "hash").put("approval", true).put("trust_request", true)
            .put("created_at", 123.0).put("source", "claude_tool_approval").put("answer_unavailable_reason", "Exact native reason")
            .put("run_id", "r").put("conversation_id", "c").put("tool_call_id", "tool")
            .put("questions", JSONArray().put(JSONObject().put("id", "q").put("question", "Approve?")
                .put("header", "Header").put("body", body).put("other_label", "Custom").put("other_description", "Native custom disclosure")))))
        val parsed = NativeParser.questions(raw).single()
        assertEquals(body, parsed.prompts.single().body)
        assertEquals("Header", parsed.prompts.single().header)
        assertEquals("Native custom disclosure", parsed.prompts.single().otherDescription)
        assertFalse(parsed.prompts.single().other)
        assertTrue(parsed.approval); assertTrue(parsed.trustRequest)
        assertEquals("Exact native reason", parsed.unavailableReason)
        assertEquals(123.0, parsed.createdAt, 0.0)
        assertEquals("tool", parsed.toolCallId)
    }
    @Test fun parserPreservesTypedSubmittedAnswerInNativeOrder() {
        val q = submitted()
        val delivery = JSONObject().put("request_id", "uuid").put("question_id", q.id).put("question_hash", q.hash)
            .put("run_id", target.run).put("conversation_id", target.conversation).put("status", "submitted")
            .put("answers", JSONArray().put(JSONObject().put("question_id", "q").put("selected_option_ids", JSONArray(listOf("second", "first"))).put("text", "literal")))
        val parsed = NativeParser.questions(JSONObject().put("pending_questions", JSONArray().put(JSONObject().put("answer_delivery", delivery)))).single()
        assertEquals(listOf("second", "first"), parsed.answerDelivery!!.answers.single().optionIds)
        assertEquals("literal", parsed.answerDelivery.answers.single().text)
    }
    @Test fun requiredActionableRequestTakesPriorityOverOptionalAndUnavailable() {
        val unavailable = question("unavailable", canAnswer = false)
        val required = question("required")
        val optional = question("optional", optional = true)
        assertEquals(required, QuestionPolicies.queue(listOf(optional, unavailable, required), target, QuestionPolicies.key(optional)).selected)
    }
    @Test fun optionalNewestFirstSelectionSurvivesPollInsertion() {
        val old = question("old", true, at = 1.0)
        val chosen = question("chosen", true, at = 2.0)
        val fresh = question("new", true, at = 3.0)
        val queue = QuestionPolicies.queue(listOf(old, fresh, chosen), target, QuestionPolicies.key(chosen))
        assertEquals(chosen, queue.selected)
        assertEquals(listOf(fresh, chosen, old), queue.optional)
        assertEquals(1, queue.index)
    }
    @Test fun submittedMatchingRequestIsHiddenFromOfferedCountAndRetainsAnswer() {
        val sent = submitted()
        val next = question("next", true)
        val queue = QuestionPolicies.queue(listOf(sent, next), target, "")
        assertEquals(next, queue.selected)
        assertEquals(1, queue.promptCount)
        assertEquals(listOf(sent), queue.submitted)
        assertEquals(listOf("second"), QuestionPolicies.submitted(sent, target)!!.answers.single().optionIds)
    }
    @Test fun nativeSubmissionMustMatchEveryIdentityField() {
        val sent = submitted()
        val delivery = sent.answerDelivery!!
        listOf(delivery.copy(questionId = "other"), delivery.copy(questionHash = "other"), delivery.copy(run = "other"),
            delivery.copy(conversation = "other"), delivery.copy(status = "uncertain")).forEach {
            assertNull(QuestionPolicies.submitted(sent.copy(answerDelivery = it), target))
        }
        assertNull(QuestionPolicies.submitted(sent, target.copy(run = "reused")))
        assertNull(QuestionPolicies.submitted(sent, target.copy(conversation = "new")))
        assertNull(QuestionPolicies.submitted(sent.copy(source = "hook"), target))
        assertNull(QuestionPolicies.submitted(sent.copy(optional = false), target))
    }
    @Test fun questionHashReuseDoesNotSelectReplacedOptionalRequest() {
        val old = question("same", true)
        val replacement = old.copy(hash = "replacement")
        val newest = question("newest", true, at = 5.0)
        assertEquals(newest, QuestionPolicies.queue(listOf(replacement, newest), target, QuestionPolicies.key(old)).selected)
    }
    @Test fun duplicateIdsAndEmptyQuestionsAreNotOfferedTwice() {
        val q = question()
        assertEquals(1, QuestionPolicies.queue(listOf(q, q, q.copy(id = ""), q.copy(id = "empty", prompts = emptyList())), target, "").promptCount)
    }
    @Test fun missingOptionsNeverCreateUnsupportedFreeformAnswer() {
        val unsupported = prompt.copy(choices = emptyList())
        assertFalse(QuestionPolicies.complete(listOf(unsupported), emptyMap(), mapOf("q" to "invented")))
        assertTrue(QuestionPolicies.answerError(listOf(unsupported), emptyMap(), mapOf("q" to "invented")).isNotBlank())
        assertEquals("", QuestionPolicies.answers(listOf(unsupported), emptyMap(), mapOf("q" to "invented")).getJSONObject(0).getString("text"))
    }
    @Test fun multiAnswersUseSourceOptionOrder() {
        val result = QuestionPolicies.answers(listOf(prompt.copy(multi = true)), mapOf("q" to linkedSetOf("second", "first")), emptyMap())
        assertEquals(listOf("first", "second"), result.getJSONObject(0).getJSONArray("selected_option_ids").let { ids -> (0 until ids.length()).map { ids.getString(it) } })
    }
    @Test fun customAnswerByteAndCommandRulesMatchNative() {
        val p = prompt.copy(other = true)
        assertTrue(QuestionPolicies.answerError(listOf(p), emptyMap(), mapOf("q" to "é".repeat(2049))).isNotBlank())
        assertTrue(QuestionPolicies.answerError(listOf(p), emptyMap(), mapOf("q" to "/command")).isNotBlank())
        assertTrue(QuestionPolicies.answerError(listOf(p), emptyMap(), mapOf("q" to "a\nb")).isNotBlank())
        assertTrue(QuestionPolicies.complete(listOf(p), emptyMap(), mapOf("q" to "literal <tag>")))
    }
    @Test fun approvalRequiresExactNativeIds() {
        val approval = question().copy(approval = true)
        assertFalse(QuestionPolicies.approvalSupported(approval))
        assertTrue(QuestionPolicies.approvalSupported(approval.copy(prompts = listOf(prompt.copy(choices = listOf(Choice("allow", "Yes", ""), Choice("deny", "No", "")))))))
    }
}
