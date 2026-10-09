package app.zerus.mobile

import java.nio.file.Files
import javax.crypto.KeyGenerator
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class ConversationPageCacheTest {
    private val target=Target("computer","codex/project/session","run","conversation","workspace")
    @Test fun actualEncryptedPageRestartPreservesScopeAndKeepsTextPrivate() {
        val root=Files.createTempDirectory("zerus-page-test").toFile()
        try {
            val key=KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
            val internal=ConversationPageKeys.storageTarget(target,"before","opaque-cursor")
            val raw=JSONObject().put("synthetic_text","private page text")
            fun engine()=EncryptedConversationFiles(root,{ key },maxEntries=128,maxBytes=64L*1024*1024)
            engine().write(internal,ConversationPageKeys.wrap(target,internal,raw))
            assertFalse(String(root.listFiles()!!.single().readBytes(),Charsets.ISO_8859_1).contains("private page text"))
            val restored=JSONObject(engine().read(internal)!!)
            assertEquals(target,MessageCodec.target(restored.getJSONObject("original_target")))
            assertEquals("private page text",restored.getJSONObject("page").getString("synthetic_text"))
            assertNull(engine().read(ConversationPageKeys.storageTarget(target,"after","opaque-cursor")))
            assertNull(engine().read(ConversationPageKeys.storageTarget(target.copy(connectionId="other"),"before","opaque-cursor")))
            assertNull(engine().read(ConversationPageKeys.storageTarget(target.copy(archiveId="archive"),"before","opaque-cursor")))
        } finally { root.deleteRecursively() }
    }
    @Test fun pageEvictionDoesNotChangeSeparateDurableReadRecordOrCursor() {
        val root=Files.createTempDirectory("zerus-page-eviction-test").toFile()
        try {
            val key=KeyGenerator.getInstance("AES").generateKey()
            var clock=1_000_000L
            val engine=EncryptedConversationFiles(root,{ key },now={clock},maxEntries=2,maxBytes=64L*1024*1024)
            val read=ConversationReadState().head(target,"epoch",900,900,true).viewport(target,"history:20",12,"cursor20","epoch")
            repeat(4) { index -> clock++;val internal=ConversationPageKeys.storageTarget(target,"before","page$index");engine.write(internal,ConversationPageKeys.wrap(target,internal,JSONObject().put("index",index))) }
            assertNull(engine.read(ConversationPageKeys.storageTarget(target,"before","page0")))
            val restored=ConversationReadPolicies.decode(ConversationReadPolicies.encode(read))
            assertEquals("cursor20",restored.record(target)!!.viewport!!.cursor)
            assertEquals(0,restored.record(target)!!.readThrough)
        } finally { root.deleteRecursively() }
    }
}
