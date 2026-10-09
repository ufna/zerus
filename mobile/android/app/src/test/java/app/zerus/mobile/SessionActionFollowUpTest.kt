package app.zerus.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SessionActionFollowUpTest {
    private val target=Target("computer","codex/example/session","run","conversation","workspace")
    private val archive=target.copy(archiveId="10000000-0000-4000-8000-000000000001")
    private fun action(operation:String,original:Target=target,destination:Target?=null)=SessionAction("request",original,operation,"{}",status="completed",resultTarget=destination)
    @Test fun removedBindingsReturnToSessionsOnlyForStillActiveConfirmation() {
        for(operation in listOf("archive","forget")) {
            val action=action(operation)
            assertEquals(SessionActionFollowUp.Kind.Sessions,SessionActionFollowUp.kind(action,target,true))
            assertEquals(SessionActionFollowUp.Kind.None,SessionActionFollowUp.kind(action,target,false))
            assertEquals(SessionActionFollowUp.Kind.None,SessionActionFollowUp.kind(action,target.copy(session="other"),true))
            assertEquals(SessionActionFollowUp.Kind.Refresh,SessionActionFollowUp.kind(action.copy(status="uncertain"),target,true))
        }
    }
    @Test fun restoreOpensVerifiedLiveResultWithoutPromotingArchiveDraft() {
        val action=action("restore",archive,target)
        assertEquals(SessionActionFollowUp.Kind.OpenResult,SessionActionFollowUp.kind(action,archive,true))
        val draft=Draft(archive,"saved archive composition")
        val state=MessageState(drafts=listOf(draft),sessionActions=listOf(action))
        assertEquals(state,ActionDraftPolicies.promote(state,action))
        assertFalse(SessionActionFollowUp.openable(action.copy(resultTarget=target.copy(connectionId="foreign"))))
        assertFalse(SessionActionFollowUp.openable(action.copy(resultTarget=archive.copy(run="other"))))
    }
    @Test fun archiveReceiptMustConfirmTheExecutedImmutableVersion() {
        val action=action("restore",archive,target).copy(status="sending")
        val native=JSONObject().put("request_id","request").put("name",archive.session).put("run_id",archive.run)
            .put("conversation_id",archive.conversation).put("status","completed")
            .put("result_target",JSONObject().put("name",target.session).put("run_id",target.run).put("conversation_id",target.conversation))
        val receipt=JSONObject().put("request_id","request").put("state","completed").put("result",native)
        assertEquals("uncertain",SessionActionPolicies.result(action,receipt).status)
        native.put("archive_id","20000000-0000-4000-8000-000000000001")
        assertEquals("uncertain",SessionActionPolicies.result(action,receipt).status)
        native.put("archive_id",archive.archiveId)
        assertEquals("completed",SessionActionPolicies.result(action,receipt).status)
        assertEquals("uncertain",SessionActionPolicies.result(action.copy(target=target),receipt).status)
    }
}
