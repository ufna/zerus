package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test
import kotlinx.coroutines.*

class MachineNamesTest {
    @Test fun orderedNameSaveKeepsTypingAndFailedSaveDoesNotChangeLabel() = runBlocking {
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.Default)
        try {
            val target=Target("node","session","run","conversation","workspace")
            val draft=Draft(target,"Before")
            val entered=CompletableDeferred<Unit>();val release=CompletableDeferred<Unit>()
            val actor=OrderedStatePersistence(MessageState(listOf(draft)),scope,{entered.complete(Unit);release.await()},Dispatchers.Unconfined)
            val key=MachineKey("workspace","node")
            val saving=async { actor.durable({MachineNames.set(it,key,"My label")},{current,_ -> MachineNames.set(current,key,"My label")}) }
            entered.await()
            actor.edit { it.copy(drafts=listOf(draft.copy(text="Typed while saving"))) }
            release.complete(Unit);saving.await();actor.flush()
            assertEquals("Typed while saving",actor.durableState.drafts.single().text)
            assertEquals("My label",actor.durableState.machineAliases.single().name)
            val failed=OrderedStatePersistence(MessageState(),scope,{throw java.io.IOException("synthetic")},Dispatchers.Unconfined)
            assertTrue(runCatching { failed.durable({MachineNames.set(it,key,"Unsaved")}) }.isFailure)
            assertTrue(failed.state.value.machineAliases.isEmpty())
        } finally { scope.cancel() }
    }
    @Test fun offlineIndexPreservesReportedNameForReset() {
        val machine=MachineNames.apply(Machine("workspace","node","Reported",true),listOf(MachineAlias(MachineKey("workspace","node"),"Phone alias")))
        val session=Session(Target("node","session","run","conversation","workspace"),"Title","codex","Idle","Project","",0,org.json.JSONObject())
        val index=ConversationIndex.capture(listOf(session),listOf(machine)).single()
        val loaded=ConversationIndex.decode(ConversationIndex.encode(index))
        assertEquals("Reported",loaded.computerName)
        val offline=MachineNames.apply(Machine("workspace","node",loaded.computerName,false),listOf(MachineAlias(MachineKey("workspace","node"),"Phone alias")))
        assertEquals("Reported",MachineNames.apply(offline,emptyList()).name)
    }
    @Test fun aliasesAreWorkspaceAndNodeScopedAndSurviveRefreshRestart() {
        val key=MachineKey("workspace","node")
        val state=MachineNames.set(MessageState(),key," Studio phone label ")
        val restored=MessageCodec.state(MessageCodec.stateJson(state))
        assertEquals("Studio phone label",MachineNames.apply(Machine("workspace","node","Fresh native name",true),restored.machineAliases).name)
        assertEquals("Native",MachineNames.apply(Machine("other","node","Native",true),restored.machineAliases).name)
        assertEquals("Native",MachineNames.apply(Machine("workspace","other","Native",true),restored.machineAliases).name)
        assertEquals("Fresh native name",MachineNames.apply(Machine("workspace","node","Fresh native name",true),MachineNames.set(restored,key,null).machineAliases).name)
    }
    @Test fun renameCannotChangeDraftFilesRequestOrMachineIdentity() {
        val target=Target("node","session","run","conversation","workspace")
        val draft=Draft(target,"Keep this text")
        val outgoing=OutgoingMessage("receipt",target,"Sent",emptyList(),1.0,"uncertain")
        val state=MachineNames.set(MessageState(listOf(draft),listOf(outgoing)),MachineKey("workspace","node"),"Label")
        assertEquals(draft,state.drafts.single());assertEquals(outgoing,state.outgoing.single())
        val machine=MachineNames.apply(Machine("workspace","node","Reported",false),state.machineAliases)
        assertEquals("node",machine.id);assertEquals("Reported",machine.nativeName)
        assertTrue(MessageCodec.state(org.json.JSONObject()).machineAliases.isEmpty())
    }
    @Test fun namesHaveBoundedLiteralUnicodeAndRejectControls() {
        assertEquals("😀".repeat(80),MachineNames.checked("😀".repeat(80)))
        listOf(" ","x".repeat(81),"bad\nname","bad\u202ename","bad\u0000name").forEach { assertThrows(IllegalArgumentException::class.java) { MachineNames.checked(it) } }
        assertEquals("Studio / desk",MachineNames.checked("Studio / desk"))
    }
}
