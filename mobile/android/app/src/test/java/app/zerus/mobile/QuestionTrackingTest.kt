package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class QuestionTrackingTest {
    private val target = Target("computer", "session", "run", "conversation", "workspace")
    private val delivery = NativeQuestionDelivery("receipt", "question", "hash", "run", "conversation", "submitted", listOf(NativeQuestionAnswer("prompt", listOf("allow"), "captured")))
    private val card = Question("question", "hash", emptyList(), true, true, true, "", source="codex_async", run="run", conversation="conversation", answerDelivery=delivery)
    @Test fun unseenSubmittedAnswerSurvivesRestartAndStaleNativePoll() {
        val state = QuestionTracking.remember(MessageState(), target, listOf(card))
        val restored = MessageCodec.state(MessageCodec.stateJson(state))
        val stale = card.copy(answerDelivery=null,delivery="")
        assertEquals(delivery, QuestionTracking.restore(restored,target,listOf(stale)).single().answerDelivery)
        assertEquals("submitted", restored.drafts.single().status)
    }
    @Test fun otherConversationOrHashCannotAcquireLock() {
        val state = QuestionTracking.remember(MessageState(), target, listOf(card))
        assertNull(QuestionTracking.restore(state,target,listOf(card.copy(hash="new",answerDelivery=null))).single().answerDelivery)
        assertTrue(QuestionTracking.remember(MessageState(),target.copy(conversation="new"),listOf(card)).drafts.isEmpty())
    }
    @Test fun unrelatedUncertainSubmissionIsPreserved() {
        val draft = Draft(target,"private",status="uncertain",requestId="different",questionId="question",questionHash="hash",answers="original")
        val state = QuestionTracking.remember(MessageState(drafts=listOf(draft)),target,listOf(card))
        assertEquals(draft,state.drafts.single())
    }
}
