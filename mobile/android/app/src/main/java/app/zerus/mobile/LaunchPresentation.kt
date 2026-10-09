package app.zerus.mobile

import org.json.JSONObject

data class LaunchAccount(val id: String,val provider: String,val label: String)
data class LaunchFolder(val path: String,val name: String)
object LaunchPresentation {
    fun agents(raw: JSONObject?): List<String> = raw?.optJSONArray("agents")?.let { array ->
        (0 until array.length()).mapNotNull { array.opt(it) as? String }.filter { it in setOf("codex","claude","kimi","dsh") }.distinct()
    }.orEmpty()
    fun accounts(raw: JSONObject?,provider: String): List<LaunchAccount> = raw?.optJSONArray("accounts")?.objects().orEmpty().mapNotNull { row ->
        val id = row.opt("id") as? String
        val agent = row.opt("provider") as? String
        val label = row.opt("label") as? String
        if(id.isNullOrBlank() || agent != provider || label == null) null else LaunchAccount(id,agent,label)
    }.distinctBy { it.id }
    fun folders(raw: JSONObject?): List<LaunchFolder> = raw?.optJSONArray("directories")?.objects().orEmpty().mapNotNull { row ->
        val path = row.opt("path") as? String
        if(path == null || !absoluteFolder(path)) null else LaunchFolder(path,(row.opt("name") as? String).orEmpty().ifBlank { path.substringAfterLast('/') })
    }.distinctBy { it.path }
    fun absoluteFolder(path: String) = path.startsWith('/') && path.length <= 4096 && path.none(Char::isISOControl)
    fun selectedFolder(raw: JSONObject?,selected: String): Boolean = absoluteFolder(selected) &&
        (raw?.opt("path") == selected || folders(raw).any { it.path == selected })
    fun nameError(tag: String): String = when {
        tag.isBlank() -> "Enter a session name."
        tag != tag.trim() -> "Remove spaces at the beginning and end."
        tag.toByteArray(Charsets.UTF_8).size > 120 -> "Session name exceeds 120 UTF-8 bytes."
        tag.any { it == '/' || it == '\\' || it == ':' || it == '.' || it.isISOControl() } -> "Use a name without path separators, dots, colons, or control characters."
        else -> ""
    }
}
