package app.zerus.mobile

/** A confirmed receipt may change presentation, never replay its native command. */
object SessionActionFollowUp {
    enum class Kind { None, Refresh, Sessions, OpenResult }
    fun kind(action:SessionAction,selected:Target?,sameNavigation:Boolean):Kind {
        if(selected!=action.target || !sameNavigation) return Kind.None
        if(action.status!="completed") return Kind.Refresh
        return when(action.operation) {
            "archive","forget" -> Kind.Sessions
            "restore" -> if(openable(action)) Kind.OpenResult else Kind.None
            "rename","resume" -> if(action.resultTarget?.let { ActionDraftPolicies.proof(action,action.target,it) }==true) Kind.OpenResult else Kind.Refresh
            else -> Kind.Refresh
        }
    }
    fun openable(action:SessionAction):Boolean {
        val destination=action.resultTarget ?: return false
        return action.status=="completed" && destination!=action.target && destination.connectionId==action.target.connectionId &&
            destination.computerId==action.target.computerId && destination.agentId.isBlank() &&
            (ActionDraftPolicies.proof(action,action.target,destination) || action.operation=="restore" && action.target.archiveId.isNotBlank() && destination.archiveId.isBlank() && destination.run.isNotBlank())
    }
}
