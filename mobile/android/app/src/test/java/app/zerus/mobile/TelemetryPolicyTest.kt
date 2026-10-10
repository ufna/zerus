package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class TelemetryPolicyTest {
    private class Store : TelemetryPreferenceStore {
        var value = TelemetryPreferences()
        var fail = false
        val calls = mutableListOf<String>()
        override fun read() = value
        override fun write(value: TelemetryPreferences) { calls += "store:${value.analytics}:${value.crashes}"; check(!fail); this.value = value }
    }
    private class Backend(val store: Store) : TelemetryBackend {
        var analytics = false
        var crashes = false
        override fun analytics(enabled: Boolean) { store.calls += "analytics:$enabled"; analytics = enabled }
        override fun crashes(enabled: Boolean) { store.calls += "crashes:$enabled"; crashes = enabled }
        override fun opened() {}
        override fun screen(value: TelemetryScreen) {}
        override fun failure(value: DiagnosticFailure) {}
    }
    @Test fun defaultsAndColdStartsHonorBothSwitches() {
        val store = Store(); val backend = Backend(store); val controller = TelemetryController(store, backend)
        controller.initialize(); assertTrue(backend.analytics); assertTrue(backend.crashes)
        controller.analytics(false); controller.crashes(false)
        val restarted = Backend(store); TelemetryController(store, restarted).initialize()
        assertFalse(restarted.analytics); assertFalse(restarted.crashes)
        controller.analytics(true); assertTrue(backend.analytics); assertFalse(backend.crashes)
    }
    @Test fun disableSdkBeforeDurableOffAndEnableAfterDurableOn() {
        val store = Store(); val backend = Backend(store); val controller = TelemetryController(store, backend)
        controller.analytics(false)
        assertEquals(listOf("analytics:false", "store:false:true"), store.calls)
        store.calls.clear(); controller.analytics(true)
        assertEquals(listOf("store:true:true", "analytics:true"), store.calls)
        store.fail = true
        try { controller.analytics(false); fail() } catch (_: IllegalStateException) {}
        assertFalse(backend.analytics); assertFalse(controller.effective.analytics)
        try { controller.analytics(true); fail() } catch (_: IllegalStateException) {}
        assertFalse(backend.analytics)
    }
    @Test fun safeFailureDropsMessagesCausesSuppressedAndDynamicFrames() {
        val secret = "https://reserved.example/private?token=fixture"
        val original = IllegalStateException(secret, RuntimeException(secret)).apply {
            addSuppressed(RuntimeException(secret))
            stackTrace = arrayOf(StackTraceElement(secret, secret, secret, 12), StackTraceElement("app.zerus.mobile.RelayApi", "call", "RelayApi.kt", 20))
        }
        val safe = SafeDiagnostics.fatal(original)
        assertNull(safe.cause); assertEquals(0, safe.suppressed.size)
        assertFalse(safe.toString().contains(secret)); assertFalse(safe.stackTrace.joinToString().contains(secret))
        assertEquals("RelayApi.kt", safe.stackTrace[1].fileName)
        DiagnosticFailure.entries.forEach { assertNull(SafeDiagnostics.failure(it).cause) }
    }
    @Test fun fatalDelegateRunsExactlyOnceWithSafeThrowableAndOriginalThread() {
        var count = 0
        val current = Thread.currentThread()
        val original = RuntimeException("fixture token")
        SanitizedCrashHandler { thread, error ->
            count++; assertSame(current, thread); assertNotSame(original, error)
            assertEquals("Uncaught application failure", error.message); assertNull(error.cause)
        }.uncaughtException(current, original)
        assertEquals(1, count)
    }
    @Test fun sanitizerFailureStillDelegatesExactlyOnce() {
        var count = 0
        val broken = object : RuntimeException("fixture secret") {
            override fun getStackTrace(): Array<StackTraceElement> = throw IllegalStateException("fixture secret")
        }
        SanitizedCrashHandler { _, error ->
            count++; assertEquals("Uncaught application failure", error.message); assertEquals(0, error.stackTrace.size)
        }.uncaughtException(Thread.currentThread(), broken)
        assertEquals(1, count)
    }
    @Test fun crashReportingPreservesOnFatalAndNeverUploadsAcrossOffBoundary() {
        assertEquals(PendingCrashAction.Send, CrashReportingPolicy.startup(true, false, true).action)
        val disabled = CrashReportingPolicy.startup(false, false, true)
        assertEquals(PendingCrashAction.Delete, disabled.action); assertTrue(disabled.discardPending); assertFalse(disabled.capture)
        val resumed = CrashReportingPolicy.startup(true, disabled.discardPending, true)
        assertEquals(PendingCrashAction.Delete, resumed.action); assertTrue(resumed.discardPending); assertFalse(resumed.capture)
        val clean = CrashReportingPolicy.startup(true, resumed.discardPending, false)
        assertFalse(clean.discardPending); assertTrue(clean.capture)
        assertEquals(PendingCrashAction.Send, CrashReportingPolicy.startup(true, clean.discardPending, true).action)
    }
    @Test fun fcmRotationRegistrationAndUnifiedPushAreIndependent() {
        assertTrue(FcmRegistrationPolicy.needsRegistration("new", "old", "fcm"))
        assertTrue(FcmRegistrationPolicy.needsRegistration("new", "", "fcm"))
        assertFalse(FcmRegistrationPolicy.needsRegistration("new", "new", "fcm"))
        assertFalse(FcmRegistrationPolicy.needsRegistration("new", "old", "unifiedpush"))
        assertFalse(FcmRegistrationPolicy.needsRegistration("", "old", "fcm"))
        assertTrue(FcmRegistrationPolicy.retry(429)); assertTrue(FcmRegistrationPolicy.retry(503)); assertFalse(FcmRegistrationPolicy.retry(401))
    }
}
