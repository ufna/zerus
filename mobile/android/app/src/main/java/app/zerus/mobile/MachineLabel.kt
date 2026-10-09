package app.zerus.mobile

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/** The same display label in session cards and machine selection. Identity stays in the caller. */
@Composable internal fun MachineLabel(name:String,modifier:Modifier=Modifier,colorHex:String?=null) {
    val dark=MaterialTheme.colorScheme.surface.luminance()<.5f
    val checked=colorHex?.let { runCatching { MachineColors.checked(it) }.getOrNull() }
    val foreground=checked?.let { Color(MachineColors.foreground(it,dark)) } ?: MaterialTheme.colorScheme.onSurfaceVariant
    val background=checked?.let { Color(MachineColors.background(it,dark)) } ?: MaterialTheme.colorScheme.surfaceVariant
    Surface(modifier,color=background,shape=RoundedCornerShape(7.dp)) {
        Row(Modifier.padding(horizontal=7.dp,vertical=3.dp),verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(5.dp)) {
            Icon(DesktopIcons.Machines,null,Modifier.size(14.dp),tint=foreground)
            Text(name,maxLines=1,overflow=TextOverflow.Ellipsis,style=MaterialTheme.typography.labelSmall,color=foreground)
        }
    }
}
