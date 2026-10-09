package app.zerus.mobile

import kotlinx.coroutines.*
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class TerminalBufferTest {
    private val target=Target("computer","session","run","conversation","workspace")
    private val buffer=TerminalBuffer(target,"binding","literal input")
    private val action=SessionAction("request",target,"terminal_input",JSONObject().put("terminal_binding_id","binding").put("text",buffer.text).put("enter",true).toString())
    @Test fun acceptedInputWritePreservesNextTerminalTypingAndOrdinaryDraft()=runBlocking {
        val chat=Draft(target,"Private chat draft")
        val initial=MessageState(drafts=listOf(chat),terminalBuffers=listOf(buffer))
        val entered=CompletableDeferred<Unit>();val release=CompletableDeferred<Unit>()
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.Default)
        val writer=OrderedStatePersistence(initial,scope,{ entered.complete(Unit);release.await() },uiDispatcher=Dispatchers.Unconfined)
        val pending=async { writer.durable({ TerminalBuffers.enqueue(it,buffer,action) },{ current,written -> TerminalBuffers.acknowledge(current,written,buffer,action) }) }
        entered.await()
        writer.edit { TerminalBuffers.edit(it,buffer.copy(text="Next terminal text",revision="new")) }
        release.complete(Unit)
        val written=pending.await();writer.flush()
        assertEquals(chat,writer.state.value.drafts.single())
        assertEquals("Next terminal text",writer.state.value.terminalBuffers.single().text)
        assertEquals(written.terminalBuffers.single().generation,writer.state.value.terminalBuffers.single().generation)
        assertEquals("literal input",JSONObject(writer.state.value.sessionActions.single().arguments).getString("text"))
        scope.cancel()
    }
    @Test fun failureBeforeDurableEnqueueLeavesBothComposersUntouched()=runBlocking {
        val initial=MessageState(drafts=listOf(Draft(target,"Chat")),terminalBuffers=listOf(buffer))
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.Default)
        val writer=OrderedStatePersistence(initial,scope,{ error("Synthetic storage failure") },uiDispatcher=Dispatchers.Unconfined)
        try { writer.durable({ TerminalBuffers.enqueue(it,buffer,action) });fail("Expected storage failure") } catch(_:IllegalStateException) { }
        assertEquals(initial,writer.state.value);scope.cancel()
    }
    @Test fun restartPreservesOriginalInputRequestAndIndependentBindings() {
        val other=buffer.copy(binding="other-binding",text="Other buffer")
        val state=TerminalBuffers.enqueue(MessageState(terminalBuffers=listOf(buffer,other)),buffer,action)
        val restored=MessageCodec.state(MessageCodec.stateJson(state))
        assertEquals("uncertain",restored.sessionActions.single().status)
        assertEquals("request",restored.sessionActions.single().requestId)
        assertEquals("Other buffer",restored.terminalBuffers.first { it.binding == "other-binding" }.text)
        assertEquals("",restored.terminalBuffers.first { it.binding == "binding" }.text)
    }
    @Test fun terminalAcceptsLiteralTextButRejectsUnlistedKeysAndMixedModes() {
        assertEquals(buffer.text,SessionActionPolicies.arguments("terminal_input",JSONObject(action.arguments)).getString("text"))
        assertThrows(IllegalArgumentException::class.java) { SessionActionPolicies.arguments("terminal_input",JSONObject(action.arguments).put("key","C-c")) }
        assertThrows(IllegalArgumentException::class.java) { SessionActionPolicies.arguments("terminal_input",JSONObject().put("terminal_binding_id","binding").put("key","arbitrary-command")) }
    }
}
