package app.zerus.mobile

import org.json.JSONObject

data class MachineAlias(val key: MachineKey, val name: String)
object MachineNames {
    fun checked(value: String): String {
        val name = value.trim()
        require(name.isNotEmpty() && name.codePointCount(0, name.length) <= 80 &&
            name.none { Character.isISOControl(it) || Character.getType(it) == Character.FORMAT.toInt() }) {
            "Use a name of 1–80 characters without control characters."
        }
        return name
    }
    fun apply(machine: Machine, aliases: List<MachineAlias>) = machine.copy(name =
        aliases.find { it.key == MachineKey(machine.connectionId, machine.id) }?.name ?: machine.nativeName)
    fun set(state: MessageState, key: MachineKey, name: String?): MessageState = state.copy(machineAliases =
        state.machineAliases.filterNot { it.key == key } + if (name == null) emptyList() else listOf(MachineAlias(key, checked(name))))
    fun encode(alias: MachineAlias) = JSONObject().put("connection",alias.key.connectionId).put("machine",alias.key.computerId).put("name",alias.name)
    fun decode(raw: JSONObject) = MachineAlias(MachineKey(raw.getString("connection"),raw.getString("machine")),checked(raw.getString("name")))
}
