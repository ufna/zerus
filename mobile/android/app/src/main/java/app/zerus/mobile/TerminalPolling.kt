package app.zerus.mobile

/** Reading is adaptive; this policy never retries terminal input. */
data class TerminalPolling(val unchanged: Int = 0) {
    val delayMillis: Long get() = (250L shl unchanged.coerceIn(0,3)).coerceAtMost(2000)
    companion object {
        fun readAllowed(foreground: Boolean,focused: Boolean,selecting: Boolean,reviewing: Boolean,ownedKeyConfirmation: Boolean) =
            foreground && !selecting && !reviewing && (focused || ownedKeyConfirmation)
    }
    fun observed(changed: Boolean) = if(changed) TerminalPolling() else copy(unchanged = (unchanged + 1).coerceAtMost(3))
}
