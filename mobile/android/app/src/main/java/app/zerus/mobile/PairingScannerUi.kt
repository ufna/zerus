package app.zerus.mobile

import android.Manifest
import android.content.pm.PackageManager
import android.util.Size
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import java.util.concurrent.Executors

@Composable internal fun ScanPairingButton(enabled: Boolean, onInvite: (PairingInvite) -> Unit) {
    val context = LocalContext.current
    var scanning by remember { mutableStateOf(false) }
    var permissionPending by remember { mutableStateOf(false) }
    var notice by remember { mutableStateOf("") }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        permissionPending = false
        scanning = granted
        notice = if(granted) "" else "Camera access was not granted. You can enter the invitation manually."
    }
    OutlinedButton(enabled = enabled && !permissionPending, onClick = {
        notice = ""
        if(ContextCompat.checkSelfPermission(context,Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) scanning = true
        else { permissionPending = true;permission.launch(Manifest.permission.CAMERA) }
    }) { Icon(Icons.Default.QrCodeScanner,null,Modifier.size(20.dp));Spacer(Modifier.width(8.dp));Text("Scan QR") }
    if(notice.isNotBlank()) Text(notice,style = MaterialTheme.typography.bodySmall)
    if(scanning) PairingScannerDialog(onDismiss = { scanning = false }, onInvite = { scanning = false;onInvite(it) })
}

@Composable private fun PairingScannerDialog(onDismiss: () -> Unit, onInvite: (PairingInvite) -> Unit) {
    ObscureConversation()
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val currentInvite by rememberUpdatedState(onInvite)
    val currentDismiss by rememberUpdatedState(onDismiss)
    var notice by remember { mutableStateOf("Point the camera at a Zerus pairing QR code.") }
    val gate = remember { PairingScanGate() }
    fun cancelScanner() { gate.cancelled();onDismiss() }
    val previewView = remember { PreviewView(context).apply { implementationMode = PreviewView.ImplementationMode.COMPATIBLE;scaleType = PreviewView.ScaleType.FILL_CENTER } }
    DisposableEffect(owner,previewView) {
        var disposed = false
        var provider: ProcessCameraProvider? = null
        var bound = false
        val executor = Executors.newSingleThreadExecutor()
        val main = ContextCompat.getMainExecutor(context)
        val preview = Preview.Builder().build().also { it.surfaceProvider = previewView.surfaceProvider }
        val analysis = ImageAnalysis.Builder().setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .setResolutionSelector(ResolutionSelector.Builder().setResolutionStrategy(ResolutionStrategy(Size(1280,720),ResolutionStrategy.FALLBACK_RULE_CLOSEST_LOWER_THEN_HIGHER)).build()).build()
        analysis.setAnalyzer(executor) { frame ->
            try {
                if(gate.isActive()) {
                    val plane = frame.planes.firstOrNull()
                    val crop = frame.cropRect
                    val bytes = plane?.let { PairingQrDecoder.luminance(it.buffer,it.rowStride,it.pixelStride,crop.left,crop.top,crop.width(),crop.height()) }
                    val decoded = bytes?.let { PairingQrDecoder.decode(it,crop.width(),crop.height()) }
                    if(decoded != null) {
                        val invite = PairingInvite.parse(decoded)
                        main.execute {
                            if(!disposed && owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) && gate.isActive()) {
                                if(invite != null && gate.claim()) currentInvite(invite)
                                else if(invite == null) notice = "This QR code is not a valid Zerus invitation. Try another, or enter the code manually."
                            }
                        }
                    }
                }
            } catch(_: Exception) { /* Invalid frames never expose their contents. */ }
            finally { frame.close() }
        }
        fun stop() { gate.paused();provider?.unbind(preview,analysis);bound = false }
        fun start() {
            if(disposed || bound || !owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) return
            val camera = provider ?: return
            try {
                val selector = when {
                    camera.hasCamera(CameraSelector.DEFAULT_BACK_CAMERA) -> CameraSelector.DEFAULT_BACK_CAMERA
                    camera.hasCamera(CameraSelector.DEFAULT_FRONT_CAMERA) -> CameraSelector.DEFAULT_FRONT_CAMERA
                    else -> error("No camera")
                }
                gate.resumed();if(!gate.isActive()) return
                camera.bindToLifecycle(owner,selector,preview,analysis);bound = true
            } catch(_: Exception) { stop();notice = "Camera unavailable. Close the scanner and enter the invitation manually." }
        }
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener({ if(!disposed) {
            try { provider = future.get();start() }
            catch(_: Exception) { notice = "Camera unavailable. Close the scanner and enter the invitation manually." }
        } },main)
        val observer = LifecycleEventObserver { _,event ->
            when(event) { Lifecycle.Event.ON_RESUME -> start();Lifecycle.Event.ON_PAUSE -> { gate.cancelled();stop();currentDismiss() };else -> Unit }
        }
        owner.lifecycle.addObserver(observer)
        onDispose { gate.cancelled();disposed = true;owner.lifecycle.removeObserver(observer);stop();analysis.clearAnalyzer();executor.shutdown() }
    }
    Dialog(onDismissRequest = ::cancelScanner,properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().systemBarsPadding().padding(16.dp),verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text("Scan pairing QR",style = MaterialTheme.typography.headlineSmall)
                AndroidView(factory = { previewView },modifier = Modifier.fillMaxWidth().weight(1f).background(androidx.compose.ui.graphics.Color.Black))
                Text(notice,style = MaterialTheme.typography.bodyMedium)
                Text("Images stay on this phone. Review the gateway and code before pairing.",style = MaterialTheme.typography.bodySmall)
                OutlinedButton(onClick = ::cancelScanner,modifier = Modifier.fillMaxWidth()) { Text("Cancel scan") }
            }
        }
    }
}
