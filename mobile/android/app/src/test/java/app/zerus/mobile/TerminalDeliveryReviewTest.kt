package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test
import org.json.JSONObject

/**
 * Contract for the live parent Terminal's two recovery actions (zerus-4ayxb):
 * Check delivery reuses [ZerusViewModel.checkOutgoing]; Dismiss reuses
 * [ZerusViewModel.dismissOutgoing] through [MessageChanges.dismissOutgoing].
 * Neither action may send, replay, press Enter, clear native input or change
 * the native composer.
 */
class TerminalDeliveryReviewTest {
    private val target = Target("computer", "session", "run", "conversation", "workspace")
    private val request = "11111111-1111-4111-8111-111111111111"
    private val file = Attachment("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "fixture.txt", "text/plain", 7, "a".repeat(64))
    private val original = OutgoingMessage(request, target, "Original saved message", listOf(file), 100.0,
        status = "uncertain", error = "Delivery is unconfirmed.")
    private val editor = Draft(target, "Next distinct composition")

    private fun vectors() = JSONObject(java.io.File("../../../services/mobile/vectors/send_outcome/outcomes.json").readText())
        .getJSONArray("vectors").objects()

    @Test fun dismissOnlyUnblocksTheWarningAndNeverSendsOrTouchesTheEditor() {
        val buffer = TerminalBuffer(target, "binding", "buffered input")
        val state = MessageState(drafts = listOf(editor), outgoing = listOf(original), terminalBuffers = listOf(buffer))
        val dismissed = MessageChanges.dismissOutgoing(state, original)
        val kept = dismissed.outgoing.single()
        // no new request, no status change, payload and identity stay frozen
        assertEquals(1, dismissed.outgoing.size)
        assertEquals("uncertain", kept.status)
        assertEquals(original.text, kept.text)
        assertEquals(original.attachments, kept.attachments)
        assertEquals(original.requestId, kept.requestId)
        assertEquals(original.target, kept.target)
        assertEquals(original.createdAt, kept.createdAt, 0.0)
        assertTrue(kept.deliveryDismissed)
        assertFalse(kept.blocksSending)
        // the editor and the native terminal buffer are untouched
        assertEquals(listOf(editor), dismissed.drafts)
        assertEquals(listOf(buffer), dismissed.terminalBuffers)
    }

    @Test fun staleOrSettledSelectionIsANoOpForDismiss() {
        val state = MessageState(drafts = listOf(editor), outgoing = listOf(original))
        val stale = listOf(
            original.copy(text = "changed payload"),
            original.copy(attachments = emptyList()),
            original.copy(target = target.copy(run = "other-run")),
            original.copy(target = target.copy(conversation = "other-conversation")),
            original.copy(requestId = "22222222-2222-4222-8222-222222222222"),
            original.copy(status = "submitted"),
            original.copy(status = "recorded"),
            original.copy(status = "sending"),
            original.copy(deliveryDismissed = true))
        stale.forEach { assertSame(state, MessageChanges.dismissOutgoing(state, it)) }
    }

    @Test fun terminalScopeGuardRejectsAStaleCallbackAfterNavigation() {
        val targetA = target
        val targetB = target.copy(session = "session-b", run = "run-b", conversation = "conversation-b")
        // Open Terminal for A: the captured callback references A's exact blocker.
        assertTrue(TerminalBlocker.matches(original, targetA, terminalVisible = true, outgoing = listOf(original)))
        // Navigate to B: the same captured callback must be a no-op (no check/dismiss for A).
        assertFalse(TerminalBlocker.matches(original, targetB, terminalVisible = true, outgoing = listOf(original)))
        // Closing the Terminal also invalidates the captured callback.
        assertFalse(TerminalBlocker.matches(original, targetA, terminalVisible = false, outgoing = listOf(original)))
        // A different outgoing that is now the blocker invalidates the captured request id.
        val next = original.copy(requestId = "22222222-2222-4222-8222-222222222222")
        assertFalse(TerminalBlocker.matches(original, targetA, terminalVisible = true, outgoing = listOf(next)))
        // A changed connection/machine/session/run/conversation never matches.
        val moved = original.copy(target = target.copy(computerId = "other-computer"))
        assertFalse(TerminalBlocker.matches(moved, targetA, terminalVisible = true, outgoing = listOf(moved)))
    }

    @Test fun checkDeliverySettlesTheExactOriginalAndNeverEnqueuesANewSend() {
        for (v in vectors()) {
            val w = v.getJSONObject("send").getJSONObject("payload")
            val b = w.getJSONObject("binding"); val p = w.getJSONObject("payload")
            val t = Target(b.string("computer_id"), b.string("session"), b.string("run_id"), b.string("conversation_id"), "gateway")
            val files = if (p.getJSONArray("attachments").length() == 0) emptyList()
                else listOf(Attachment("55555555-5555-4555-8555-555555555555", "note.txt", "text/plain", 5, SendReceiptRecovery.hash("hello")))
            val row = OutgoingMessage(b.string("message_request_id"), t, p.string("text"), files, 1.0, "uncertain")
            val intent = NativeSendOutcome.freeze(row, p, b.string("provider"), b.string("machine_id"), b.string("session_id"), b.string("native_encoding"))!!
            val captured = row.copy(nativeSendIntent = intent)
            val state = MessageState(outgoing = listOf(captured))
            val receipt = JSONObject().put("request_id", captured.requestId).put("state", "failed").put("result", v.getJSONObject("not_sent"))
            val settled = NativeSendOutcome.settleReceipt(state, captured, receipt, 100, 99)
            // The check path only transitions the saved original; it never mints a new send row.
            assertEquals(1, settled.outgoing.size)
            assertEquals(setOf(captured.requestId), settled.outgoing.map { it.requestId }.toSet())
            assertTrue(settled.outgoing.none { it.status == "sending" })
            assertEquals("failed", settled.outgoing.single().status)
            assertEquals(captured.text, settled.outgoing.single().text)
            assertEquals(captured.attachments, settled.outgoing.single().attachments)
        }
    }
}
