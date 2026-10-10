package app.zerus.mobile

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Activity-owned operation state survives rotation while durable settings finish on IO. */
class AppSettingsViewModel(application: Application) : AndroidViewModel(application) {
    private val preferences = AndroidTelemetryPreferences(application)
    var value by mutableStateOf(AppTelemetry.effective() ?: preferences.read()); private set
    var saving by mutableStateOf(false); private set
    var error by mutableStateOf(false); private set

    fun refresh() { if (!saving) value = AppTelemetry.effective() ?: preferences.read() }
    fun change(analytics: Boolean, enabled: Boolean) {
        if (saving) return
        saving = true
        viewModelScope.launch {
            try {
                withContext(Dispatchers.IO) { if (analytics) AppTelemetry.analytics(enabled) else AppTelemetry.crashes(enabled) }
                value = AppTelemetry.effective() ?: preferences.read(); error = false
            } catch (_: Exception) { value = AppTelemetry.effective() ?: preferences.read(); error = true }
            finally { saving = false }
        }
    }
}
