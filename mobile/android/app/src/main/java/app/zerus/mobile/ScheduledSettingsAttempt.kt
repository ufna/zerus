package app.zerus.mobile

/** Original scheduled authorization keeps its one attempt proof even if terminal receipts are pruned. */
object ScheduledSettingsAttempt {
    fun begin(state:MessageState,authorized:SessionAction,attempt:SessionAction):MessageState {
        val source=state.sessionActions.find { it.requestId == authorized.requestId && it.status == "scheduled" && it.applyAttemptId.isBlank() && it.pendingId == authorized.pendingId }
            ?: error("This authorized settings intent already has an attempt.")
        require(source.operation == "settings" && attempt.operation == "settings")
        require(source.target.connectionId == attempt.target.connectionId && source.target.computerId == attempt.target.computerId && source.target.session == attempt.target.session &&
            source.target.conversation.isNotBlank() && source.target.conversation == attempt.target.conversation && attempt.target.agentId.isBlank() && attempt.target.archiveId.isBlank())
        val requested=org.json.JSONObject(source.arguments)
        require(listOf("model","effort").all { !requested.has(it) || requested.get(it) == org.json.JSONObject(attempt.arguments).opt(it) })
        require(org.json.JSONObject(attempt.arguments).string("expected_pending_id") == source.pendingId)
        return state.action(source.copy(applyAttemptId=attempt.requestId)).action(attempt)
    }
}
