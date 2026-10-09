package app.zerus.mobile

import org.json.JSONObject

/** Exact cached inspection may describe status, but never authorizes a native action. */
object SelectedSessionPresentation {
    fun session(selected: Session,inspection: JSONObject?): Session {
        val target = selected.target
        if(inspection == null || inspection.string("name") != target.session || inspection.string("run_id") != target.run ||
            inspection.string("conversation_id") != target.conversation ||
            target.archiveId.isNotBlank() && inspection.string("archive_id") != target.archiveId ||
            target.agentId.isNotBlank() && inspection.string("parent_conversation_id") != target.parentConversation) return selected
        return selected.copy(raw = inspection)
    }
    fun status(selected: Session,inspection: JSONObject?): String {
        val displayed = session(selected,inspection)
        if(selected.target.agentId.isNotBlank()) return SessionDetailsPresentation.agentStatus(displayed.raw.string("display_state","state","status")) +
            if(displayed.raw.optBoolean("read_only")) " / History only" else ""
        if(displayed.raw.string("activity").isBlank() && !displayed.raw.has("tracked")) return "Status not reported"
        return SessionFilters.status(displayed,true)
    }
}
