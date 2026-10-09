package app.zerus.mobile

import android.app.AlertDialog
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.widget.Toast
import androidx.core.app.NotificationCompat
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.work.CoroutineWorker
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.ForegroundInfo
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap
import org.json.JSONObject
import org.unifiedpush.android.connector.FailedReason
import org.unifiedpush.android.connector.PushService
import org.unifiedpush.android.connector.UnifiedPush
import org.unifiedpush.android.connector.data.PushEndpoint
import org.unifiedpush.android.connector.data.PushMessage

object SessionNotifications {
    private const val ALERTS = "zerus_session_alerts"
    const val LIVE = "zerus_live_connection"
    private val locks = ConcurrentHashMap<String, Mutex>()
    fun channels(context: Context) {
        context.getSystemService(NotificationManager::class.java).createNotificationChannels(listOf(
            NotificationChannel(ALERTS, "Session alerts", NotificationManager.IMPORTANCE_DEFAULT),
            NotificationChannel(LIVE, "Live connection", NotificationManager.IMPORTANCE_LOW)))
    }
    fun alert(context: Context, connection: Connection, event: JSONObject) {
        if (!PrivateStore(context).notificationEnabled()) return
        channels(context)
        val intent = Intent(context, MainActivity::class.java).putExtra("connection", connection.id)
            .putExtra("computer", event.string("computer_id")).putExtra("session", event.string("session"))
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        val id = (connection.id + ":" + event.string("id")).hashCode()
        val pending = PendingIntent.getActivity(context, id, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val notification = NotificationCompat.Builder(context, ALERTS).setSmallIcon(R.drawable.ic_zerus)
            .setContentTitle("Zerus").setContentText(when (event.string("kind")) {
                "attention" -> "A session needs your input."
                "completed" -> "A session finished its turn."
                "error" -> "A session needs attention."
                else -> "Session activity changed."
            }).setContentIntent(pending).setAutoCancel(true).setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setSilent(context.getSystemService(NotificationManager::class.java).activeNotifications.any { it.id == ("wake:${connection.id}").hashCode() }).build()
        runCatching { context.getSystemService(NotificationManager::class.java).notify(id, notification) }
    }
    fun wake(context: Context, connectionId: String) {
        val store = PrivateStore(context)
        if (!store.notificationEnabled() || store.connections().none { it.id == connectionId }) return
        channels(context)
        val pending = PendingIntent.getActivity(context, connectionId.hashCode(), Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val alert = NotificationCompat.Builder(context, ALERTS).setSmallIcon(R.drawable.ic_zerus)
            .setContentTitle("Zerus").setContentText("New session activity. Open Zerus to check your agents.")
            .setContentIntent(pending).setAutoCancel(true).setOnlyAlertOnce(true).setVisibility(NotificationCompat.VISIBILITY_PRIVATE).build()
        runCatching { context.getSystemService(NotificationManager::class.java).notify(("wake:$connectionId").hashCode(), alert) }
    }
    suspend fun sync(context: Context, connection: Connection, wait: Int = 0) {
        locks.getOrPut(connection.id) { Mutex() }.withLock {
        val preferences = context.getSharedPreferences("zerus_event_cursors", Context.MODE_PRIVATE)
        val cursor = preferences.getLong(connection.id, 0)
        val response = RelayApi().events(connection, cursor, wait)
        currentCoroutineContext().ensureActive()
        response.optJSONArray("events")?.objects().orEmpty().forEach { alert(context, connection, it) }
        preferences.edit().putLong(connection.id, maxOf(cursor, response.optLong("cursor", cursor))).commit()
        if ((response.optJSONArray("events")?.length() ?: 0) > 0) context.getSystemService(NotificationManager::class.java).cancel(("wake:${connection.id}").hashCode())
        }
    }
    fun enqueue(context: Context, connectionId: String) {
        val work = OneTimeWorkRequestBuilder<NotificationSyncWorker>().setInputData(Data.Builder().putString("connection", connectionId).build())
            .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST).build()
        WorkManager.getInstance(context).enqueueUniqueWork("zerus_push_$connectionId", ExistingWorkPolicy.REPLACE, work)
    }
}

class NotificationSyncWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun getForegroundInfo(): ForegroundInfo {
        SessionNotifications.channels(applicationContext)
        val notification = NotificationCompat.Builder(applicationContext, SessionNotifications.LIVE).setSmallIcon(R.drawable.ic_zerus)
            .setContentTitle("Zerus").setContentText("Fetching session alerts").setOngoing(true).build()
        return ForegroundInfo(3, notification)
    }
    override suspend fun doWork(): Result {
        val connection = PrivateStore(applicationContext).connections().find { it.id == inputData.getString("connection") } ?: return Result.success()
        return try { SessionNotifications.sync(applicationContext, connection); Result.success() }
        catch (e: RelayException) { if (e.status in 400..499) Result.failure() else Result.retry() }
        catch (e: kotlinx.coroutines.CancellationException) { throw e }
        catch (_: Exception) { Result.retry() }
    }
}

