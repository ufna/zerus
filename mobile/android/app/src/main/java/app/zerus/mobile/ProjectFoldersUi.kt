package app.zerus.mobile

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.FolderOpen
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

private fun folderBasename(folder:ProjectFolder)=folder.path.trimEnd('/').substringAfterLast('/')
private fun folderName(folder:ProjectFolder)=folder.name.ifBlank { folderBasename(folder).ifBlank { folder.path } }

@Composable private fun ProjectFolderRow(folder:ProjectFolder,muted:Color) {
    Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(8.dp)) {
        Icon(Icons.Filled.FolderOpen,null,Modifier.padding(top=2.dp).size(16.dp),tint=muted)
        Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(2.dp)) {
            Text(folderName(folder),style=MaterialTheme.typography.bodySmall,fontWeight=FontWeight.SemiBold)
            SelectionContainer {
                Text(folder.path,Modifier.fillMaxWidth(),style=MaterialTheme.typography.bodySmall,color=muted)
            }
        }
    }
}

@Composable internal fun ProjectFoldersPreview(model:ZerusViewModel,project:ProjectSummary,muted:Color) {
    // groupBy keeps the catalog's machine and folder order; exact IDs own grouping.
    val groups=project.folders.groupBy { it.computerId }
    groups.entries.take(2).forEach { (computerId,folders) ->
        Column(Modifier.fillMaxWidth(),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            MachineLabel(model.machineName(project.key.connectionId,computerId,folders.first().computerName),colorHex=model.machineColor(MachineKey(project.key.connectionId,computerId)))
            folders.take(2).forEach { folder -> ProjectFolderRow(folder,muted) }
            if(folders.size>2) Text("+${folders.size-2} more ${if(folders.size==3) "folder" else "folders"}",color=muted,style=MaterialTheme.typography.labelSmall)
        }
    }
    if(groups.isEmpty()) Text("No saved folders on connected machines",color=muted,style=MaterialTheme.typography.bodySmall)
    else if(groups.size>2) Text("+${groups.size-2} more ${if(groups.size==3) "machine" else "machines"}",color=muted,style=MaterialTheme.typography.bodySmall)
}

@Composable internal fun ProjectFolderDetails(model:ZerusViewModel,project:ProjectSummary,containerColor:Color,muted:Color) {
    project.folders.groupBy { it.computerId }.forEach { (computerId,folders) ->
        Card(colors=CardDefaults.cardColors(containerColor=containerColor)) {
            Column(Modifier.fillMaxWidth().padding(14.dp),verticalArrangement=Arrangement.spacedBy(10.dp)) {
                MachineLabel(model.machineName(project.key.connectionId,computerId,folders.first().computerName),colorHex=model.machineColor(MachineKey(project.key.connectionId,computerId)))
                folders.forEach { folder -> ProjectFolderRow(folder,muted) }
            }
        }
    }
    if(project.folders.isEmpty()) Text("No saved folders on connected machines",color=muted)
}
