package app.zerus.mobile

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

/** Independent encrypted page retention. Internal storage identities never leave this class. */
class ConversationPageCache(context:Context) {
    private val root=File(context.applicationContext.noBackupFilesDir,"conversation-pages-v1")
    private val files=ConversationCacheEngines.get(root) { EncryptedConversationFiles(root,::key,maxEntries=128,maxBytes=64L*1024*1024) }
    private fun key():SecretKey=synchronized(KEY_LOCK) {
        val store=KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(KEY_ALIAS,null) as? SecretKey) ?: KeyGenerator.getInstance("AES","AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(KEY_ALIAS,KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT).setKeySize(256)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        }.generateKey()
    }
    suspend fun read(target:Target,mode:String,cursor:String=""):JSONObject?=withContext(Dispatchers.IO) {
        val internal=ConversationPageKeys.storageTarget(target,mode,cursor)
        files.read(internal)?.let { value -> runCatching {
            val wrapper=JSONObject(value);require(MessageCodec.target(wrapper.getJSONObject("original_target"))==target)
            wrapper.getJSONObject("page")
        }.getOrNull() }
    }
    suspend fun write(target:Target,mode:String,cursor:String,raw:JSONObject) {
        val internal=ConversationPageKeys.storageTarget(target,mode,cursor);val generation=files.generation(target.connectionId)
        withContext(Dispatchers.IO) {
            val wrapper=ConversationPageKeys.wrap(target,internal,raw)
            files.write(internal,wrapper,System.currentTimeMillis(),generation)
        }
    }
    suspend fun purgeWorkspace(connection:String) {
        files.invalidate(connection)
        withContext(NonCancellable+Dispatchers.IO) { files.purgeWorkspace(connection) }
    }
    companion object { private const val KEY_ALIAS="zerus_conversation_pages_v1";private val KEY_LOCK=Any() }
}

internal object ConversationPageKeys {
    fun storageTarget(target:Target,mode:String,cursor:String):Target {
        require(mode in setOf("","before","after","around","unread") && cursor.length<=8192)
        val digest=MessageDigest.getInstance("SHA-256").digest(JSONArray(listOf(mode,cursor)).toString().toByteArray()).joinToString("") { "%02x".format(it) }
        return target.copy(session=target.session+"/zerus-private-page/"+digest)
    }
    fun wrap(target:Target,internal:Target,raw:JSONObject):JSONObject = MessageCodec.targetJson(internal)
        .put("name",internal.session).put("run_id",target.run).put("conversation_id",target.conversation)
        .put("original_target",MessageCodec.targetJson(target)).put("page",raw).also {
            if(target.archiveId.isNotBlank()) it.put("archive_id",target.archiveId)
            if(target.agentId.isNotBlank()) it.put("agent_id",target.agentId).put("parent_conversation_id",target.parentConversation)
        }
}
