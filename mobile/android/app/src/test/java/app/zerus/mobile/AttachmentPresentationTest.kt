package app.zerus.mobile

import okio.Buffer
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.util.Base64

class AttachmentPresentationTest {
    private fun metadata(id: String, text: String, submitted: String = text) = JSONObject().put("type", "UserPromptSubmit")
        .put("source", "hgs_delivery").put("message_id", id).put("at", 100).put("detail", text).put("submitted_text", submitted)
        .put("attachments", JSONArray().put(JSONObject().put("name", "note.txt").put("mime", "text/plain").put("bytes", 123)))
    @Test fun receiptMetadataReplacesNativeInstructionsWithUserTextAndFiles() {
        val instructions = "Review\nRead the attached file at /private/synthetic/file"
        val raw = JSONObject().put("events", JSONArray().put(JSONObject().put("type", "UserPromptSubmit").put("seq", 1).put("at", 101).put("detail", instructions)))
            .put("attachment_messages", JSONArray().put(metadata("request", "Review", instructions)))
        val rows = NativeParser.events(raw)
        assertEquals(1, rows.size)
        assertEquals("Review", rows.single().text)
        assertEquals("request", rows.single().requestId)
        assertEquals("hgs_delivery", rows.single().source)
        assertEquals("note.txt", rows.single().attachments.single().name)
    }
    @Test fun attachmentOnlyMessageRendersWithoutNativePathOrPlaceholderText() {
        val rows = NativeParser.events(JSONObject().put("attachment_messages", JSONArray().put(metadata("request", "", "Read /private/synthetic/file"))))
        assertEquals(1, rows.size)
        assertEquals("", rows.single().text)
        assertEquals(1, rows.single().attachments.size)
    }
    @Test fun sameProviderAndJournalIdCannotCollideAcrossRoles() {
        val raw = JSONObject("""{"events":[{"type":"UserPromptSubmit","seq":1,"at":1,"detail":"User"}],"provider_messages":[{"type":"AgentMessage","message_id":"1","at":2,"detail":"Assistant"}]}""")
        assertEquals(2, NativeParser.events(raw).map { it.id }.distinct().size)
    }
    @Test fun largeInlineFileStreamsWithExactLengthAndNoAlteredBytes() {
        val bytes = ByteArray(2_285_568) { (it % 251).toByte() }
        val body = JSONObject().put("payload", JSONObject().put("text", "Quotes \" and unicode λ\n").put("attachments", JSONArray().put(
            JSONObject().put("name", "file.txt").put("mime", "text/plain").put("data_base64", Base64.getEncoder().encodeToString(bytes)))))
        val request = JsonRequestBody(body, 30_408_704)
        val buffer = Buffer(); request.writeTo(buffer)
        assertEquals(buffer.size, request.contentLength())
        val parsed = JSONObject(buffer.readUtf8())
        assertEquals("Quotes \" and unicode λ\n", parsed.getJSONObject("payload").getString("text"))
        assertArrayEquals(bytes, Base64.getDecoder().decode(parsed.getJSONObject("payload").getJSONArray("attachments").getJSONObject(0).getString("data_base64")))
        assertThrows(IllegalArgumentException::class.java) { JsonRequestBody(body, 1_048_576) }
    }
    @Test fun requestLimitIsCheckedBeforeTransport() {
        assertThrows(IllegalArgumentException::class.java) { JsonRequestBody(JSONObject().put("data_base64", "A".repeat(30_408_704)), 30_408_704) }
        assertThrows(IllegalArgumentException::class.java) { JsonRequestBody(JSONObject().put("data_base64", "not a file"), 30_408_704) }
    }
}
