package app.zerus.mobile

import android.icu.lang.UCharacter
import org.json.JSONArray

/** Connection IDs delimit paired workspaces; labels, endpoints and profile names are presentation. */
data class AccountGroupKey(val connectionId:String,val provider:String,val kind:String,val identity:String,val scope:String,val computerId:String="",val profileId:String="") {
    val key:String get()=JSONArray(listOf(connectionId,provider,kind,identity,scope,computerId,profileId)).toString()
}
data class AccountMember(val catalog:AccountCatalog,val account:ReportedAccount) {
    val selection get()=AccountSelection(catalog.key,account.provider,account.id)
}
data class GroupedAccount(val key:AccountGroupKey,val label:String,val members:List<AccountMember>,val snapshot:AccountMember)

/** Mirrors tray/src/AccountCatalog.h: provider identity owns quotas; profiles remain exact machine bindings. */
object AccountGrouping {
    private fun key(member:AccountMember,fold:(String)->String,emailOnly:Boolean=false):AccountGroupKey {
        val a=member.account;val i=a.identity;val email=fold(i.email.trim())
        if(a.status=="signed_out" || (email.isEmpty() && i.accountId.isEmpty()))
            return AccountGroupKey(member.catalog.key.connectionId,a.provider,"profile","","",member.catalog.key.computerId,a.id)
        val byId=!emailOnly && i.accountId.isNotEmpty()
        return AccountGroupKey(member.catalog.key.connectionId,a.provider,if(byId) "id" else "email",if(byId) i.accountId else email,fold(i.organization.trim()))
    }
    private fun named(label:String)=label.isNotEmpty() && label!="Default account" && label!="Native DeepSeek account"
    private fun quality(member:AccountMember)=if(member.account.status in setOf("ok","configured")) { if(member.catalog.online) 3 else 2 } else 1
    internal fun group(catalogs:List<AccountCatalog>,fold:(String)->String={ UCharacter.foldCase(it,true) }):List<GroupedAccount> {
        val members=catalogs.flatMap { c -> c.accounts.map { AccountMember(c,it) } }
        val aliases=mutableMapOf<AccountGroupKey,MutableSet<AccountGroupKey>>()
        members.filter { it.account.status!="signed_out" && it.account.identity.accountId.isNotEmpty() && it.account.identity.email.isNotEmpty() }.forEach {
            aliases.getOrPut(key(it,fold,true)) { mutableSetOf() }.add(key(it,fold))
        }
        return members.groupBy { member -> val initial=key(member,fold);aliases[initial]?.singleOrNull() ?: initial }.map { (id,rows) ->
            val best=rows.reduce { previous,next ->
                if(quality(next)>quality(previous) || quality(next)==quality(previous) && (next.account.checkedAt ?: 0.0)>(previous.account.checkedAt ?: 0.0)) next else previous
            }
            val label=rows.firstOrNull { named(it.account.label) }?.account?.label ?: best.account.identity.email.ifBlank { rows.first().account.label }
            GroupedAccount(id,label,rows,best)
        }
    }
    /** Recompute against current membership, anchored to the exact profile originally opened. */
    fun selected(groups:List<GroupedAccount>,selection:AccountSelection)=groups.find { group -> group.members.any { it.selection==selection } }
}
