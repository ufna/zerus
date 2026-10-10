package app.zerus.mobile
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
class CreationAnswerSafetyTest {
    private val target=Target("computer","session","run","conversation","workspace")
    private val draft=Draft(target,"answer",status="submitting",requestId="request",questionId="question",questionHash="hash",answers="[]")
    private fun receipt()=JSONObject().put("request_id","request").put("state","completed").put("result",JSONObject().put("status","submitted").put("request_id","request").put("question_hash","hash"))
    @Test fun receiptLocksBeforeStaleInspectionAndSurvivesRestart() {
        val restored=MessageCodec.state(MessageCodec.stateJson(QuestionTracking.settle(MessageState(drafts=listOf(draft)),draft,receipt())))
        assertTrue(QuestionTracking.locked(restored.drafts.single()))
        val stale=Question("question","hash",emptyList(),true,true,true,"",source="codex_async",run="run",conversation="conversation")
        assertEquals(restored,QuestionTracking.reconcile(restored,target,listOf(stale)))
        assertEquals(1,QuestionTracking.reconcile(restored,target.copy(run="other"),emptyList()).drafts.size)
        assertEquals(restored,QuestionTracking.reconcile(restored,target,emptyList()))
    }
    @Test fun foreignReceiptAndDifferentAttemptCannotLockDraft() {
        assertEquals("uncertain",QuestionTracking.settle(MessageState(drafts=listOf(draft)),draft,receipt().put("request_id","foreign")).drafts.single().status)
        val wrong=receipt().also { it.getJSONObject("result").put("question_hash","new") }
        assertEquals("uncertain",QuestionTracking.settle(MessageState(drafts=listOf(draft)),draft,wrong).drafts.single().status)
        val newer=draft.copy(requestId="new")
        assertEquals(newer,QuestionTracking.settle(MessageState(drafts=listOf(newer)),draft,receipt()).drafts.single())
    }
    @Test fun interruptionCleanupAndUncertainRecovery() {
        val interrupt=draft.copy(questionId="__interrupt")
        assertTrue(QuestionTracking.settle(MessageState(drafts=listOf(interrupt)),interrupt,receipt()).drafts.isEmpty())
        assertEquals("uncertain",MessageCodec.state(MessageCodec.stateJson(MessageState(drafts=listOf(draft)))).drafts.single().status)
    }
    private fun catalog()=JSONObject().put("state","ok").put("stale",false).put("path","/repo").put("common_dir","/repo/.git").put("worktrees",JSONArray().put(JSONObject().put("path","/repo").put("kind","main").put("available",true)).put(JSONObject().put("path","/linked").put("kind","linked").put("available",true)))
    @Test fun exactRelatedWorktreeRejectsNestedRepoStaleAndForeignAnchor() {
        val project=LaunchProject("project","Project","",listOf(LaunchProjectFolder("anchor","/repo","Repo")))
        assertNotNull(WorktreePresentation.anchor(catalog(),project,"/linked"))
        assertNull(WorktreePresentation.anchor(catalog(),project,"/repo/unrelated"))
        assertNull(WorktreePresentation.anchor(catalog().put("stale",true),project,"/linked"))
        assertNull(WorktreePresentation.anchor(catalog(),project.copy(folders=listOf(LaunchProjectFolder("other","/clone","Clone"))),"/linked"))
        assertTrue(WorktreePresentation.entries(catalog().put("state","not_repo")).isEmpty())
    }
    @Test fun generatedNamesValidAndDistinct() {
        val first=LaunchPresentation.generatedName();assertTrue(first.matches(Regex("work-[a-f0-9]{8}")))
        assertNotEquals(first,LaunchPresentation.generatedName());assertEquals("",LaunchPresentation.nameError("edited name"))
    }
    @Test fun worktreeReceiptCarriesSeparateCanonicalResultPathAcrossRestart() {
        val args=JSONObject().put("path","/repo").put("common_dir","/repo/.git").put("destination","/alias/linked").put("branch","feature").put("base","HEAD")
        val action=SessionAction("request",target.copy(session="",run="",conversation=""),"worktree_create",args.toString())
        val native=JSONObject().put("request_id","request").put("status","created").put("path","/canonical/linked").put("common_dir","/repo/.git").put("branch","feature")
        val completed=SessionActionPolicies.result(action,JSONObject().put("request_id","request").put("state","completed").put("result",native))
        assertEquals("completed",completed.status);assertEquals("/canonical/linked",completed.resultPath);assertEquals("",completed.pendingId)
        assertEquals(completed,MessageCodec.state(MessageCodec.stateJson(MessageState(sessionActions=listOf(completed)))).sessionActions.single())
        native.put("request_id","foreign")
        assertEquals("uncertain",SessionActionPolicies.result(action,JSONObject().put("request_id","request").put("state","completed").put("result",native)).status)
    }

}
