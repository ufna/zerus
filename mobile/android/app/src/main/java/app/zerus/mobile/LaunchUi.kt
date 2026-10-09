package app.zerus.mobile

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

@Composable internal fun NewSessionButton(model: ZerusViewModel) {
    var choosing by remember { mutableStateOf(false) }
    IconButton(onClick = { choosing = true },enabled = !model.demo && model.connections.isNotEmpty()) { Icon(Icons.Default.Add,"New session") }
    if(choosing) {
        ObscureConversation()
        AlertDialog(onDismissRequest = { choosing = false },title = { Text("New session on which machine?") },text = {
            Column(Modifier.heightIn(max = 440.dp).verticalScroll(rememberScrollState()),verticalArrangement = Arrangement.spacedBy(8.dp)) {
                model.machines.forEach { machine ->
                    val key = MachineKey(machine.connectionId,machine.id)
                    val reason = model.launchReason(key)
                    TextButton(onClick = { choosing = false;model.prepareLaunch(key) },enabled = reason.isBlank(),modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.fillMaxWidth()) {
                            MachineLabel(machine.name,colorHex = model.machineColor(key))
                            if(model.connections.size > 1) Text(model.connections.find { it.id == machine.connectionId }?.displayName.orEmpty(),style = MaterialTheme.typography.labelSmall)
                            if(reason.isNotBlank()) Text(reason,style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
                if(model.machines.isEmpty()) Text("Refresh your connected machines first.")
            }
        },confirmButton = { TextButton(onClick = { choosing = false }) { Text("Cancel") } })
    }
}

@Composable internal fun LaunchSessionDialog(model: ZerusViewModel) {
    if(!model.launchVisible) return
    val machine = model.launchMachine ?: return
    ObscureConversation()
    var provider by remember(machine) { mutableStateOf("") }
    var account by remember(machine) { mutableStateOf("") }
    var projectId by remember(machine) { mutableStateOf("") }
    var folder by remember(machine) { mutableStateOf("") }
    var browsedFolder by remember(machine) { mutableStateOf("") }
    var tag by remember(machine) { mutableStateOf("") }
    var browser by remember(machine) { mutableStateOf(false) }
    var allowedCatalog by remember(machine) { mutableStateOf("") }
    val initialLaunchIds = remember(machine) { model.sessionActions.filter { it.operation == "launch" && it.status == "completed" }.map { it.requestId }.toSet() }
    val signature = model.launchCatalogSignature()
    LaunchedEffect(signature) { if(allowedCatalog.isBlank() && signature.isNotBlank()) allowedCatalog = signature }
    val providers = remember(model.launchCatalogState) { LaunchPresentation.agents(model.launchCatalogState) }
    val accounts = remember(model.launchCatalogState,provider) { LaunchPresentation.accounts(model.launchCatalogState,provider) }
    val projects = remember(model.launchCatalogState) { LaunchPresentation.projects(model.launchCatalogState) }
    val projectSupported = model.launchCatalogState?.opt("project_launch_supported")==true
    val project = projects.find { it.id == projectId }
    LaunchedEffect(providers) { if(provider !in providers) provider=providers.firstOrNull().orEmpty() }
    LaunchedEffect(accounts) { account=LaunchPresentation.defaultAccount(accounts,account) }
    LaunchedEffect(projects) {
        if(projects.none { it.id == projectId }) {
            val preferred=model.selectedProject?.key?.takeIf { it.connectionId==machine.connectionId && it.swarmId==model.launchCatalogState?.string("swarm_id") }?.id
                ?: model.launchCatalogState?.string("default_project")
            projectId=projects.find { it.id==preferred }?.id ?: projects.firstOrNull()?.id.orEmpty()
            folder=browsedFolder
        }
    }
    LaunchedEffect(projectId,model.launchDirectoryLoading) {
        if(folder.isBlank() && !model.launchDirectoryLoading) project?.folders?.firstOrNull()?.let {
            folder=it.path;model.browseLaunchDirectory(machine,it.path)
        }
    }
    val folderValid = LaunchPresentation.selectedFolder(model.launchDirectoryState,folder)
    val changed = allowedCatalog.isNotBlank() && signature != allowedCatalog
    val reason = model.launchReason(machine)
    val nameError = LaunchPresentation.nameError(tag)
    val pending = model.sessionActions.lastOrNull { it.operation == "launch" && it.target.connectionId == machine.connectionId && it.target.computerId == machine.computerId && it.status in setOf("sending","uncertain","failed") }
    val confirmed = model.sessionActions.lastOrNull { it.operation == "launch" && it.target.connectionId == machine.connectionId && it.target.computerId == machine.computerId &&
        it.status == "completed" && it.resultTarget != null && it.requestId !in initialLaunchIds }
    val computer = model.machines.find { it.id == machine.computerId && it.connectionId == machine.connectionId }?.name ?: machine.computerId
    fun close() { if(!model.launchSubmitting) model.closeLaunch() }
    AlertDialog(onDismissRequest = ::close,title = { Column(verticalArrangement=Arrangement.spacedBy(6.dp)) { Text("New session");MachineLabel(computer,colorHex = model.machineColor(machine)) } },text = {
        Column(Modifier.heightIn(max = 540.dp).verticalScroll(rememberScrollState()),verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if(model.launchCatalogLoading) LinearProgressIndicator(Modifier.fillMaxWidth())
            if(changed) {
                Text("Creation choices changed. Review them before creating a session.",color = MaterialTheme.colorScheme.secondary)
                TextButton(onClick = { allowedCatalog=signature;provider="";account="";projectId="";folder="" }) { Text("Review updated choices") }
            }
            LaunchSelector("Project",project?.name ?: "Choose project",!model.launchSubmitting && projectSupported,
                projects.map { it.id to it.name },projectId) { projectId=it;folder=browsedFolder }
            if(!projectSupported && !model.launchCatalogLoading) Text("Project assignment is unavailable on this computer. Update its connector and native CLI to choose a project.",style=MaterialTheme.typography.bodySmall)
            Text("Folder",fontWeight=FontWeight.SemiBold)
            if(project?.folders?.isNotEmpty()==true) LaunchSelector(null,
                project.folders.find { it.path==folder }?.name ?: "Choose project folder",!model.launchSubmitting,
                project.folders.map { it.path to it.name },folder) { folder=it;browsedFolder="";model.browseLaunchDirectory(machine,it) }
            if(folder.isNotBlank()) SelectionContainer { Text(folder,style=MaterialTheme.typography.bodySmall) }
            OutlinedButton(onClick = { browser=true },enabled = !model.launchSubmitting) { Icon(Icons.Default.Folder,null);Spacer(Modifier.width(8.dp));Text(if(folder.isBlank()) "Browse folder" else "Browse another folder") }
            if(model.launchDirectoryLoading) LinearProgressIndicator(Modifier.fillMaxWidth())
            if(folder.isNotBlank() && !folderValid && !model.launchDirectoryLoading) Text("Choose this folder again in the current native browser.",style=MaterialTheme.typography.bodySmall)
            if(project!=null && folder.isNotBlank() && project.folders.none { it.path==folder }) Text("This folder will be added to ${project.name}.",style=MaterialTheme.typography.bodySmall)
            LaunchSelector("Agent",DesktopIcons.providerName(provider).ifBlank { "Choose agent" },!model.launchSubmitting,
                providers.map { it to DesktopIcons.providerName(it) },provider,providerIcons=true) { provider=it;account="" }
            if(providers.isEmpty() && !model.launchCatalogLoading) Text("No installed native providers were reported.",style=MaterialTheme.typography.bodySmall)
            val selectedAccount=accounts.find { it.id==account }
            LaunchSelector("Account",selectedAccount?.let { LaunchPresentation.accountTitle(it,accounts) } ?: "Choose account",!model.launchSubmitting && provider.isNotBlank(),
                accounts.map { it.id to LaunchPresentation.accountTitle(it,accounts) },account) { account=it }
            selectedAccount?.let { LaunchPresentation.accountStatus(it) }?.takeIf { it.isNotBlank() }?.let { Text(it,style=MaterialTheme.typography.bodySmall) }
            OutlinedTextField(tag,{ tag=it },Modifier.fillMaxWidth(),label={ Text("Session name") },singleLine=true,readOnly=model.launchSubmitting)
            if(tag.isNotEmpty() && nameError.isNotBlank()) Text(nameError,color=MaterialTheme.colorScheme.secondary,style=MaterialTheme.typography.bodySmall)
            Text("Uses the existing folder. No Git repository or worktree is created.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(model.launchError.isNotBlank()) Text(model.launchError,color=MaterialTheme.colorScheme.secondary,style=MaterialTheme.typography.bodySmall)
            if(reason.isNotBlank()) Text(reason,style=MaterialTheme.typography.bodySmall)
            pending?.let { action ->
                if(action.status=="uncertain") Text("Launch result unknown. A native session may already exist. This request has not been retried.",style=MaterialTheme.typography.bodySmall)
                TextButton(onClick={ model.checkSessionAction(action) },enabled=!model.launchSubmitting && "action:${action.requestId}" !in model.receiptFlights) { Text("Check original launch result") }
            }
            confirmed?.let { action ->
                Text("The machine confirmed creation of ${action.resultTarget?.session}.",style=MaterialTheme.typography.bodySmall)
                if(action.error.isNotBlank() && action.error!=model.launchError) Text(action.error,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.secondary)
                TextButton(onClick={ model.openSessionActionResult(action) }) { Text("Open confirmed session") }
                if(model.actionResultNotice.isNotBlank()) Text(model.actionResultNotice,style=MaterialTheme.typography.bodySmall)
            }
        }
    },confirmButton={ TextButton(onClick={ model.launchSession(machine,provider,folder,tag,account,allowedCatalog,projectId) },
        enabled=!model.launchSubmitting && !model.launchCatalogLoading && !model.launchDirectoryLoading && !changed && allowedCatalog.isNotBlank() &&
            reason.isBlank() && provider in providers && folderValid && nameError.isBlank() && accounts.any { it.id==account } && (!projectSupported || project!=null) && confirmed==null) { Text(if(model.launchSubmitting) "Creating…" else "Create session") } },
        dismissButton={ TextButton(onClick=::close,enabled=!model.launchSubmitting) { Text("Cancel") } })
    if(browser) LaunchFolderDialog(model,machine,{ selected -> folder=selected;browsedFolder=selected;browser=false }) { browser=false }
}

@Composable private fun LaunchSelector(label:String?,value:String,enabled:Boolean,choices:List<Pair<String,String>>,selected:String,providerIcons:Boolean=false,onChoose:(String)->Unit) {
    var expanded by remember { mutableStateOf(false) }
    Column {
        if(label!=null) Text(label,fontWeight=FontWeight.SemiBold)
        Box {
            OutlinedButton(onClick={ expanded=true },enabled=enabled && choices.isNotEmpty(),modifier=Modifier.fillMaxWidth()) {
                if(providerIcons && selected.isNotBlank()) { ProviderBadge(selected,Modifier.size(24.dp));Spacer(Modifier.width(8.dp)) }
                Text(value,Modifier.weight(1f));Icon(Icons.Default.ArrowDropDown,"Show choices",Modifier.padding(start=8.dp))
            }
            DropdownMenu(expanded && enabled,onDismissRequest={ expanded=false }) {
                choices.forEach { (id,title) -> DropdownMenuItem(text={ Row(verticalAlignment=Alignment.CenterVertically) {
                    if(providerIcons) { ProviderBadge(id,Modifier.size(24.dp));Spacer(Modifier.width(8.dp)) }
                    Text(title,fontWeight=if(id==selected) FontWeight.SemiBold else FontWeight.Normal)
                } },onClick={ expanded=false;onChoose(id) }) }
            }
        }
    }
}

@Composable private fun LaunchFolderDialog(model: ZerusViewModel,machine: MachineKey,onChoose: (String) -> Unit,onDismiss: () -> Unit) {
    val raw = model.launchDirectoryState
    val path = raw?.string("path").orEmpty()
    val parent = raw?.string("parent").orEmpty()
    val home = raw?.string("home").orEmpty()
    var typedPath by remember(machine) { mutableStateOf(path) }
    val folders = remember(raw) { LaunchPresentation.folders(raw) }
    AlertDialog(onDismissRequest = onDismiss,title = { Text("Choose folder on machine") },text = {
        Column(Modifier.heightIn(max = 480.dp).verticalScroll(rememberScrollState()),verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if(model.launchDirectoryLoading) LinearProgressIndicator(Modifier.fillMaxWidth())
            SelectionContainer { Text(path.ifBlank { "Loading native folders…" },style = MaterialTheme.typography.bodySmall) }
            Row {
                TextButton(onClick = { model.browseLaunchDirectory(machine,parent) },enabled = parent.isNotBlank() && !model.launchDirectoryLoading) { Text("Up") }
                TextButton(onClick = { model.browseLaunchDirectory(machine,home.ifBlank { "~" }) },enabled = !model.launchDirectoryLoading) { Text("Home") }
            }
            OutlinedTextField(typedPath,{ typedPath = it },Modifier.fillMaxWidth(),label = { Text("Browse absolute path") },singleLine = true)
            TextButton(onClick = { model.browseLaunchDirectory(machine,typedPath) },enabled = LaunchPresentation.absoluteFolder(typedPath) && !model.launchDirectoryLoading) { Text("Browse") }
            folders.forEach { folder -> TextButton(onClick = { model.browseLaunchDirectory(machine,folder.path) },enabled = !model.launchDirectoryLoading,modifier = Modifier.fillMaxWidth()) {
                Row(Modifier.fillMaxWidth(),verticalAlignment = Alignment.CenterVertically) { Icon(Icons.Default.Folder,null);Text(folder.name,Modifier.padding(start = 8.dp)) }
            } }
            if(raw?.optBoolean("truncated") == true) Text("Folder listing is incomplete. Browse an exact path to reach another folder.",style = MaterialTheme.typography.bodySmall)
            if(model.launchError.isNotBlank()) Text(model.launchError,color = MaterialTheme.colorScheme.secondary,style = MaterialTheme.typography.bodySmall)
        }
    },confirmButton = { TextButton(onClick = { onChoose(path) },enabled = LaunchPresentation.absoluteFolder(path) && !model.launchDirectoryLoading) { Text("Use this folder") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } })
}
