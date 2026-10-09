package app.zerus.mobile

object ComposerIdentity {
    // Initial parent conversation promotion keeps the same editor; child lanes never share it.
    fun lane(target: Target) = listOf(target.connectionId, target.computerId, target.session, target.run,
        target.archiveId, target.agentId, target.parentConversation)
}
