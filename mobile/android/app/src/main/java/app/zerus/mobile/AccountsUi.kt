package app.zerus.mobile

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

@Composable fun AccountsScreen(model:ZerusViewModel,onMachines:()->Unit) {
    var now by remember { mutableDoubleStateOf(System.currentTimeMillis()/1000.0) }
    LaunchedEffect(Unit) { while(true) { delay(30_000);now=System.currentTimeMillis()/1000.0 } }
    val catalogs=model.displayedAccounts
    LazyColumn(Modifier.fillMaxSize(),contentPadding=PaddingValues(16.dp),verticalArrangement=Arrangement.spacedBy(14.dp)) {
        item { Column(verticalArrangement=Arrangement.spacedBy(6.dp)) {
            Text("Accounts",style=MaterialTheme.typography.headlineSmall,fontWeight=FontWeight.SemiBold)
            Text("Reported account identities and limits from your machines.",style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
            Text("Limits update automatically about every five minutes. Last reported values remain available offline.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(model.demo) Text("PREVIEW / Sample account data",color=MaterialTheme.colorScheme.primary,style=MaterialTheme.typography.labelMedium)
            if(model.accountsError.isNotBlank()) Text(model.accountsError,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.error)
        } }
        if(catalogs.isEmpty()) item { Column(verticalArrangement=Arrangement.spacedBy(10.dp)) {
            Text(if(model.connections.isEmpty()) "Pair a workspace to view its account profiles." else "No account snapshots available yet. Refresh to check your machines.")
            TextButton(onClick=onMachines) { Text("Open Machines") }
        } }
        catalogs.forEach { catalog ->
            item(key="machine:"+catalog.key.toString()) { Column(verticalArrangement=Arrangement.spacedBy(6.dp)) {
                val connection=model.connections.find { it.id==catalog.key.connectionId }
                Text(connection?.displayName ?: if(model.demo) "Preview workspace" else "Workspace",style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                MachineLabel(model.machineName(catalog.key.connectionId,catalog.key.computerId,catalog.machineName),colorHex=model.machineColor(catalog.key))
                if(!catalog.online) Text("Offline / Last reported data",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                if(!catalog.supported) Text("Update the machine connector to view accounts.",style=MaterialTheme.typography.bodySmall)
                else if(!catalog.available) Text("Account catalog is not available yet. Last reported values are shown when retained.",style=MaterialTheme.typography.bodySmall)
                else if(catalog.accounts.isEmpty()) Text("No account profiles reported on this machine.",style=MaterialTheme.typography.bodySmall)
            } }
            items(catalog.accounts,key={ "account:"+org.json.JSONArray(listOf(catalog.key.connectionId,catalog.key.computerId,it.provider,it.id)).toString() }) { account ->
                AccountCard(catalog,account,now)
            }
        }
    }
}

@Composable private fun AccountCard(catalog:AccountCatalog,account:ReportedAccount,now:Double) {
    val stale=AccountPresentation.stale(catalog,account,now)
    val muted=MaterialTheme.colorScheme.onSurfaceVariant
    Surface(shape=MaterialTheme.shapes.large,color=MaterialTheme.colorScheme.surfaceContainer) {
        Column(Modifier.fillMaxWidth().padding(16.dp),verticalArrangement=Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(10.dp)) {
                ProviderBadge(account.provider,Modifier.size(24.dp))
                Column(Modifier.weight(1f)) {
                    Text(account.label,style=MaterialTheme.typography.titleMedium,fontWeight=FontWeight.SemiBold)
                    Text(DesktopIcons.providerName(account.provider)+(if(account.isDefault) " / Default" else ""),style=MaterialTheme.typography.bodySmall,color=muted)
                }
            }
            SelectionContainer { Column(verticalArrangement=Arrangement.spacedBy(3.dp)) {
                listOf(account.identity.name,account.identity.email,account.identity.organization).filter(String::isNotBlank).distinct().forEach { Text(it,style=MaterialTheme.typography.bodyMedium) }
                if(account.identity.plan.isNotBlank()) Text("Plan: ${account.identity.plan}",style=MaterialTheme.typography.bodyMedium)
                else Text("Plan not reported",style=MaterialTheme.typography.bodySmall,color=muted)
                if(account.identity.authMethod.isNotBlank()) Text("Authentication: ${account.identity.authMethod}",style=MaterialTheme.typography.bodySmall,color=muted)
            } }
            if(!account.installed) Text("Provider not installed on this machine",style=MaterialTheme.typography.bodySmall,color=muted)
            val warning=account.status in setOf("expired","signed_out","credentials_locked","desktop_session_unavailable","credentials_unavailable","error") || account.authStatus in setOf("expired","signed_out","locked","credentials_locked")
            Text(AccountPresentation.status(account)+(if(stale) " / Last reported" else ""),style=MaterialTheme.typography.labelMedium,color=if(warning) Color(0xFFF0A35B) else muted)
            if(account.identityCached) Text("Identity retained from the last provider report",style=MaterialTheme.typography.bodySmall,color=muted)
            if(account.refreshing) Text("Provider refresh in progress",style=MaterialTheme.typography.bodySmall,color=muted)
            if(account.refreshError) Text("The last provider refresh failed. Retained values may be out of date.",style=MaterialTheme.typography.bodySmall,color=muted)
            account.windows.forEach { window ->
                val ended=AccountPresentation.ended(window,now)
                val tone=when { stale || ended -> muted; (window.usedPercent ?: 0.0)>=90 -> Color(0xFFF07878);(window.usedPercent ?: 0.0)>=70 -> Color(0xFFF0A35B);else -> Color(0xFF72CDB2) }
                Column(verticalArrangement=Arrangement.spacedBy(4.dp)) {
                    Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                        Text(AccountPresentation.period(window),Modifier.weight(1f),style=MaterialTheme.typography.bodyMedium)
                        Text(AccountPresentation.percent(window.usedPercent),style=MaterialTheme.typography.labelLarge,color=tone)
                    }
                    window.usedPercent?.let { LinearProgressIndicator(progress={ (it/100.0).coerceIn(0.0,1.0).toFloat() },modifier=Modifier.fillMaxWidth(),color=tone,trackColor=MaterialTheme.colorScheme.surfaceVariant) }
                    Text(if(ended) "Window ended / Refresh needed" else AccountPresentation.date(window.resetsAt)?.let { "Resets $it" } ?: "Reset time not reported",style=MaterialTheme.typography.bodySmall,color=muted)
                }
            }
            if(account.windows.isEmpty()) Text("Usage limits not reported",style=MaterialTheme.typography.bodySmall,color=muted)
            if(account.unlimited==true) Text("Credits: Unlimited",style=MaterialTheme.typography.bodyMedium)
            else account.creditBalance?.let { Text("Credit balance: ${it}",style=MaterialTheme.typography.bodyMedium) }
            account.balances.forEach { balance -> Text(listOf(balance.kind.ifBlank { "Balance" },balance.balance ?: "Unknown",balance.currency).filter(String::isNotBlank).joinToString(" / "),style=MaterialTheme.typography.bodyMedium) }
            val checked=account.checkedAt?.takeIf { it<=now+60 }?.let(AccountPresentation::date)
            Text(if(checked==null) "Provider update time not reported" else "Provider updated $checked",style=MaterialTheme.typography.bodySmall,color=muted)
        }
    }
}
