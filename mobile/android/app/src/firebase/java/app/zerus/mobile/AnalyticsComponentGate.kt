package app.zerus.mobile

import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build

/** Measurement's independent background entrypoints must follow owner intent. */
internal class AnalyticsComponentGate(context: Context) {
    private val manager = context.packageManager
    private val components = listOf(
        "com.google.android.gms.measurement.AppMeasurementService",
        "com.google.android.gms.measurement.AppMeasurementJobService",
        "com.google.android.gms.measurement.AppMeasurementReceiver"
    ).map { ComponentName(context.packageName, it) }

    fun reconcile(enabled: Boolean, durable: Boolean) {
        val desired = if (enabled) PackageManager.COMPONENT_ENABLED_STATE_ENABLED else PackageManager.COMPONENT_ENABLED_STATE_DISABLED
        val flags = PackageManager.DONT_KILL_APP or
            if (durable && Build.VERSION.SDK_INT >= 30) PackageManager.SYNCHRONOUS else 0
        components.forEach { component ->
            val current = manager.getComponentEnabledSetting(component)
            // Manifest enabled is required for SDK validation. Explicitly apply
            // owner OFF even from DEFAULT before any Firebase initialization.
            // Only settings changes request synchronous disk persistence on IO.
            if (current != desired) {
                manager.setComponentEnabledSetting(component, desired, flags)
            }
        }
    }
}
