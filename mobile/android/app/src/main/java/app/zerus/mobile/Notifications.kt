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
    internal const val ALERTS = "zerus_session_alerts"
    const val LIVE = "zerus_live_connection"
    private const val CARD_ID = 40
    private const val SUMMARY_ID = 41
    private const val SUMMARY_TAG = "zerus:overflow:v2"
    private const val PREFIX = "zerus:session:v2:"
    private val locks = ConcurrentHashMap<String, Mutex>()
    private val delivery = Mutex()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val api = RelayApi()
    private var lastPost = -1L
    private var migrated = false

    private fun manager(context: Context) = context.getSystemService(NotificationManager::class.java)
    private fun cards(context: Context) = manager(context).activeNotifications.filter { it.tag?.startsWith(PREFIX) == true && it.id == CARD_ID }
    fun channels(context: Context) = synchronized(NotificationSettings.deliveryLock) {
        val manager = manager(context)
        manager.createNotificationChannels(listOf(
            NotificationChannel(ALERTS, "Session alerts", NotificationManager.IMPORTANCE_DEFAULT),
            NotificationChannel(LIVE, "Live connection", NotificationManager.IMPORTANCE_LOW)))
        if (!migrated) {
            manager.activeNotifications.filter { it.notification.channelId == ALERTS &&
                it.tag?.startsWith(PREFIX) != true && it.tag != SUMMARY_TAG }.forEach { manager.cancel(it.tag, it.id) }
            migrated = true
        }
    }
    fun initialize(context: Context) {
        val app = context.applicationContext
        scope.launch {
            runCatching {
                channels(app)
                val store = PrivateStore(app)
                val connections = store.connections()
                store.pruneNotificationStates(connections.map { it.id }.toSet())
                synchronized(NotificationSettings.deliveryLock) {
                    cards(app).filter { it.notification.extras.getString("zerus_connection") !in connections.map { row -> row.id } }
                        .forEach { manager(app).cancel(it.tag, it.id) }
                }
                connections.forEach { connection ->
                    locks.getOrPut(connection.id) { Mutex() }.withLock {
                        val state = store.notificationState(connection.id)
                        store.saveNotificationState(connection.id, state.copy(deliveryNeeded = true))
                    }
                    enqueue(app, connection.id, true)
                }
            }
        }
    }
    // Providers already enqueue authenticated work. A wake hint has no user-visible content.
    fun wake(context: Context, connectionId: String) = Unit

    fun cancelDisabled(context: Context, preferences: NotificationPreferences) = synchronized(NotificationSettings.deliveryLock) {
        cards(context).filter { row ->
            val kind = runCatching { SessionAlertKind.valueOf(row.notification.extras.getString("zerus_kind").orEmpty()) }.getOrNull()
            kind == null || !preferences.allows(kind)
        }.forEach { manager(context).cancel(it.tag, it.id) }
        // A type change invalidates the aggregate; queued catalogs rebuild it without stale counts.
        manager(context).cancel(SUMMARY_TAG, SUMMARY_ID)
    }

    private fun connected(store: PrivateStore, connection: Connection) = store.connections().any {
        it.id == connection.id && it.url == connection.url && it.token == connection.token
    }

    /** true means durable work remains; the worker retries instead of replacing itself. */
    suspend fun sync(context: Context, connection: Connection, wait: Int = 0, forceCatalog: Boolean = false): Boolean =
        locks.getOrPut(connection.id) { Mutex() }.withLock {
            channels(context)
            val store = PrivateStore(context)
            var state = store.notificationState(connection.id)
            var force = forceCatalog
            var operations = 0
            while (operations < 10) {
                currentCoroutineContext().ensureActive()
                if (!connected(store, connection)) return@withLock false
                val preferences = store.notificationPreferences()
                val response = api.events(connection, state.cursor,
                    if (operations == 0 && !state.bootstrap && !state.draining && state.pending.isEmpty() && !force &&
                        state.settingsGeneration == preferences.generation && state.pendingPosts.isEmpty() && !state.deliveryNeeded) wait else 0)
                operations++
                currentCoroutineContext().ensureActive()
                val page = NotificationPolicy.ingest(state, NotificationCatalogParser.events(response))
                state = page.state
                store.saveNotificationState(connection.id, state)
                val needsCatalog = page.full || page.empty || operations >= 9
                if (needsCatalog && (state.bootstrap || state.pending.isNotEmpty() || force ||
                    state.settingsGeneration != preferences.generation || state.pendingPosts.isNotEmpty() || state.deliveryNeeded)) {
                    val raw = api.computers(connection).getJSONArray("computers").objects()
                    operations++
                    currentCoroutineContext().ensureActive()
                    if (!connected(store, connection)) return@withLock false
                    val catalog = NotificationCatalogParser.parse(connection, raw, store.messageState().machineAliases)
                    val active = synchronized(NotificationSettings.deliveryLock) {
                        cards(context).mapNotNull { it.notification.extras.getString("zerus_slot") }.toSet()
                    }
                    val plan = NotificationPolicy.plan(NotificationPolicy.clockGuard(state, android.os.SystemClock.elapsedRealtime()),
                        catalog.sessions, state.pending.mapNotNull { catalog.resolve(connection.id, it) },
                        store.notificationPreferences(), System.currentTimeMillis() / 1000.0, active)
                    state = plan.state
                    store.saveNotificationState(connection.id, state)
                    state = deliver(context, connection, store, state, plan)
                    force = false
                }
                if (page.empty) return@withLock false
            }
            true
        }

    private suspend fun pace() {
        val now = android.os.SystemClock.elapsedRealtime()
        if (lastPost >= 0 && now >= lastPost) delay((350 - (now - lastPost)).coerceAtLeast(0))
        currentCoroutineContext().ensureActive()
    }
    private suspend fun deliver(context: Context, connection: Connection, store: PrivateStore,
        initial: NotificationState, plan: NotificationPlan): NotificationState = delivery.withLock {
        var state = initial
        synchronized(NotificationSettings.deliveryLock) {
            cards(context).filter { it.notification.extras.getString("zerus_connection") == connection.id &&
                it.notification.extras.getString("zerus_slot") !in plan.retained }.forEach { manager(context).cancel(it.tag, it.id) }
        }
        if (!androidx.core.app.NotificationManagerCompat.from(context).areNotificationsEnabled() ||
            manager(context).getNotificationChannel(ALERTS)?.importance == NotificationManager.IMPORTANCE_NONE) {
            state = state.copy(deliveryNeeded = false, pendingPosts = emptySet(), overflowKinds = emptySet(),
                records = state.records.map { record -> if (record.slot.key in state.pendingPosts)
                    record.copy(postedFingerprint = "", postedKind = null) else record })
            store.saveNotificationState(connection.id, state)
            summary(context)
            return@withLock state
        }
        val alreadyActive = synchronized(NotificationSettings.deliveryLock) { cards(context).mapNotNull { it.notification.extras.getString("zerus_slot") }.toSet() }
        val overflowKinds = plan.overflowKinds.toMutableSet()
        val ordered = plan.desired.sortedWith(compareByDescending<SessionNotice> { it.current.slot.key in alreadyActive }
            .thenBy { it.kind == SessionAlertKind.Finished }.thenBy { it.current.slot.key })
        for (notice in ordered) {
            currentCoroutineContext().ensureActive()
            val record = state.records.find { it.slot == notice.current.slot }
            val existing = synchronized(NotificationSettings.deliveryLock) { cards(context).find { it.tag == notice.current.slot.tag } }
            val changed = existing?.notification?.let { it.extras.getString("zerus_fingerprint") != notice.fingerprint ||
                it.extras.getString(android.app.Notification.EXTRA_TITLE) != notice.current.title ||
                it.extras.getString(android.app.Notification.EXTRA_TEXT) != notice.body } ?: false
            val recovering = notice.current.slot.key in state.pendingPosts
            val restore = notice.kind in plan.restoreKinds && (record?.restoredAt ?: -1) < store.notificationPreferences().enabledAt(notice.kind)
            if (existing == null && record?.postedFingerprint == notice.fingerprint && !recovering && !restore) continue
            if (existing != null && !changed && !recovering) continue
            pace()
            val preferences = store.notificationPreferences()
            if (!preferences.allows(notice.kind) || !connected(store, connection)) continue
            val admitted = synchronized(NotificationSettings.deliveryLock) {
                val active = cards(context)
                if (active.any { it.tag == notice.current.slot.tag } || active.size < 24) true
                else if (notice.kind != SessionAlertKind.Finished) {
                    active.find { it.notification.extras.getString("zerus_kind") == SessionAlertKind.Finished.name }?.let {
                        manager(context).cancel(it.tag, it.id); true
                    } ?: false
                } else false
            }
            if (!admitted) { if (notice.kind != SessionAlertKind.Finished) overflowKinds += notice.kind; continue }
            val elapsed = android.os.SystemClock.elapsedRealtime()
            val audible = notice.fresh && record?.postedFingerprint != notice.fingerprint && !recovering &&
                NotificationPolicy.canSound(state, elapsed)
            state = NotificationPolicy.reserve(state, notice, audible, elapsed, preferences.enabledAt(notice.kind))
            // Save evidence before Android delivery. A crash resumes this outbox silently.
            store.saveNotificationState(connection.id, state)
            currentCoroutineContext().ensureActive()
            val posted = synchronized(NotificationSettings.deliveryLock) {
                val latest = store.notificationPreferences()
                if (!latest.allows(notice.kind) || !connected(store, connection)) false else {
                    manager(context).notify(notice.current.slot.tag, CARD_ID, notification(context, notice, !audible))
                    lastPost = android.os.SystemClock.elapsedRealtime(); true
                }
            }
            if (posted) {
                state = state.copy(pendingPosts = state.pendingPosts - notice.current.slot.key)
                store.saveNotificationState(connection.id, state)
            }
        }
        val latest = store.notificationPreferences()
        state = state.copy(overflowKinds = if (latest.generation == plan.state.settingsGeneration)
            overflowKinds.filter(latest::allows).toSet() else emptySet())
        store.saveNotificationState(connection.id, state)
        summary(context)
        state = state.copy(deliveryNeeded = false, pendingRestoreKinds = emptySet(), records = state.records.map {
            it.copy(candidateFingerprint = "", candidateTarget = "", candidateEventId = 0, candidateEventAt = 0.0)
        })
        store.saveNotificationState(connection.id, state)
        state
    }

    private fun publicVersion(context: Context) = NotificationCompat.Builder(context, ALERTS).setSmallIcon(R.drawable.ic_zerus)
        .setContentTitle("Zerus").setContentText("New session activity.").setSilent(true).build()
    private fun intent(context: Context, slot: NotificationSlot?): PendingIntent {
        val uri = android.net.Uri.Builder().scheme("zerus-alert").authority(if (slot == null) "overview" else "session")
        slot?.let { uri.appendPath(it.connection).appendPath(it.computer).appendPath(it.session) }
        val intent = Intent(context, MainActivity::class.java).setData(uri.build())
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        slot?.let { intent.putExtra("connection", it.connection).putExtra("computer", it.computer).putExtra("session", it.session) }
        return PendingIntent.getActivity(context, 0, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }
    private fun notification(context: Context, notice: SessionNotice, silent: Boolean) =
        NotificationCompat.Builder(context, ALERTS).setSmallIcon(R.drawable.ic_zerus)
            .setContentTitle(notice.current.title).setContentText(notice.body).setStyle(NotificationCompat.BigTextStyle().bigText(notice.body))
            .setContentIntent(intent(context, notice.current.slot)).setAutoCancel(true)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE).setPublicVersion(publicVersion(context))
            .setGroup("zerus_workspace:${notice.current.slot.connection}").setSilent(silent)
            .addExtras(android.os.Bundle().apply {
                putString("zerus_connection", notice.current.slot.connection); putString("zerus_slot", notice.current.slot.key)
                putString("zerus_kind", notice.kind.name); putString("zerus_fingerprint", notice.fingerprint)
            }).build()
    private suspend fun summary(context: Context) {
        val store = PrivateStore(context)
        val preferences = store.notificationPreferences()
        // Read durable membership even in a background-only process after restart.
        val needed = preferences.master && store.connections().any { connection ->
            store.notificationState(connection.id).overflowKinds.any(preferences::allows)
        }
        if (!needed) { synchronized(NotificationSettings.deliveryLock) { manager(context).cancel(SUMMARY_TAG, SUMMARY_ID) }; return }
        val existing = synchronized(NotificationSettings.deliveryLock) { manager(context).activeNotifications.any { it.tag == SUMMARY_TAG } }
        if (existing) return
        pace()
        synchronized(NotificationSettings.deliveryLock) {
            if (needed && PrivateStore(context).notificationPreferences() == preferences) {
                val notification = NotificationCompat.Builder(context, ALERTS).setSmallIcon(R.drawable.ic_zerus)
                    .setContentTitle("More sessions need attention").setContentText("Open Zerus to review all workspaces")
                    .setContentIntent(intent(context, null)).setAutoCancel(true).setSilent(true)
                    .setVisibility(NotificationCompat.VISIBILITY_PRIVATE).setPublicVersion(publicVersion(context)).build()
                manager(context).notify(SUMMARY_TAG, SUMMARY_ID, notification)
                lastPost = android.os.SystemClock.elapsedRealtime()
            }
        }
    }
    fun enqueue(context: Context, connectionId: String, forceCatalog: Boolean = false) {
        val work = OneTimeWorkRequestBuilder<NotificationSyncWorker>().setInputData(Data.Builder()
            .putString("connection", connectionId).putBoolean("catalog", forceCatalog).build())
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
        return try { if (SessionNotifications.sync(applicationContext, connection, forceCatalog = inputData.getBoolean("catalog", false))) Result.retry() else Result.success() }
        catch (e: RelayException) { if (e.status in listOf(401, 403)) Result.failure() else Result.retry() }
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
                    try { if (SessionNotifications.sync(this@LiveConnectionService, connection, 25)) delay(1_000) }
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
            PrivateStore(context).connections().forEach { connection -> PrivateStore(context).savePushProvider(connection.id, "unifiedpush"); UnifiedPush.register(context, instance = connection.id, messageForDistributor = "Zerus session alerts") }
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
        if (PrivateStore(this).pushProvider(instance) != "unifiedpush") return
        PrivateStore(this).savePushStatus(instance, "Push distributor disconnected.")
        WorkManager.getInstance(this).enqueue(OneTimeWorkRequestBuilder<PushEndpointWorker>().setInputData(
            Data.Builder().putString("connection", instance).putBoolean("delete", true).build()).build())
    }
}

class PushEndpointWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        val connection = PrivateStore(applicationContext).connections().find { it.id == inputData.getString("connection") } ?: return Result.success()
        return PushSetupLocks.withConnection(connection.id) { try {
            val delete = inputData.getBoolean("delete", false)
            if (PrivateStore(applicationContext).pushProvider(connection.id) != "unifiedpush") return@withConnection Result.success()
            if (!delete) {
                val providers = RelayApi().call(connection.url, connection.token, "/v1/capabilities").optJSONArray("push_providers")
                if (providers == null || (0 until providers.length()).none { providers.optString(it) == "unifiedpush" }) {
                    PrivateStore(applicationContext).savePushStatusIfCurrent(connection, "unifiedpush", "Gateway UnifiedPush is not configured. Use the live connection.")
                    return@withConnection Result.failure()
                }
            }
            if (!PrivateStore(applicationContext).connections().contains(connection) || PrivateStore(applicationContext).pushProvider(connection.id) != "unifiedpush") return@withConnection Result.success()
            val body = if (delete) null else JSONObject().put("provider", "unifiedpush").put("endpoint", EndpointPolicy.push(PrivateStore(applicationContext).pushEndpoint(connection.id)))
            RelayApi().call(connection.url, connection.token, "/v1/push", body, delete)
            PrivateStore(applicationContext).savePushStatusIfCurrent(connection, "unifiedpush", if (delete) "Push disconnected." else "UnifiedPush configured")
            Result.success()
        } catch (e: RelayException) {
            PrivateStore(applicationContext).savePushStatusIfCurrent(connection, "unifiedpush", "Gateway rejected push setup (${e.status}). Check gateway configuration.")
            if (e.status in 400..499) Result.failure() else Result.retry()
        }
        catch (_: IllegalArgumentException) { PrivateStore(applicationContext).savePushStatusIfCurrent(connection, "unifiedpush", "Invalid push endpoint. Check the distributor."); Result.failure() }
        catch (e: kotlinx.coroutines.CancellationException) { throw e }
        catch (_: Exception) { PrivateStore(applicationContext).savePushStatusIfCurrent(connection, "unifiedpush", "Push setup waiting for gateway connection."); Result.retry() }
        }
    }
}
