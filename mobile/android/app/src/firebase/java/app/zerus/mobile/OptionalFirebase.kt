package app.zerus.mobile

import android.content.Context
import android.os.Bundle
import android.widget.Toast
import androidx.work.*
import com.google.firebase.FirebaseApp
import com.google.firebase.analytics.FirebaseAnalytics
import com.google.firebase.crashlytics.FirebaseCrashlytics
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.util.concurrent.Executor

/** Compiled only with private Firebase project configuration. */
object OptionalFirebase {
    private val tokenLock = Any()
    private var tokenGeneration = 0L
    fun tokenChanged(context: Context, token: String) {
        val generation = synchronized(tokenLock) { ++tokenGeneration }
        CoroutineScope(Dispatchers.IO).launch {
            runCatching { synchronized(tokenLock) {
                if (generation == tokenGeneration) { PrivateStore(context).saveFirebaseToken(token); enqueue(context) }
            } }
        }
    }
    @JvmStatic fun create(context: Context, preferences: TelemetryPreferences): TelemetryBackend {
        // Manifest FALSE covers first install. SDK-persisted FALSE covers later OFF launches;
        // our controller writes OFF only after the documented SDK disable call.
        val analytics = FirebaseAnalytics.getInstance(context)
        analytics.setConsent(mapOf(FirebaseAnalytics.ConsentType.AD_STORAGE to FirebaseAnalytics.ConsentStatus.DENIED,
            FirebaseAnalytics.ConsentType.AD_USER_DATA to FirebaseAnalytics.ConsentStatus.DENIED,
            FirebaseAnalytics.ConsentType.AD_PERSONALIZATION to FirebaseAnalytics.ConsentStatus.DENIED))
        analytics.setAnalyticsCollectionEnabled(preferences.analytics)
        checkNotNull(FirebaseApp.initializeApp(context))
        val crash = FirebaseCrashlytics.getInstance()
        val previous = checkNotNull(Thread.getDefaultUncaughtExceptionHandler())
        // Let Crashlytics and Android terminate normally; serialize only a sanitized error.
        Thread.setDefaultUncaughtExceptionHandler(SanitizedCrashHandler(previous))
        return FirebaseBackend(context, analytics, crash, preferences.crashes)
    }
    @JvmStatic fun start(context: Context) {
        FirebaseMessaging.getInstance().isAutoInitEnabled = true
        val generation = synchronized(tokenLock) { tokenGeneration }
        FirebaseMessaging.getInstance().token.addOnSuccessListener { token ->
            CoroutineScope(Dispatchers.IO).launch {
                runCatching { synchronized(tokenLock) {
                    if (generation == tokenGeneration) { PrivateStore(context).saveFirebaseToken(token); enqueue(context) }
                } }
            }
        }
    }
    @JvmStatic fun register(context: Context) {
        CoroutineScope(Dispatchers.IO).launch {
            PrivateStore(context).connections().forEach { PrivateStore(context).savePushProvider(it.id, "fcm") }
            start(context)
        }
        Toast.makeText(context, "Firebase push setup will retry when connected.", Toast.LENGTH_LONG).show()
    }
    fun enqueue(context: Context) {
        val store = PrivateStore(context)
        val token = store.firebaseToken()
        if (token.isBlank()) return
        store.connections().filter { FcmRegistrationPolicy.needsRegistration(token, store.firebaseBinding(it.id), store.pushProvider(it.id)) }.forEach { connection ->
            val request = OneTimeWorkRequestBuilder<FirebaseTokenWorker>()
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .setInputData(Data.Builder().putString("connection", connection.id).build()).build()
            WorkManager.getInstance(context).enqueueUniqueWork("zerus-fcm:${connection.id}", ExistingWorkPolicy.REPLACE, request)
        }
    }
}
private class FirebaseBackend(context: Context, private val analytics: FirebaseAnalytics,
    private val crash: FirebaseCrashlytics, initialCrashes: Boolean) : TelemetryBackend {
    private val reportPreferences = context.getSharedPreferences("zerus_telemetry", Context.MODE_PRIVATE)
    private val reportLock = Any()
    @Volatile private var analyticsEnabled = false
    @Volatile private var crashesEnabled = initialCrashes
    @Volatile private var captureEnabled = false
    init {
        // Never automatically upload. Documented manual send/delete chooses one
        // startup action without racing SDK automatic-collection permission.
        crash.setCrashlyticsCollectionEnabled(false)
        val callbackExecutor = Executor { command -> CoroutineScope(Dispatchers.IO).launch { command.run() } }
        crash.checkForUnsentReports().addOnSuccessListener(callbackExecutor) { hasReports -> synchronized(reportLock) {
            val currentEnabled = crashesEnabled && reportPreferences.getBoolean("crashes", true)
            val plan = CrashReportingPolicy.startup(currentEnabled,
                reportPreferences.getBoolean("discard_crash_reports", false), hasReports)
            if (plan.action == PendingCrashAction.Delete) crash.deleteUnsentReports()
            // A failed boundary commit remains denied and must never grant send.
            val saved = reportPreferences.edit().putBoolean("discard_crash_reports", plan.discardPending).commit()
            captureEnabled = saved && plan.capture
            if (saved && plan.action == PendingCrashAction.Send && currentEnabled) crash.sendUnsentReports()
        } }
    }
    override fun analytics(enabled: Boolean) { analyticsEnabled = enabled; analytics.setAnalyticsCollectionEnabled(enabled) }
    override fun crashes(enabled: Boolean) = synchronized(reportLock) {
        if (!enabled) {
            crashesEnabled = false; captureEnabled = false
            crash.setCrashlyticsCollectionEnabled(false)
            // The controller atomically persists OFF plus the discard boundary.
            crash.deleteUnsentReports()
        } else crashesEnabled = true // Upload/capture boundary changes after restart.
    }
    override fun opened() { if (analyticsEnabled) analytics.logEvent("app_open", null) }
    override fun screen(value: TelemetryScreen) { if (analyticsEnabled) analytics.logEvent("screen_open", Bundle().apply { putString("screen", value.name.lowercase()) }) }
    override fun failure(value: DiagnosticFailure) { if (crashesEnabled && captureEnabled) crash.recordException(SafeDiagnostics.failure(value)) }
}

