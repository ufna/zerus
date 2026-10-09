package app.zerus.mobile

import java.util.Locale
import org.junit.Assert.*
import org.junit.Test

class AccountGroupingTest {
    // ASCII fixtures exercise grouping semantics; production uses Android ICU full Unicode folding.
    private val asciiFixtureFold:(String)->String={ it.lowercase(Locale.ROOT) }
    private fun account(profile:String="native",id:String="identity",email:String="one@example.com",scope:String=" Team ",provider:String="codex",status:String="ok",checked:Double?=10.0,label:String="Default account")=
        ReportedAccount(profile,provider,label,true,true,false,status,"signed_in",AccountIdentity(email=email,organization=scope,accountId=id),listOf(AccountWindow("hour","Session",42.0,300.0,1000.0)),null,null,emptyList(),checked,false,false,false,false)
    private fun catalog(machine:String="a",accounts:List<ReportedAccount> = listOf(account()),workspace:String="w",online:Boolean=true)=
        AccountCatalog(MachineKey(workspace,machine),"Same machine label",true,true,!online,online,20.0,accounts)
    private fun group(vararg catalogs:AccountCatalog)=AccountGrouping.group(catalogs.toList(),asciiFixtureFold)

    @Test fun reportedIdentityJoinsDifferentProfilesAndMachineLabelsAreNotIdentity() {
        val groups=group(catalog(accounts=listOf(account("one",label="Personal"))),catalog("b",listOf(account("two",scope="team",email="other@example.com"))))
        val joined=groups.single();assertEquals(2,joined.members.size);assertEquals("Personal",joined.label)
        assertEquals(listOf("a","b"),joined.members.map { it.catalog.key.computerId })
        assertEquals(1,joined.snapshot.account.windows.size);assertEquals(42.0,joined.snapshot.account.windows.single().usedPercent!!,0.0)
        assertEquals(2,group(catalog(accounts=listOf(account(id="first"),account("other",id="second")))).size)
    }
    @Test fun emailAliasJoinsOnlyOneDistinctIdInMatchingScope() {
        val a=catalog(accounts=listOf(account(email=" ONE@example.com ")))
        val b=catalog("b",listOf(account("email-only",id="",email="one@example.COM",scope="team")))
        assertEquals(2,group(a,b).single().members.size)
        val repeated=catalog("c",listOf(account("repeat")))
        assertEquals(3,group(a,b,repeated).single().members.size)
        val ambiguous=catalog("c",listOf(account("other",id="different")))
        assertEquals(3,group(a,b,ambiguous).size)
    }
    @Test fun providerOrganizationAndWorkspaceNeverCrossMerge() {
        val base=catalog()
        assertEquals(4,group(base,catalog("b",listOf(account(provider="claude"))),catalog("c",listOf(account(scope="other"))),catalog("a",workspace="other-workspace")).size)
        assertEquals(2,group(base,catalog("b",listOf(account(id="IDENTITY")))).size)
    }
    @Test fun signedOutAndUnidentifiedRemainExactProfiles() {
        val signed=account(status="signed_out")
        val unknown=account("unknown",id="",email="",label="Personal")
        val groups=group(catalog(accounts=listOf(signed,unknown)),catalog("b",listOf(signed,unknown)))
        assertEquals(4,groups.size);assertTrue(groups.all { it.key.kind=="profile" })
        assertEquals(3,group(catalog(accounts=listOf(account(),signed.copy(id="signed"))),catalog("b",listOf(signed))).size)
    }
    @Test fun snapshotQualityWinsBeforeTimeAndNeverCombinesQuotas() {
        val online=catalog(accounts=listOf(account(checked=10.0)))
        val offline=catalog("b",listOf(account(checked=100.0)),online=false)
        val failed=catalog("c",listOf(account(status="error",checked=1000.0)))
        val g=group(offline,failed,online).single()
        assertEquals("a",g.snapshot.catalog.key.computerId);assertEquals(3,g.members.size)
        assertFalse(g.members.first().catalog.online);assertEquals("error",g.members[1].account.status)
        assertEquals("b",group(failed,offline).single().snapshot.catalog.key.computerId)
        assertEquals("d",group(online,catalog("d",listOf(account(checked=11.0)))).single().snapshot.catalog.key.computerId)
        assertEquals("a",group(online,catalog("d",listOf(account(checked=10.0)))).single().snapshot.catalog.key.computerId)
    }
    @Test fun selectionTracksCurrentExactMemberWhenAliasesChangeOrDisappear() {
        val a=catalog();val b=catalog("b",listOf(account("email",id="")))
        val anchor=AccountSelection(b.key,"codex","email")
        assertEquals(2,AccountGrouping.selected(group(a,b),anchor)!!.members.size)
        val conflicting=catalog("c",listOf(account(id="other")))
        val split=AccountGrouping.selected(group(a,b,conflicting),anchor)!!
        assertEquals("email",split.key.kind);assertEquals(1,split.members.size)
        val refreshed=b.copy(accounts=listOf(b.accounts.single().copy(status="signed_out")))
        assertEquals("profile",AccountGrouping.selected(group(a,refreshed),anchor)!!.key.kind)
        assertNull(AccountGrouping.selected(group(a),anchor))
        assertNull(AccountGrouping.selected(group(a,catalog("b",workspace="other-workspace",accounts=b.accounts)),anchor))
    }
    @Test fun fallbackLabelUsesSelectedSnapshotEmailAndNamedLabelsWin() {
        val a=catalog(accounts=listOf(account(label="Native DeepSeek account",checked=1.0)))
        val b=catalog("b",listOf(account(label="Default account",email="best@example.com",checked=2.0)))
        assertEquals("best@example.com",group(a,b).single().label)
        assertEquals("Custom profile",group(a,b.copy(accounts=listOf(b.accounts.single().copy(label="Custom profile")))).single().label)
    }
    @Test fun builtinPreviewKeepsAmbiguousProfilesSeparateAndOfflineMemberVisible() {
        val catalogs=AccountSnapshots.demo(1000.0)
        assertEquals(setOf("demo","demo-laptop","demo-build"),catalogs.map { it.key.computerId }.toSet())
        val groups=AccountGrouping.group(catalogs,asciiFixtureFold)
        assertEquals(5,groups.size)
        val personal=groups.single { it.snapshot.account.identity.accountId=="example-personal" }
        assertEquals(3,personal.members.size);assertTrue(personal.snapshot.catalog.online)
        assertTrue(personal.members.any { !it.catalog.online && it.account.refreshError })
        assertEquals(3,groups.count { it.snapshot.account.identity.email=="shared@example.com" })
    }

}
