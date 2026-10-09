package app.zerus.mobile

import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class SelectedSessionPresentationTest {
    private val target = Target("computer","codex/example/mobile","run","conversation","workspace")
    private val selected = Session(target,"Mobile","codex","","example","",0,JSONObject().put("name",target.session))
    private fun raw() = JSONObject().put("name",target.session).put("run_id",target.run).put("conversation_id",target.conversation)
        .put("activity","idle").put("tracked",true).put("state","running")
    @Test fun exactCachedInspectionPreservesReadyInsteadOfInventingUntracked() {
        assertEquals("Ready",SelectedSessionPresentation.status(selected,raw()))
        assertEquals(DesktopBadgeKind.Neutral,DesktopSessionPalette.badge(SelectedSessionPresentation.session(selected,raw()),true).kind)
    }
    @Test fun anotherRunOrConversationCannotSupplyStatus() {
        assertEquals("Status not reported",SelectedSessionPresentation.status(selected,raw().put("run_id","other")))
        assertEquals("Status not reported",SelectedSessionPresentation.status(selected,raw().put("conversation_id","other")))
    }
    @Test fun knownUntrackedAndWorkingStillUseNativeSemantics() {
        assertEquals("Not tracked",SelectedSessionPresentation.status(selected,raw().put("tracked",false).put("activity","")))
        assertEquals("Working",SelectedSessionPresentation.status(selected,raw().put("activity","busy")))
    }
    @Test fun archivedStatusNeverComesFromLiveInspection() {
        val archive = selected.copy(target = target.copy(archiveId="archive"))
        assertSame(archive,SelectedSessionPresentation.session(archive,raw()))
    }
}
