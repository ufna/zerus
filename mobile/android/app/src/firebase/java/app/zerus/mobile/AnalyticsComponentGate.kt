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
            // DEFAULT is disabled in the manifest. Avoid startup disk writes;
            // settings changes request synchronous persistence on their IO thread.
            if (current != desired && !(current == PackageManager.COMPONENT_ENABLED_STATE_DEFAULT && !enabled)) {
                manager.setComponentEnabledSetting(component, desired, flags)
            }
        }
    }
}
