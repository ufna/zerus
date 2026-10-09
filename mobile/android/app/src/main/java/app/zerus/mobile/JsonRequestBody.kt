package app.zerus.mobile

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.RequestBody
import okio.BufferedSink
import okio.utf8Size
import org.json.JSONArray
import org.json.JSONObject

/** Streams the bounded inline envelope without making a second full base64/JSON byte array. */
class JsonRequestBody(private val value: JSONObject, maxBytes: Long) : RequestBody() {
    private val length = size(value)
    init { require(length <= maxBytes) { "Message exceeds the gateway request limit." } }
    override fun contentType() = "application/json".toMediaType()
    override fun contentLength() = length
    override fun writeTo(sink: BufferedSink) { write(value, sink) }
    private fun string(value: String, key: String): String {
        if (key != "data_base64") return JSONObject.quote(value)
        require(value.all { it in 'A'..'Z' || it in 'a'..'z' || it in '0'..'9' || it in "+/=" }) { "Invalid file encoding." }
        return value
    }
    private fun size(value: Any?, key: String = ""): Long = when (value) {
        is JSONObject -> 2L + value.keys().asSequence().map { child -> JSONObject.quote(child).utf8Size() + 1 + size(value.get(child), child) }.toList().let { it.sum() + (it.size - 1).coerceAtLeast(0) }
        is JSONArray -> 2L + (0 until value.length()).sumOf { size(value.get(it)) } + (value.length() - 1).coerceAtLeast(0)
        is String -> if (key == "data_base64") string(value, key).length.toLong() + 2 else string(value, key).utf8Size()
        null, JSONObject.NULL -> 4L
        else -> value.toString().utf8Size()
    }
    private fun write(value: Any?, sink: BufferedSink, key: String = "") {
        when (value) {
            is JSONObject -> {
                sink.writeByte('{'.code)
                value.keys().asSequence().forEachIndexed { index, child ->
                    if (index > 0) sink.writeByte(','.code)
                    sink.writeUtf8(JSONObject.quote(child)).writeByte(':'.code)
                    write(value.get(child), sink, child)
                }
                sink.writeByte('}'.code)
            }
            is JSONArray -> {
                sink.writeByte('['.code)
                for (index in 0 until value.length()) { if (index > 0) sink.writeByte(','.code); write(value.get(index), sink) }
                sink.writeByte(']'.code)
            }
            is String -> if (key == "data_base64") { sink.writeByte('"'.code); sink.writeUtf8(value); sink.writeByte('"'.code) }
                else sink.writeUtf8(JSONObject.quote(value))
            null, JSONObject.NULL -> sink.writeUtf8("null")
            else -> sink.writeUtf8(value.toString())
        }
    }
}
