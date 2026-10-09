package app.zerus.mobile

import org.json.JSONObject
import java.util.Locale
import java.util.UUID

/** An opaque app-private object reference; no URI or filesystem path is retained. */
data class Attachment(val id: String, val name: String, val mime: String, val bytes: Long, val sha256: String) {
    fun toJson(): JSONObject = JSONObject().put("id", id).put("name", name).put("mime", mime)
        .put("bytes", bytes).put("sha256", sha256)

    companion object {
        fun fromJson(value: JSONObject): Attachment = Attachment(value.getString("id"), value.getString("name"),
            value.getString("mime"), value.getLong("bytes"), value.getString("sha256"))
            .also(AttachmentPolicy::validate)
    }
}

class AttachmentException(message: String) : Exception(message)

object AttachmentPolicy {
    const val MAX_FILES = 8
    const val MAX_FILE_BYTES = 10L * 1024 * 1024
    const val MAX_TOTAL_BYTES = 20L * 1024 * 1024
    private val mimePattern = Regex("^[a-z0-9][a-z0-9!#$&^_.+\\-]{0,126}/[a-z0-9][a-z0-9!#$&^_.+\\-]{0,126}$")

    fun validId(id: String): Boolean = runCatching { UUID.fromString(id).toString() == id }.getOrDefault(false)

    fun filename(value: String?): String {
        val leaf = value.orEmpty().substringAfterLast('/').substringAfterLast('\\')
        val result = StringBuilder()
        var offset = 0
        var length = 0
        while (offset < leaf.length) {
            val point = leaf.codePointAt(offset)
            offset += Character.charCount(point)
            val type = Character.getType(point)
            val unsafe = type == Character.CONTROL.toInt() || type == Character.FORMAT.toInt()
                || type == Character.SURROGATE.toInt() || point in intArrayOf(47, 92, 58, 42, 63, 34, 60, 62, 124)
            val text = if (unsafe) "_" else String(Character.toChars(point))
            val size = text.toByteArray(Charsets.UTF_8).size
            if (length + size > 255) break
            result.append(text)
            length += size
        }
        return result.toString().trim().trimEnd('.').takeUnless { it.isBlank() || it == "." || it == ".." } ?: "attachment"
    }

    fun mime(value: String?): String = value?.lowercase(Locale.ROOT)?.takeIf {
        it.toByteArray(Charsets.UTF_8).size <= 100 && it.matches(mimePattern)
    }
        ?: "application/octet-stream"

    fun validate(value: Attachment) {
        if (!validId(value.id) || value.name != filename(value.name)
            || value.name.toByteArray(Charsets.UTF_8).size > 255 || value.mime != mime(value.mime)
            || value.bytes !in 1..MAX_FILE_BYTES || !value.sha256.matches(Regex("^[a-f0-9]{64}$")))
            throw AttachmentException("Saved attachment metadata is invalid. Select the file again.")
    }

    fun selection(values: List<Attachment>, adding: Boolean = false): Long {
        if (values.size + (if (adding) 1 else 0) > MAX_FILES)
            throw AttachmentException("Attach at most 8 files.")
        if (values.map { it.id }.toSet().size != values.size)
            throw AttachmentException("The same attachment cannot appear twice.")
        values.forEach(::validate)
        val total = values.sumOf { it.bytes }
        if (total > MAX_TOTAL_BYTES) throw AttachmentException("Attachments exceed 20 MiB in total.")
        return total
    }
}
