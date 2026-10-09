package app.zerus.mobile

/** Only a confirmed user action can route an ordinary editor to an authoritative target. */
object ActionDraftPolicies {
    fun proof(action: SessionAction, from: Target, to: Target): Boolean = action.status == "completed" &&
        action.operation in setOf("rename", "resume") && action.target == from && action.resultTarget == to &&
        from.agentId.isBlank() && to.agentId.isBlank() && from.archiveId.isBlank() && to.archiveId.isBlank() && from.connectionId == to.connectionId && from.computerId == to.computerId &&
        from.conversation.isNotBlank() && from.conversation == to.conversation
    fun moved(state: MessageState, from: Target, to: Target) = from != to && state.sessionActions.any { proof(it,from,to) }
    fun destinationReady(state:MessageState,target:Target)=state.drafts.none { draft -> draft.target==target && draft.questionId.isBlank() && draft.detachedId.isBlank() &&
        (draft.status!="editing" || draft.text.isNotEmpty() || draft.attachments.isNotEmpty() || draft.answers.isNotEmpty() || draft.requestId.isNotEmpty()) }
    fun restore(state:MessageState,action:SessionAction,expected:Draft):MessageState {
        val current=state.drafts.find { it.key==expected.key && it.generation==expected.generation && it.revision==expected.revision && it.attachments==expected.attachments } ?: return state
        if(current.questionId.isNotBlank() || current.detachedId.isNotBlank() || current.status!="editing" || current.target!=action.target) return state
        return promote(state,action)
    }
    fun promote(state: MessageState, action: SessionAction): MessageState {
        val to=action.resultTarget ?: return state
        if(!proof(action,action.target,to) || state.sessionActions.none { it.requestId == action.requestId && proof(it,action.target,to) }) return state
        val editor=state.drafts.find { it.target == action.target && it.questionId.isBlank() && it.detachedId.isBlank() && it.status == "editing" } ?: return state
        if(!destinationReady(state,to)) return state
        return state.copy(drafts=state.drafts.filterNot { it.target==to && it.questionId.isBlank() && it.detachedId.isBlank() }.map { if(it.key == editor.key) it.copy(target=to) else it })
    }
}
