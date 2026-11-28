package app.zerus.mobile

/**
 * Guard for the live parent Terminal's Check delivery / Dismiss recovery actions (zerus-4ayxb).
 * A stale Terminal callback must not act on a message after the person navigated to another
 * session or after a different outgoing became the blocker.
 */
object TerminalBlocker {
    /** True only when [message] is still the exact outgoing original blocking the currently open Terminal. */
    fun matches(message: OutgoingMessage, selectedTarget: Target?, terminalVisible: Boolean, outgoing: List<OutgoingMessage>): Boolean {
        val target = selectedTarget ?: return false
        if (!terminalVisible || !message.belongsTo(target)) return false
        val blocker = outgoing.lastOrNull { it.belongsTo(target) && it.blocksSending }
        return blocker?.requestId == message.requestId
    }
}
