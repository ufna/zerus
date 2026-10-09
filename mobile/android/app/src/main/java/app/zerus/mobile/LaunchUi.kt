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
import org.json.JSONObject

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
    var tag by remember(machine) { mutableStateOf(LaunchPresentation.generatedName()) }
    var worktreePicker by remember(machine) { mutableStateOf(false) }
    var creatingWorktree by remember(machine) { mutableStateOf(false) }
    var browser by remember(machine) { mutableStateOf(false) }
    var allowedCatalog by remember(machine) { mutableStateOf("") }
    val initialLaunchIds = remember(machine) { model.sessionActions.filter { it.operation in setOf("launch", "worktree_create") && it.status == "completed" }.map { it.requestId }.toSet() }
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
    val currentWorktrees = model.launchWorktreeState
    val worktrees = currentWorktrees?.takeIf { WorktreePresentation.contains(it,folder) }
    val related = project?.let { WorktreePresentation.anchor(worktrees,it,folder) } != null
    val busy = model.launchSubmitting || model.worktreeSubmitting
    val pendingWorktree = model.sessionActions.lastOrNull { it.operation=="worktree_create" && it.target.connectionId==machine.connectionId && it.target.computerId==machine.computerId && it.status in setOf("sending","uncertain","failed") }
    val folderValid = LaunchPresentation.selectedFolder(model.launchDirectoryState,folder)
    val changed = allowedCatalog.isNotBlank() && signature != allowedCatalog
    val reason = model.launchReason(machine)
    val nameError = LaunchPresentation.nameError(tag)
    val pending = model.sessionActions.lastOrNull { it.operation == "launch" && it.target.connectionId == machine.connectionId && it.target.computerId == machine.computerId && it.status in setOf("sending","uncertain","failed") }
    val confirmed = model.sessionActions.lastOrNull { it.operation == "launch" && it.target.connectionId == machine.connectionId && it.target.computerId == machine.computerId &&
        it.status == "completed" && it.resultTarget != null && it.requestId !in initialLaunchIds }
    val computer = model.machines.find { it.id == machine.computerId && it.connectionId == machine.connectionId }?.name ?: machine.computerId
    fun close() { if(!busy) model.closeLaunch() }
    AlertDialog(onDismissRequest = ::close,title = { Column(verticalArrangement=Arrangement.spacedBy(6.dp)) { Text("New session");MachineLabel(computer,colorHex = model.machineColor(machine)) } },text = {
        Column(Modifier.heightIn(max = 540.dp).verticalScroll(rememberScrollState()),verticalArrangement = Arrangement.spacedBy(6.dp)) {
            if(model.launchCatalogLoading) LinearProgressIndicator(Modifier.fillMaxWidth())
            if(changed) {
                Text("Creation choices changed. Review them before creating a session.",color = MaterialTheme.colorScheme.secondary)
                TextButton(onClick = { allowedCatalog=signature;provider="";account="";projectId="";folder="" }) { Text("Review updated choices") }
            }
            LaunchSelector("Project",project?.name ?: "Choose project",!busy && projectSupported,
                projects.map { it.id to it.name },projectId) { projectId=it;folder=browsedFolder }
            if(!projectSupported && !model.launchCatalogLoading) Text("Project assignment is unavailable on this computer. Update its connector and native CLI to choose a project.",style=MaterialTheme.typography.bodySmall)
            Text("Folder",fontWeight=FontWeight.SemiBold)
            if(project?.folders?.isNotEmpty()==true) LaunchSelector(null,
                project.folders.find { it.path==folder }?.name ?: "Choose project folder",!busy,
                project.folders.map { it.path to it.name },folder) { folder=it;browsedFolder="";model.browseLaunchDirectory(machine,it) }
            if(folder.isNotBlank()) SelectionContainer { Text(folder,style=MaterialTheme.typography.bodySmall) }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = { browser=true },enabled = !busy) { Icon(Icons.Default.Folder,null,Modifier.size(18.dp));Spacer(Modifier.width(4.dp));Text("Browse") }
                if(WorktreePresentation.healthy(currentWorktrees)) TextButton(onClick = { worktreePicker=true },enabled = !busy && !model.launchWorktreeLoading) { Text("Worktrees") }
            }
            if(model.launchDirectoryLoading) LinearProgressIndicator(Modifier.fillMaxWidth())
            if(folder.isNotBlank() && !folderValid && !model.launchDirectoryLoading) Text("Choose this folder again in the current native browser.",style=MaterialTheme.typography.bodySmall)
            if(related) Text("Verified worktree of ${project?.name}. No project folder will be added.",style=MaterialTheme.typography.bodySmall)
            else if(project!=null && folder.isNotBlank() && project.folders.none { it.path==folder }) Text("This folder will be added to ${project.name}.",style=MaterialTheme.typography.bodySmall)
            LaunchSelector("Agent",DesktopIcons.providerName(provider).ifBlank { "Choose agent" },!busy,
                providers.map { it to DesktopIcons.providerName(it) },provider,providerIcons=true) { provider=it;account="" }
            if(providers.isEmpty() && !model.launchCatalogLoading) Text("No installed native providers were reported.",style=MaterialTheme.typography.bodySmall)
            val selectedAccount=accounts.find { it.id==account }
            LaunchSelector("Account",selectedAccount?.let { LaunchPresentation.accountTitle(it,accounts) } ?: "Choose account",!busy && provider.isNotBlank(),
                accounts.map { it.id to LaunchPresentation.accountTitle(it,accounts) },account) { account=it }
            selectedAccount?.let { LaunchPresentation.accountStatus(it) }?.takeIf { it.isNotBlank() }?.let { Text(it,style=MaterialTheme.typography.bodySmall) }
            OutlinedTextField(tag,{ tag=it },Modifier.fillMaxWidth(),label={ Text("Session name") },singleLine=true,readOnly=busy)
            if(tag.isNotEmpty() && nameError.isNotBlank()) Text(nameError,color=MaterialTheme.colorScheme.secondary,style=MaterialTheme.typography.bodySmall)
            pendingWorktree?.let { action ->
                Text(if(action.status=="failed") "Worktree was not created. ${action.error}" else "Worktree creation is unconfirmed. Check the original result before creating another.",style=MaterialTheme.typography.bodySmall)
                TextButton(onClick={ model.checkSessionAction(action) },enabled=!busy && "action:${action.requestId}" !in model.receiptFlights) { Text("Check original worktree result") }
            }
            model.sessionActions.lastOrNull { it.operation=="worktree_create" && it.target.connectionId==machine.connectionId && it.target.computerId==machine.computerId && it.status=="completed" && it.requestId !in initialLaunchIds }?.let { action ->
                Text("Worktree created: ${action.resultPath}. It remains if you cancel this session.",style=MaterialTheme.typography.bodySmall)
                if(action.resultPath!=folder) TextButton(onClick={ folder=action.resultPath;browsedFolder=folder;model.browseLaunchDirectory(machine,folder) },enabled=!busy) { Text("Use created worktree") }
            }
            if(model.launchError.isNotBlank()) Text(model.launchError,color=MaterialTheme.colorScheme.secondary,style=MaterialTheme.typography.bodySmall)
            if(reason.isNotBlank()) Text(reason,style=MaterialTheme.typography.bodySmall)
            pending?.let { action ->
                if(action.status=="uncertain") Text("Launch result unknown. A native session may already exist. This request has not been retried.",style=MaterialTheme.typography.bodySmall)
                TextButton(onClick={ model.checkSessionAction(action) },enabled=!busy && "action:${action.requestId}" !in model.receiptFlights) { Text("Check original launch result") }
            }
            confirmed?.let { action ->
                Text("The machine confirmed creation of ${action.resultTarget?.session}.",style=MaterialTheme.typography.bodySmall)
                if(action.error.isNotBlank() && action.error!=model.launchError) Text(action.error,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.secondary)
                TextButton(onClick={ model.openSessionActionResult(action) }) { Text("Open confirmed session") }
                if(model.actionResultNotice.isNotBlank()) Text(model.actionResultNotice,style=MaterialTheme.typography.bodySmall)
            }
        }
    },confirmButton={ TextButton(onClick={ model.launchSession(machine,provider,folder,tag,account,allowedCatalog,projectId,worktrees) },
        enabled=!busy && !model.launchCatalogLoading && !model.launchDirectoryLoading && !model.launchWorktreeLoading && !changed && allowedCatalog.isNotBlank() &&
            reason.isBlank() && provider in providers && folderValid && nameError.isBlank() && accounts.any { it.id==account } && (!projectSupported || project!=null) && confirmed==null) { Text(if(busy) "Creating…" else "Create session") } },
        dismissButton={ TextButton(onClick=::close,enabled=!busy) { Text("Cancel") } })
    if(worktreePicker && currentWorktrees!=null) {
        AlertDialog(onDismissRequest={ worktreePicker=false },title={ Text("Worktrees on this machine") },text={
            Column(Modifier.heightIn(max=400.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(6.dp)) {
                WorktreePresentation.entries(currentWorktrees).forEach { entry -> TextButton(onClick={ folder=entry.path;browsedFolder=folder;worktreePicker=false;model.browseLaunchDirectory(machine,folder) },modifier=Modifier.fillMaxWidth()) {
                    Column(Modifier.fillMaxWidth()) { Text(entry.branch.ifBlank { entry.kind });Text(entry.path,style=MaterialTheme.typography.bodySmall) }
                } }
            }
        },confirmButton={ TextButton(onClick={ worktreePicker=false;creatingWorktree=true },enabled=model.worktreeReason(machine).isBlank()) { Text("New worktree") } },dismissButton={ TextButton(onClick={ worktreePicker=false }) { Text("Close") } })
    }
    if(creatingWorktree && currentWorktrees!=null) CreateWorktreeDialog(model,machine,currentWorktrees,{ path ->
        folder=path;browsedFolder=path;creatingWorktree=false;model.browseLaunchDirectory(machine,path)
    }) { if(!model.worktreeSubmitting) creatingWorktree=false }
    if(browser) LaunchFolderDialog(model,machine,{ selected -> folder=selected;browsedFolder=selected;browser=false }) { browser=false }
}

