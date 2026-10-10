package app.zerus.mobile

/** Only fixed product events cross this boundary. Feature code cannot supply payloads. */
enum class TelemetryScreen { Sessions, Projects, Drafts, Accounts, Machines, Settings }
enum class DiagnosticFailure { PrivateStorage, PushRegistration }
data class TelemetryPreferences(val analytics: Boolean = true, val crashes: Boolean = true)
interface TelemetryPreferenceStore {
    fun read(): TelemetryPreferences
    fun write(value: TelemetryPreferences)
}
interface TelemetryBackend {
    fun analytics(enabled: Boolean)
    fun crashes(enabled: Boolean)
    fun opened()
    fun screen(value: TelemetryScreen)
    fun failure(value: DiagnosticFailure)
}

/** Suppress SDK collection and durably gate background components before owner OFF.
 * Enable components and lazily create Analytics only after durable owner ON.
 * SDK collection setters persist asynchronously; they are not the OFF boundary. */
class TelemetryController(private val store: TelemetryPreferenceStore, private val backend: TelemetryBackend) {
    var effective = store.read(); private set
    @Synchronized fun initialize() {
        val value = store.read()
        backend.analytics(value.analytics)
        backend.crashes(value.crashes)
    }
    @Synchronized fun analytics(enabled: Boolean) {
        if (!enabled) { effective = effective.copy(analytics = false); backend.analytics(false) }
        store.write(store.read().copy(analytics = enabled))
        if (enabled) { backend.analytics(true); effective = effective.copy(analytics = true) }
    }
    @Synchronized fun crashes(enabled: Boolean) {
        if (!enabled) { effective = effective.copy(crashes = false); backend.crashes(false) }
        store.write(store.read().copy(crashes = enabled))
        if (enabled) { backend.crashes(true); effective = effective.copy(crashes = true) }
    }
}

/** Strip arbitrary messages, causes and suppressed exceptions before SDK serialization. */
object SafeDiagnostics {
    fun failure(value: DiagnosticFailure) = IllegalStateException(value.name)
    fun fatal(original: Throwable): Throwable = RuntimeException("Uncaught application failure").apply {
        stackTrace = original.stackTrace.take(128).map { frame ->
            // Runtime-created class/method/file strings are not trusted as telemetry.
            fun safe(value: String?, fallback: String) = value?.takeIf { it.length <= 200 && it.matches(Regex("[A-Za-z0-9_.$<>-]+")) } ?: fallback
            StackTraceElement(safe(frame.className, "unknown"), safe(frame.methodName, "unknown"),
                frame.fileName?.let { safe(it, "unknown") }, frame.lineNumber.coerceIn(-2, 1_000_000))
        }.toTypedArray()
    }
}

/** Each fatal is forwarded once; the installed SDK/platform handler owns termination. */
class SanitizedCrashHandler(private val delegate: Thread.UncaughtExceptionHandler) : Thread.UncaughtExceptionHandler {
    private val fallback = RuntimeException("Uncaught application failure").apply { stackTrace = emptyArray() }
    override fun uncaughtException(thread: Thread, exception: Throwable) {
        val safe = try { SafeDiagnostics.fatal(exception) } catch (_: Throwable) { fallback }
        delegate.uncaughtException(thread, safe)
    }
}
