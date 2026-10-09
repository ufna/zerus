package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class ComposerIdentityTest {
    @Test fun parentAndDistinctChildrenNeverShareEditorLane() {
        val parent = Target("computer","session","run","conversation","workspace")
        val child = parent.copy(agentId="child",parentConversation=parent.conversation,conversation="conversation/child")
        assertNotEquals(ComposerIdentity.lane(parent), ComposerIdentity.lane(child))
        assertNotEquals(ComposerIdentity.lane(child), ComposerIdentity.lane(child.copy(agentId="second",conversation="conversation/second")))
        assertNotEquals(ComposerIdentity.lane(child), ComposerIdentity.lane(child.copy(parentConversation="new-parent")))
        assertEquals(ComposerIdentity.lane(parent), ComposerIdentity.lane(parent.copy()))
    }
    @Test fun initialParentConversationPromotionPreservesEditor() {
        val initial = Target("computer","session","run","","workspace")
        assertEquals(ComposerIdentity.lane(initial), ComposerIdentity.lane(initial.copy(conversation="created")))
        assertNotEquals(ComposerIdentity.lane(initial), ComposerIdentity.lane(initial.copy(run="reused")))
        assertNotEquals(ComposerIdentity.lane(initial), ComposerIdentity.lane(initial.copy(connectionId="other-workspace")))
    }
}