@Composable private fun LaunchSelector(label:String?,value:String,enabled:Boolean,choices:List<Pair<String,String>>,selected:String,providerIcons:Boolean=false,onChoose:(String)->Unit) {
    var expanded by remember { mutableStateOf(false) }
    Column {
        if(label!=null) Text(label,style=MaterialTheme.typography.labelMedium)
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

@Composable private fun CreateWorktreeDialog(model:ZerusViewModel,machine:MachineKey,evidence:JSONObject,onCreated:(String)->Unit,onDismiss:()->Unit) {
    var branch by remember(machine,evidence.string("common_dir")) { mutableStateOf(LaunchPresentation.generatedName()) }
    var base by remember(machine,evidence.string("common_dir")) { mutableStateOf("HEAD") }
    var destination by remember(machine,evidence.string("common_dir")) { mutableStateOf(WorktreePresentation.destination(evidence,branch)) }
    val problem=WorktreePresentation.createError(branch,base,destination)
    AlertDialog(onDismissRequest=onDismiss,title={ Text("Create worktree") },text={
        Column(Modifier.heightIn(max=420.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Text("Creates a new branch and folder on this machine. Creating a session is a separate step.",style=MaterialTheme.typography.bodySmall)
            SelectionContainer { Text(evidence.string("path"),style=MaterialTheme.typography.bodySmall) }
            OutlinedTextField(branch,{ branch=it },Modifier.fillMaxWidth(),label={ Text("New branch") },singleLine=true,readOnly=model.worktreeSubmitting)
            OutlinedTextField(base,{ base=it },Modifier.fillMaxWidth(),label={ Text("Starting revision") },singleLine=true,readOnly=model.worktreeSubmitting)
            OutlinedTextField(destination,{ destination=it },Modifier.fillMaxWidth(),label={ Text("New folder path") },singleLine=true,readOnly=model.worktreeSubmitting)
            if(problem.isNotBlank()) Text(problem,style=MaterialTheme.typography.bodySmall)
            if(model.launchError.isNotBlank()) Text(model.launchError,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.secondary)
            if(model.worktreeSubmitting) LinearProgressIndicator(Modifier.fillMaxWidth())
        }
    },confirmButton={ TextButton(onClick={ model.createLaunchWorktree(machine,evidence,branch,base,destination,onCreated) },enabled=!model.worktreeSubmitting && problem.isBlank() && model.worktreeReason(machine).isBlank()) { Text(if(model.worktreeSubmitting) "Creating…" else "Create worktree") } },dismissButton={ TextButton(onClick=onDismiss,enabled=!model.worktreeSubmitting) { Text("Cancel") } })
}
