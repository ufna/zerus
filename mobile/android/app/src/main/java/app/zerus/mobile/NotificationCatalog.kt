package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject

data class NotificationCatalog(val sessions: List<NotificationCurrent>, val identities: Map<String, String?>) {
    fun resolve(connection: String, event: NotificationEvent): Pair<NotificationSlot, NotificationEvent>? =
        identities[event.computer]?.let { NotificationSlot(connection, it, event.session) to event }
}

object NotificationCatalogParser {
    fun parse(connection: Connection, records: List<JSONObject>, aliases: List<MachineAlias>): NotificationCatalog {
        val identities = mutableMapOf<String, String?>()
        val current = mutableListOf<NotificationCurrent>()
        val unique = records.groupBy { it.getString("id") }.filterValues { it.size == 1 }.values.map { it.single() }
        for (raw in unique) {
            val machine = MachineNames.apply(MachineCatalog.parse(connection.id, raw), aliases)
            for (id in machine.serverAliases + machine.id) {
                if (!identities.containsKey(id)) identities[id] = machine.id
                else if (identities[id] != machine.id) identities[id] = null
            }
            for (session in NativeParser.sessions(connection, raw)) {
                if (session.target.session.isBlank() || session.target.session.length > 1024 || SessionFilters.archived(session)) continue
                val value = session.raw
                val phase = value.string("phase")
                val recovery = value.optJSONObject("recovery")?.string("state").orEmpty()
                val pending = value.optJSONArray("mobile_attention")?.objects().orEmpty().map {
                    JSONArray(listOf(it.string("question_id"), it.string("question_hash"))).toString()
                }.sorted()
                val kind = when {
                    phase == "error" && recovery !in setOf("waiting", "dispatching", "retrying") ||
                        recovery in setOf("uncertain", "blocked", "exhausted") -> SessionAlertKind.Error
                    phase in setOf("input", "approval") || pending.isNotEmpty() || value.optInt("pending_question_count") > 0 -> SessionAlertKind.Input
                    else -> null
                }
                val identity = NotificationPolicy.fingerprint(JSONArray(listOf(phase, value.string("attention_id"),
                    value.opt("question_request"), value.opt("question_requests"), pending, recovery,
                    value.optJSONObject("provider_error")?.let { listOf(it.string("source"), it.string("message_id"), it.string("code", "type"), it.string("message")) },
                    value.optJSONObject("recovery")?.let { listOf(it.string("request_id"), it.string("error", "reason")) })).toString())
                val target = if (session.target.run.isNotBlank() && session.target.conversation.isNotBlank())
                    JSONArray(listOf(session.target.run, session.target.conversation)).toString() else ""
                current += NotificationCurrent(NotificationSlot(connection.id, machine.id, session.target.session), target,
                    clean(SessionFilters.label(session).ifBlank { session.title }, "Session"), clean(machine.name, "Computer"), kind,
                    identity, value.string("activity") == "idle", machine.online,
                    SessionFilters.state(session) == "running" && value.string("process_state") != "exited")
            }
        }
        return NotificationCatalog(current, identities)
    }

    private fun clean(value: String, fallback: String) = value.filterNot {
        Character.isISOControl(it) || Character.getType(it) == Character.FORMAT.toInt()
    }.take(256).ifBlank { fallback }

    fun events(raw: JSONObject): List<NotificationEvent> {
        val values = raw.getJSONArray("events")
        require(values.length() <= 100)
        return values.objects().map { row ->
            val id = row.getLong("id")
            val computer = row.getString("computer_id")
            val session = row.getString("session")
            val at = row.getDouble("created_at")
            require(id > 0 && computer.length in 1..512 && session.length in 1..1024 &&
                session.none { Character.isISOControl(it) } && at.isFinite() && at > 0)
            NotificationEvent(id, computer, session, row.getString("kind"), at)
        }.also { events -> require(events.zipWithNext().all { (a, b) -> a.id < b.id }) }
    }
}
