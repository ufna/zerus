package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class LoadingGateTest {
    @Test fun automaticReadsRemainInvisibleForFiveMonotonicSeconds() {
        var time = 42_000L
        val gate = LoadingGate({ time })
        val operation = gate.begin("session-a").operation
        assertFalse(gate.visible())
        time += 4_999
        assertFalse(gate.visible())
        assertEquals(1L, gate.remaining(operation.id))
        time++
        assertTrue(gate.visible())
        assertEquals(0L, gate.remaining(operation.id))
    }
    @Test fun repeatedPollKeepsDeadlineAndDoesNotCreateSecondRead() {
        var time = 0L
        val gate = LoadingGate({ time })
        val first = gate.begin("catalog")
        time = 3_000
        val second = gate.begin("catalog")
        assertFalse(second.newRequest)
        assertEquals(first.operation.id, second.operation.id)
        assertEquals(0L, second.operation.startedAt)
        time = 5_000
        assertTrue(gate.visible())
    }
    @Test fun explicitRefreshPromotesOngoingReadWithoutRestartOrDuplicate() {
        var time = 0L
        val gate = LoadingGate({ time })
        val first = gate.begin("session")
        time = 100
        val manual = gate.begin("session", explicit = true)
        assertFalse(manual.newRequest)
        assertEquals(first.operation.id, manual.operation.id)
        assertEquals(first.operation.startedAt, manual.operation.startedAt)
        assertTrue(gate.visible())
        assertTrue(gate.begin("session").operation.explicit)
    }
    @Test fun oldSessionCompletionCannotClearNewSessionLoadingOrResultOwnership() {
        var time = 0L
        val gate = LoadingGate({ time })
        val old = gate.begin("old").operation
        time = 4_999
        val next = gate.begin("new").operation
        assertFalse(gate.owns(old.id))
        assertFalse(gate.finish(old.id))
        assertTrue(gate.owns(next.id))
        time = 5_000
        assertFalse(gate.visible())
        time = 9_999
        assertTrue(gate.visible())
    }
    @Test fun reopeningSameSessionAfterNavigationUsesNewIdentity() {
        var time = 0L
        val gate = LoadingGate({ time })
        val old = gate.begin("session").operation
        gate.cancel()
        time = 1_000
        val reopened = gate.begin("session").operation
        assertNotEquals(old.id, reopened.id)
        assertFalse(gate.finish(old.id))
        assertTrue(gate.finish(reopened.id))
        assertFalse(gate.visible())
    }
    @Test fun fastCompletionNeverLeavesProgressAndNextOperationGetsFullDelay() {
        var time = 0L
        val gate = LoadingGate({ time })
        val first = gate.begin("catalog").operation
        time = 200
        assertTrue(gate.finish(first.id))
        assertFalse(gate.visible())
        val next = gate.begin("catalog").operation
        assertEquals(5_000L, gate.remaining(next.id))
    }
}
