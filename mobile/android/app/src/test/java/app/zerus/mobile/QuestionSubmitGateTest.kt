package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class QuestionSubmitGateTest {
    @Test fun hardwareThenImeAndIconCannotStartAnotherAttemptBeforeCompletion() {
        val gate=QuestionSubmitGate()
        assertTrue(gate.claim(true))
        assertFalse(gate.claim(true))
        assertFalse(gate.claim(true))
        gate.finish()
        assertTrue(gate.claim(true))
    }
    @Test fun DisabledActionDoesNotClaimOrPermitUnrelatedSubmittedForm() {
        val gate=QuestionSubmitGate()
        assertFalse(gate.claim(false))
        assertTrue(gate.claim(true))
        gate.finish()
        assertFalse(gate.claim(false)) // Submitted/uncertain draft stays blocked even after its network job finishes.
    }
}
