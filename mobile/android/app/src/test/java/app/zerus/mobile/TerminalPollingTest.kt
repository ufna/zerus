package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class TerminalPollingTest {
    @Test fun heldKeyConfirmationKeepsReadonlyFreshnessWithoutAdoptingInputBinding() {
        assertTrue(TerminalPolling.readAllowed(true,false,false,false,true))
        assertFalse(TerminalPolling.readAllowed(true,false,false,false,false))
        assertFalse(TerminalPolling.readAllowed(false,false,false,false,true))
        assertFalse(TerminalPolling.readAllowed(true,false,true,false,true))
        assertFalse(TerminalPolling.readAllowed(true,false,false,true,true))
    }
    @Test fun idleReadsBackOffAndNewOutputRestoresResponsiveness() {
        var policy = TerminalPolling()
        assertEquals(250L,policy.delayMillis)
        policy = policy.observed(false);assertEquals(500L,policy.delayMillis)
        policy = policy.observed(false);assertEquals(1000L,policy.delayMillis)
        repeat(50) { policy = policy.observed(false) }
        assertEquals(2000L,policy.delayMillis)
        assertEquals(250L,policy.observed(true).delayMillis)
    }
}
