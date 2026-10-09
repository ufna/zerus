package app.zerus.mobile

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class ConfirmedTargetResolutionTest {
    @Test fun membershipDelayUsesOnlyReadRetriesAndPreservesOriginalDraftUntilProof()=runBlocking {
        val target=Target("computer","codex/project/chat","run","conversation","workspace")
        val original=Draft(target,"unsent text")
        var state=MessageState(drafts=listOf(original));var reads=0;var pauses=0
        val destination=target.copy(session="codex/project/renamed")
        val action=SessionAction("id",target,"rename","{}",status="completed",resultTarget=destination)
        state=state.action(action)
        val proof=ConfirmedTargetResolution.read(active={true},pause={pauses++;assertEquals(original,state.drafts.single())},inspect={
            reads++;if(reads<3) error("Snapshot membership has not caught up")
            destination
        })
        assertEquals(3,reads);assertEquals(2,pauses);assertEquals(original,state.drafts.single())
        state=ActionDraftPolicies.promote(state,action)
        assertEquals(proof,state.drafts.single().target);assertEquals(original.text,state.drafts.single().text)
        assertEquals(1,state.sessionActions.size)
    }
    @Test fun explicitConfirmedRestoreAcceptsEmptyEditorButPreservesOccupiedAndChangedDraft() {
        val from=Target("computer","codex/project/chat","run","conversation","workspace")
        val to=from.copy(session="codex/project/renamed")
        val action=SessionAction("id",from,"rename","{}",status="completed",resultTarget=to)
        val draft=Draft(from,"saved text")
        val state=MessageState(drafts=listOf(draft,Draft(to,"")),sessionActions=listOf(action))
        val restored=ActionDraftPolicies.restore(state,action,draft)
        assertEquals(1,restored.drafts.size);assertEquals(to,restored.drafts.single().target);assertEquals(draft.generation,restored.drafts.single().generation)
        val occupied=state.copy(drafts=listOf(draft,Draft(to,"already typed")))
        assertEquals(occupied,ActionDraftPolicies.restore(occupied,action,draft))
        val changed=state.copy(drafts=listOf(draft.copy(revision="new",text="later edit")))
        assertEquals(changed,ActionDraftPolicies.restore(changed,action,draft))
    }
    @Test fun navigationCancelsFurtherReadsAndDoesNotMoveDraft()=runBlocking {
        var active=true;var reads=0
        try { ConfirmedTargetResolution.read(active={active},pause={active=false},inspect={reads++;error("Offline")});fail("Expected cancellation") }
        catch(_:CancellationException) { assertEquals(1,reads) }
    }
    @Test fun offlineOutcomeRemainsConfirmedAndRecoverableWithoutMutationReplay()=runBlocking {
        var reads=0
        try { ConfirmedTargetResolution.read(active={true},pause={},inspect={reads++;error("Offline")});fail("Expected unavailable result") }
        catch(_:IllegalStateException) { assertEquals(4,reads) }
    }
}