/** Started explicitly while the app is visible, for cross-device text conversation continuity. */
class LiveConnectionService : Service() {
    companion object { var running by mutableStateOf(false); private set }
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val listeners = mutableMapOf<String, kotlinx.coroutines.Job>()
    override fun onBind(intent: Intent?): IBinder? = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        SessionNotifications.channels(this)
        val stop = PendingIntent.getService(this, 2, Intent(this, LiveConnectionService::class.java).setAction("stop"), PendingIntent.FLAG_IMMUTABLE)
        if (intent?.action == "stop") { stopSelf(); return START_NOT_STICKY }
        val open = PendingIntent.getActivity(this, 1, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val notification = NotificationCompat.Builder(this, SessionNotifications.LIVE).setSmallIcon(R.drawable.ic_zerus)
            .setContentTitle("Zerus live connection").setContentText("Listening for agent session updates")
            .setOngoing(true).setContentIntent(open).addAction(0, "Stop", stop).build()
        if (Build.VERSION.SDK_INT >= 34) startForeground(1, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING)
        else startForeground(1, notification)
        running = true
        val current = PrivateStore(this).connections()
        listeners.filterKeys { id -> current.none { it.id == id } }.values.forEach { it.cancel() }
        current.forEach { connection ->
            if (listeners[connection.id]?.isActive != true) listeners[connection.id] = scope.launch {
                while (isActive) {
                    if (PrivateStore(this@LiveConnectionService).connections().none { it.id == connection.id }) break
                    try { SessionNotifications.sync(this@LiveConnectionService, connection, 25) }
                    catch (e: RelayException) { if (e.status in listOf(401, 403)) break else delay(15_000) }
                    catch (e: kotlinx.coroutines.CancellationException) { throw e }
                    catch (_: Exception) { delay(15_000) }
                }
            }
        }
        return START_NOT_STICKY
    }
    override fun onDestroy() { running = false; scope.cancel(); super.onDestroy() }
}

object PushRegistration {
    fun choose(context: Context) {
        val distributors = UnifiedPush.getDistributors(context)
        if (distributors.isEmpty()) {
            AlertDialog.Builder(context).setTitle("UnifiedPush distributor needed")
                .setMessage("Install and configure a UnifiedPush distributor such as ntfy or NextPush, then return here. You can also enable the live connection in Computers.")
                .setPositiveButton("OK", null).show(); return
        }
        val labels = distributors.map { name -> runCatching {
            context.packageManager.getApplicationLabel(context.packageManager.getApplicationInfo(name, 0)).toString()
        }.getOrDefault(name) }.toTypedArray()
        AlertDialog.Builder(context).setTitle("Choose push distributor").setItems(labels) { _, index ->
            UnifiedPush.saveDistributor(context, distributors[index])
            PrivateStore(context).connections().forEach { connection -> UnifiedPush.register(context, instance = connection.id, messageForDistributor = "Zerus session alerts") }
        }.setNegativeButton("Cancel", null).show()
    }
}

class ZerusPushService : PushService() {
    override fun onNewEndpoint(endpoint: PushEndpoint, instance: String) {
        val connection = PrivateStore(this).connections().find { it.id == instance } ?: return
        try { PrivateStore(this).savePushEndpoint(instance, EndpointPolicy.push(endpoint.url)) }
        catch (_: Exception) { PrivateStore(this).savePushStatus(instance, "Could not save a secure push endpoint."); return }
        PrivateStore(this).savePushStatus(instance, "Registering UnifiedPush with gateway…")
        WorkManager.getInstance(this).enqueue(OneTimeWorkRequestBuilder<PushEndpointWorker>().setInputData(
            Data.Builder().putString("connection", instance).build()).build())
        SessionNotifications.enqueue(this, connection.id)
    }
    override fun onMessage(message: PushMessage, instance: String) {
        // Push is a wake-up hint. Authoritative events are fetched with our authenticated gateway token.
        SessionNotifications.wake(this, instance); SessionNotifications.enqueue(this, instance)
    }
    override fun onRegistrationFailed(reason: FailedReason, instance: String) {
        PrivateStore(this).savePushStatus(instance, "Distributor registration failed. Check its network and settings.")
        Toast.makeText(this, "Push registration failed. Check your distributor or use the live connection.", Toast.LENGTH_LONG).show()
    }
    override fun onUnregistered(instance: String) {
        PrivateStore(this).savePushStatus(instance, "Push distributor disconnected.")
        WorkManager.getInstance(this).enqueue(OneTimeWorkRequestBuilder<PushEndpointWorker>().setInputData(
            Data.Builder().putString("connection", instance).putBoolean("delete", true).build()).build())
    }
}

class PushEndpointWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        val connection = PrivateStore(applicationContext).connections().find { it.id == inputData.getString("connection") } ?: return Result.success()
        return try {
            val delete = inputData.getBoolean("delete", false)
            if (!delete) {
                val providers = RelayApi().call(connection.url, connection.token, "/v1/capabilities").optJSONArray("push_providers")
                if (providers == null || (0 until providers.length()).none { providers.optString(it) == "unifiedpush" }) {
                    PrivateStore(applicationContext).savePushStatus(connection.id, "Gateway UnifiedPush is not configured. Use the live connection.")
                    return Result.failure()
                }
            }
            val body = if (delete) null else JSONObject().put("provider", "unifiedpush").put("endpoint", EndpointPolicy.push(PrivateStore(applicationContext).pushEndpoint(connection.id)))
            RelayApi().call(connection.url, connection.token, "/v1/push", body, delete)
            PrivateStore(applicationContext).savePushStatus(connection.id, if (delete) "Push disconnected." else "UnifiedPush configured")
            Result.success()
        } catch (e: RelayException) {
            PrivateStore(applicationContext).savePushStatus(connection.id, "Gateway rejected push setup (${e.status}). Check gateway configuration.")
            if (e.status in 400..499) Result.failure() else Result.retry()
        }
        catch (_: IllegalArgumentException) { PrivateStore(applicationContext).savePushStatus(connection.id, "Invalid push endpoint. Check the distributor."); Result.failure() }
        catch (_: Exception) { PrivateStore(applicationContext).savePushStatus(connection.id, "Push setup waiting for gateway connection."); Result.retry() }
    }
}
