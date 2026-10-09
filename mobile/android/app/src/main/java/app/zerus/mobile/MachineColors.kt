package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import kotlin.math.roundToInt

data class MachineColorOverride(val key: MachineKey, val hex: String)

/** Phone-local appearance uses immutable computer identity, never the editable label. */
object MachineColors {
    val palette = listOf("#5fbfa5", "#8ea9f4", "#c098e6", "#e7ae66", "#e38f9e", "#75bfcf", "#acc577")
    val names = listOf("Mint", "Blue", "Purple", "Amber", "Rose", "Cyan", "Lime")

    fun checked(value: String): String {
        require(Regex("#[0-9a-fA-F]{6}").matches(value)) { "Use a six-digit RGB color." }
        return value.lowercase()
    }
    fun automatic(key: MachineKey): String {
        var hash = 2166136261L
        for (byte in key.computerId.toByteArray(Charsets.UTF_8)) {
            hash = ((hash xor (byte.toInt() and 255).toLong()) * 16777619L) and 0xffffffffL
        }
        return palette[(((hash ushr 16) xor hash) % palette.size).toInt()]
    }
    fun overrideHex(state: MessageState, key: MachineKey): String? = state.machineColors.find { it.key == key }?.hex
    fun color(state: MessageState, key: MachineKey): String = overrideHex(state, key) ?: automatic(key)
    fun set(state: MessageState, key: MachineKey, hex: String?): MessageState {
        require(key.connectionId.isNotBlank() && key.computerId.isNotBlank()) { "Choose an existing machine." }
        val next = state.machineColors.filterNot { it.key == key } +
            if (hex == null) emptyList() else listOf(MachineColorOverride(key, checked(hex)))
        require(next.size <= 1000) { "Too many saved machine colors." }
        return state.copy(machineColors = next)
    }
    fun encode(value: MachineColorOverride) = JSONObject().put("connection", value.key.connectionId)
        .put("machine", value.key.computerId).put("color", checked(value.hex))
    fun decodeArray(values: JSONArray?): List<MachineColorOverride> = (0 until minOf(values?.length() ?: 0, 1000))
        .mapNotNull { index -> values?.optJSONObject(index)?.let { raw ->
            runCatching {
                val connection = raw.get("connection") as String
                val machine = raw.get("machine") as String
                require(connection.isNotBlank() && machine.isNotBlank() && connection.length <= 512 && machine.length <= 512)
                MachineColorOverride(MachineKey(connection, machine), checked(raw.get("color") as String))
            }.getOrNull()
        } }.distinctBy { it.key }

    /** Exact desktop MachineAppearance tonal blends, with opaque RGB results. */
    fun foreground(hex: String, dark: Boolean): Int = blend(hex, if (dark) 0xffffff else 0x111820, if (dark) .38 else .48)
    fun background(hex: String, dark: Boolean): Int = blend(hex, if (dark) 0x171e25 else 0xffffff, if (dark) .78 else .83)
    private fun blend(hex: String, other: Int, amount: Double): Int {
        val rgb = checked(hex).drop(1).toInt(16)
        var result = 0xff000000.toInt()
        for (shift in listOf(16, 8, 0)) {
            val value = (((rgb ushr shift) and 255) * (1 - amount) + ((other ushr shift) and 255) * amount).roundToInt()
            result = result or (value shl shift)
        }
        return result
    }
}
