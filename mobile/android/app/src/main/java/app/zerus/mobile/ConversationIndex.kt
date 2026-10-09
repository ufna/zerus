package app.zerus.mobile

import org.json.JSONObject

/** Bounded encrypted navigation index. It carries no credentials or mutation capabilities. */
data class IndexedConversation(val target: Target, val title: String, val agent: String, val project: String,
    val preview: String, val computerName: String, val state: String, val archivedAt: Double = 0.0) {
    fun session(): Session = Session(target,title,agent,state,project,preview,0,JSONObject().put("name",target.session)
        .put("run_id",target.run).put("conversation_id",target.conversation).put("state",state).put("archived_at",archivedAt)
        .put("archive_id",target.archiveId).put("last_known",true))
}
object ConversationIndex {
    fun capture(sessions: List<Session>, machines: List<Machine>): List<IndexedConversation> = sessions.take(1000).map { session ->
        IndexedConversation(session.target,session.title.take(512),session.agent.take(120),session.project.take(4096),session.preview.take(1024),
            machines.find { it.connectionId == session.target.connectionId && it.id == session.target.computerId }?.nativeName.orEmpty().take(512),
            session.raw.string("state").ifBlank { "running" },session.raw.optDouble("archived_at",0.0).takeIf { it.isFinite() } ?: 0.0)
    }
    fun encode(row: IndexedConversation) = MessageCodec.targetJson(row.target).put("title",row.title).put("agent_name",row.agent)
        .put("project",row.project).put("preview",row.preview).put("computer_name",row.computerName).put("state",row.state).put("archived_at",row.archivedAt)
    fun decode(row: JSONObject) = IndexedConversation(MessageCodec.target(row),row.string("title").take(512),row.string("agent_name").take(120),row.string("project").take(4096),
        row.string("preview").take(1024),row.string("computer_name").take(512),row.string("state"),row.optDouble("archived_at",0.0).takeIf { it.isFinite() } ?: 0.0)
}