class ZerusFirebaseService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        OptionalFirebase.tokenChanged(this, token)
    }
    override fun onMessageReceived(message: RemoteMessage) {
        // No payload enters notifications/telemetry. Fetch authoritative events instead.
        PrivateStore(this).connections().forEach { SessionNotifications.wake(this, it.id); SessionNotifications.enqueue(this, it.id) }
    }
}
class FirebaseTokenWorker(context: Context, parameters: WorkerParameters) : CoroutineWorker(context, parameters) {
    override suspend fun doWork(): Result {
        val store = PrivateStore(applicationContext)
        val connection = store.connections().find { it.id == inputData.getString("connection") } ?: return Result.success()
        val token = store.firebaseToken()
        if (!FcmRegistrationPolicy.needsRegistration(token, store.firebaseBinding(connection.id), store.pushProvider(connection.id))) return Result.success()
        return PushSetupLocks.withConnection(connection.id) { try {
            val api = RelayApi()
            val providers = api.call(connection.url, connection.token, "/v1/capabilities").optJSONArray("push_providers")
            if (providers == null || (0 until providers.length()).none { providers.optString(it) == "fcm" }) {
                store.savePushStatusIfCurrent(connection, "fcm", "Gateway Firebase push is not configured. Use UnifiedPush or the live connection.")
                return@withConnection Result.failure()
            }
            // Push registration is idempotent and separate from native session mutations.
            if (!store.connections().contains(connection) || store.firebaseToken() != token || store.pushProvider(connection.id) == "unifiedpush") return@withConnection Result.success()
            api.call(connection.url, connection.token, "/v1/push", JSONObject().put("provider", "fcm").put("token", token))
            store.confirmFirebaseBinding(connection, token)
            Result.success()
        } catch (e: CancellationException) { throw e }
        catch (e: RelayException) {
            store.savePushStatusIfCurrent(connection, "fcm", "Firebase push setup waiting for gateway connection.")
            if (FcmRegistrationPolicy.retry(e.status)) Result.retry() else Result.failure()
        } catch (_: Exception) { AppTelemetry.failure(DiagnosticFailure.PushRegistration); Result.retry() }
        }
    }
}
