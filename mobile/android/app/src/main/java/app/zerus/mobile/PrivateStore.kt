package app.zerus.mobile

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** All credentials and drafts are encrypted with a non-exportable Android Keystore key. */
class PrivateStore(context: Context) {
    private val preferences = context.getSharedPreferences("zerus_private", Context.MODE_PRIVATE)
    private val key: SecretKey by lazy {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey("zerus_private_v1", null) as? SecretKey) ?: KeyGenerator.getInstance(
            KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder("zerus_private_v1", KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        }.generateKey()
    }
    @Synchronized private fun read(name: String): JSONArray {
        val stored = preferences.getString(name, null) ?: return JSONArray()
        val packed = Base64.decode(stored, Base64.NO_WRAP)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, packed.copyOfRange(0, 12)))
        return JSONArray(String(cipher.doFinal(packed.copyOfRange(12, packed.size)), Charsets.UTF_8))
    }
    @Synchronized private fun write(name: String, data: JSONArray) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key)
        val packed = cipher.iv + cipher.doFinal(data.toString().toByteArray(Charsets.UTF_8))
        check(preferences.edit().putString(name, Base64.encodeToString(packed, Base64.NO_WRAP)).commit()) {
            "Could not save private state. Message was not sent."
        }
    }
    fun connections() = read("computers").objects().map { Connection(it.getString("id"), it.getString("label"), it.getString("url"), it.getString("token")) }
    fun saveConnections(computers: List<Connection>) = write("computers", JSONArray(computers.map {
        JSONObject().put("id", it.id).put("label", it.label).put("url", it.url).put("token", it.token)
    }))
    fun messageState(): MessageState = if (preferences.contains("message_state"))
        MessageCodec.state(read("message_state").getJSONObject(0))
    else MessageState(read("drafts").objects().map(MessageCodec::draft))
    // Legacy encrypted drafts are retained; only a successful combined commit becomes authoritative.
    fun saveMessageState(state: MessageState) = write("message_state", JSONArray().put(MessageCodec.stateJson(state)))
    fun readState()=ConversationReadPolicies.decode(read("conversation_read_v1"))
    fun saveReadState(state:ConversationReadState)=write("conversation_read_v1",ConversationReadPolicies.encode(state))
    fun notificationEnabled() = preferences.getBoolean("notifications", true)
    fun setNotificationEnabled(enabled: Boolean) { preferences.edit().putBoolean("notifications", enabled).apply() }
    fun savePushEndpoint(connection: String, endpoint: String) {
        val records = read("push").objects().filterNot { it.string("connection") == connection } + JSONObject().put("connection", connection).put("endpoint", endpoint)
        write("push", JSONArray(records))
    }
    fun pushEndpoint(connection: String) = read("push").objects().find { it.string("connection") == connection }?.string("endpoint").orEmpty()
    fun saveFirebaseToken(token: String) = write("firebase", JSONArray().put(JSONObject().put("token", token)))
    fun firebaseToken() = read("firebase").optJSONObject(0)?.string("token").orEmpty()
    fun savePushStatus(connection: String, status: String) = write("push_status:$connection", JSONArray().put(JSONObject().put("status", status)))
    fun pushStatus(connection: String) = read("push_status:$connection").optJSONObject(0)?.string("status").orEmpty()
}
