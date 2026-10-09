package app.zerus.mobile

import android.content.Context
import android.widget.Toast
import androidx.work.CoroutineWorker
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Included only when explicitly built with private Firebase project configuration. */
object OptionalFirebase {
    @JvmStatic fun register(context: Context) {
        FirebaseMessaging.getInstance().isAutoInitEnabled = true
        FirebaseMessaging.getInstance().token.addOnSuccessListener { token ->
            PrivateStore(context).saveFirebaseToken(token)
            CoroutineScope(Dispatchers.IO).launch {
                val result = runCatching { registerToken(context, token) }
                val failure = result.exceptionOrNull()
                val message = when (failure) {
                    null -> "Firebase push registered."
                    is RelayException -> "Gateway rejected Firebase setup (${failure.status}). Check gateway configuration."
                    is IllegalStateException -> "Gateway Firebase push is not configured. Use UnifiedPush or the live connection."
                    else -> "Firebase setup could not reach the gateway. Try again when connected."
                }
                withContext(Dispatchers.Main) { Toast.makeText(context, message, Toast.LENGTH_LONG).show() }
            }
        }.addOnFailureListener { Toast.makeText(context, "Firebase registration failed. Check Google services.", Toast.LENGTH_LONG).show() }
    }
    suspend fun registerToken(context: Context, token: String) {
        val api = RelayApi()
        PrivateStore(context).connections().forEach { connection ->
            val providers = api.call(connection.url, connection.token, "/v1/capabilities").optJSONArray("push_providers")
            check(providers != null && (0 until providers.length()).any { providers.optString(it) == "fcm" }) { "This gateway does not have Firebase push configured. Use UnifiedPush or the live connection." }
            api.call(connection.url, connection.token, "/v1/push", JSONObject().put("provider", "fcm").put("token", token))
            PrivateStore(context).savePushStatus(connection.id, "Firebase push configured")
        }
    }
}
class ZerusFirebaseService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        PrivateStore(this).saveFirebaseToken(token)
        WorkManager.getInstance(this).enqueue(OneTimeWorkRequestBuilder<FirebaseTokenWorker>().build())
    }
    override fun onMessageReceived(message: RemoteMessage) {
        PrivateStore(this).connections().forEach { SessionNotifications.wake(this, it.id); SessionNotifications.enqueue(this, it.id) }
    }
}
class FirebaseTokenWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result = try {
        OptionalFirebase.registerToken(applicationContext, PrivateStore(applicationContext).firebaseToken()); Result.success()
    } catch (e: RelayException) { if (e.status in 400..499) Result.failure() else Result.retry() }
    catch (_: IllegalStateException) { Result.failure() }
    catch (_: Exception) { Result.retry() }
}
