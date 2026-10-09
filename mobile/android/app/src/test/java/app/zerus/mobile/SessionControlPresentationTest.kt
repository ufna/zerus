package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SessionControlPresentationTest {
    private val target = Target("computer","provider/project/tag","run","conversation","workspace")
    @Test fun nativeAllowedActionsAreThePresentationAuthority() {
        val raw = JSONObject().put("allowed_actions",JSONArray(listOf("pause","rename")))
            .put("action_reasons",JSONObject().put("resume","Already running"))
        assertEquals("",SessionControlPresentation.reason(target,"pause",raw))
        assertEquals("Already running",SessionControlPresentation.reason(target,"resume",raw))
        assertTrue(SessionControlPresentation.reason(target,"fork",raw).isNotBlank())
    }
    @Test fun missingNativeEligibilityNeverGuessesFromIdleOrProvider() {
        val raw = JSONObject().put("activity","idle").put("state","running").put("resumable",true)
        assertTrue(SessionControlPresentation.reason(target,"pause",raw).isNotBlank())
    }
    @Test fun childCannotExposeParentControls() {
        val raw = JSONObject().put("allowed_actions",JSONArray(listOf("pause")))
        assertTrue(SessionControlPresentation.reason(target.copy(agentId="child",parentConversation="conversation"),"pause",raw).contains("parent"))
    }
}
