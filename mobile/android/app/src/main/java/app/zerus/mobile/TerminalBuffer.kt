package app.zerus.mobile

import org.json.JSONObject
import java.util.UUID

/** Terminal input is separate from ordinary messages and immutable after enqueue. */
data class TerminalBuffer(val target:Target,val binding:String,val text:String="",val revision:String=UUID.randomUUID().toString(),val generation:String=UUID.randomUUID().toString()) {
    val key get()=target.key + ":terminal:" + binding
}
object TerminalBuffers {
    fun encode(buffer:TerminalBuffer)=MessageCodec.targetJson(buffer.target).put("binding",buffer.binding).put("text",buffer.text).put("revision",buffer.revision).put("generation",buffer.generation)
    fun decode(raw:JSONObject)=TerminalBuffer(MessageCodec.target(raw),raw.getString("binding"),raw.getString("text"),raw.getString("revision"),raw.string("generation").ifBlank { UUID.randomUUID().toString() })
    fun edit(state:MessageState,editor:TerminalBuffer):MessageState {
        val current=state.terminalBuffers.find { it.key == editor.key }
        val next=current?.copy(text=editor.text,revision=editor.revision) ?: editor
        return state.copy(terminalBuffers=state.terminalBuffers.filterNot { it.key == editor.key } + next)
    }
    fun enqueue(state:MessageState,buffer:TerminalBuffer,action:SessionAction):MessageState {
        require(state.terminalBuffers.find { it.key == buffer.key }?.revision.let { it == null || it == buffer.revision }) { "The terminal buffer changed." }
        require(org.json.JSONObject(action.arguments).getString("text") == buffer.text)
        val cleared=buffer.copy(text="",revision=UUID.randomUUID().toString(),generation=UUID.randomUUID().toString())
        return state.action(action).copy(terminalBuffers=state.terminalBuffers.filterNot { it.key == buffer.key } + cleared)
    }
    fun acknowledge(current:MessageState,written:MessageState,buffer:TerminalBuffer,action:SessionAction):MessageState {
        val editor=current.terminalBuffers.find { it.key == buffer.key }
        val cleared=written.terminalBuffers.first { it.key == buffer.key }
        val next=when { editor == null || editor.revision == buffer.revision -> cleared
            editor.generation != buffer.generation -> editor
            else -> cleared.copy(text=editor.text,revision=editor.revision) }
        return current.action(written.sessionActions.first { it.requestId == action.requestId }).copy(terminalBuffers=current.terminalBuffers.filterNot { it.key == buffer.key } + next)
    }
}
