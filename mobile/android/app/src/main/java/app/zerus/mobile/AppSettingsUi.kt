package app.zerus.mobile

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable internal fun AppSettingsDialog(onDismiss: () -> Unit) {
    ObscureConversation()
    val context = LocalContext.current
    val preferences = remember(context) { AndroidTelemetryPreferences(context) }
    var value by remember { mutableStateOf(AppTelemetry.effective() ?: preferences.read()) }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    fun change(analytics: Boolean, enabled: Boolean) {
        saving = true
        scope.launch {
            try {
                withContext(Dispatchers.IO) { if (analytics) AppTelemetry.analytics(enabled) else AppTelemetry.crashes(enabled) }
                value = AppTelemetry.effective() ?: preferences.read(); error = false
            } catch (_: Exception) { value = AppTelemetry.effective() ?: preferences.read(); error = true }
            finally { saving = false }
        }
    }
    LaunchedEffect(Unit) { AppTelemetry.screen(TelemetryScreen.Settings) }
    AlertDialog(onDismissRequest = { if (!saving) onDismiss() }, title = { Text("Settings") }, text = {
        Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (BuildConfig.FIREBASE_ENABLED) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) { Text("Usage analytics"); Text("Share app usage, without conversation content.", style = MaterialTheme.typography.bodySmall) }
                    Switch(value.analytics, { change(true, it) }, enabled = !saving)
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) { Text("Crash reports"); Text("Share technical crash diagnostics. Changes apply after restart; reports usually upload on the next launch.", style = MaterialTheme.typography.bodySmall) }
                    Switch(value.crashes, { change(false, it) }, enabled = !saving)
                }
                Text("Firebase receives app and device information with SDK installation identifiers. Session text, names, paths, account details and connection credentials are excluded from custom events and custom diagnostic fields.", style = MaterialTheme.typography.bodySmall)
                Text("Automatic crash diagnostics include exception stacks, thread names and technical network metadata, which may include gateway addresses.", style = MaterialTheme.typography.bodySmall)
                Text("Opting out discards retained reports. After re-enabling, diagnostics resume once a startup confirms the earlier reports have been removed.", style = MaterialTheme.typography.bodySmall)
                Text("These switches do not change push notifications.", style = MaterialTheme.typography.bodySmall)
            } else Text("This build does not include Firebase analytics or crash reporting.")
            if (error) Text("Could not fully apply this setting. Check the switch and try again.", color = MaterialTheme.colorScheme.error)
        }
    }, confirmButton = { TextButton(onClick = onDismiss, enabled = !saving) { Text("Close") } })
}
