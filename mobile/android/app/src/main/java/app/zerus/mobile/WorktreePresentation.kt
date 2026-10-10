package app.zerus.mobile

import org.json.JSONObject

data class LaunchWorktree(val path: String, val branch: String, val kind: String)
object WorktreePresentation {
    fun healthy(raw: JSONObject?) = raw?.string("state") == "ok" && raw.opt("stale") == false &&
        LaunchPresentation.absoluteFolder(raw.string("common_dir")) && LaunchPresentation.absoluteFolder(raw.string("path"))
    fun entries(raw: JSONObject?): List<LaunchWorktree> = if (!healthy(raw)) emptyList() else
        raw?.optJSONArray("worktrees")?.objects().orEmpty().mapNotNull { row ->
            val path = row.opt("path") as? String
            val kind = row.opt("kind") as? String
            if (path == null || !LaunchPresentation.absoluteFolder(path) || row.opt("available") != true || kind !in setOf("main", "linked")) null
            else LaunchWorktree(path, row.string("branch"), kind!!)
        }.distinctBy { it.path }.take(512)
    private fun within(path: String, root: String) = path == root
    fun contains(raw: JSONObject?, path: String) = entries(raw).any { within(path, it.path) }
    fun anchor(raw: JSONObject?, project: LaunchProject, directory: String): LaunchProjectFolder? {
        if (!healthy(raw) || !contains(raw, directory)) return null
        return project.folders.firstOrNull { folder -> contains(raw, folder.path) ||
            folder.path == raw?.string("requested_path") && contains(raw, raw.string("path")) }
    }
    fun createError(branch: String, base: String, destination: String): String = when {
        branch.isBlank() || base.isBlank() -> "Enter a new branch and starting revision."
        listOf(branch, base).any { it.trim() != it || it.startsWith('-') || it.length > 256 || it.any(Char::isISOControl) } -> "Use a valid branch and revision."
        !LaunchPresentation.absoluteFolder(destination) -> "Choose an absolute new folder path."
        else -> ""
    }
    fun destination(raw: JSONObject?, branch: String): String {
        val root = raw?.string("selected_root").orEmpty().ifBlank { raw?.string("path").orEmpty() }.trimEnd('/')
        if (!LaunchPresentation.absoluteFolder(root)) return ""
        val safe = branch.map { if (it.isLetterOrDigit() || it in "._-") it else '-' }.joinToString("")
        return "$root-$safe"
    }
}
