package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject

/** Exact native submitted answers are retained as locks, never replayed. */
object QuestionTracking {
    fun remember(state: MessageState, target: Target, questions: List<Question>): MessageState {
        var drafts = state.drafts
        questions.forEach { question ->
            val delivery = QuestionPolicies.submitted(question, target) ?: return@forEach
            if (delivery.requestId.isBlank()) return@forEach
            val existing = drafts.find { it.target == target && it.questionId == question.id && it.questionHash == question.hash && it.detachedId.isBlank() }
            if (existing != null && existing.status in listOf("submitting", "uncertain") && existing.requestId != delivery.requestId) return@forEach
            val answers = JSONArray(delivery.answers.map { answer -> JSONObject().put("question_id", answer.questionId)
                .put("selected_option_ids", JSONArray(answer.optionIds)).put("text", answer.text).put("skip", answer.skip) }).toString()
            val locked = (existing ?: Draft(target, "", questionId = question.id, questionHash = question.hash))
                .copy(status = "submitted", requestId = delivery.requestId, answers = answers)
            if (locked != existing) drafts = drafts.filterNot { it.key == locked.key } + locked
        }
        return if (drafts == state.drafts) state else state.copy(drafts = drafts)
    }
    fun restore(state: MessageState, target: Target, questions: List<Question>): List<Question> = questions.map { question ->
        if (QuestionPolicies.submitted(question, target) != null || question.source != "codex_async" || !question.optional ||
            question.run != target.run || question.conversation != target.conversation) return@map question
        val draft = state.drafts.find { it.target == target && it.questionId == question.id && it.questionHash == question.hash &&
            it.detachedId.isBlank() && it.status == "submitted" && it.requestId.isNotBlank() } ?: return@map question
        val answers = runCatching { JSONArray(draft.answers).objects().map { value -> NativeQuestionAnswer(value.getString("question_id"),
            value.getJSONArray("selected_option_ids").let { array -> (0 until array.length()).map(array::getString) }, value.getString("text"), value.optBoolean("skip")) } }.getOrNull() ?: return@map question
        question.copy(answerDelivery = NativeQuestionDelivery(draft.requestId, question.id, question.hash, target.run, target.conversation, "submitted", answers), delivery = "submitted")
    }
}
