package app.zerus.mobile

/** Three continuous foreground/visible seconds; background time never approves an action. */
data class ApprovalReview(val startedAt: Long? = null) {
    fun observe(eligible: Boolean, nowMillis: Long): ApprovalReview =
        if (!eligible || startedAt != null && nowMillis < startedAt) ApprovalReview()
        else if (startedAt == null) ApprovalReview(nowMillis) else this

    fun secondsRemaining(nowMillis: Long): Int = startedAt?.let {
        (3 - ((nowMillis - it).coerceAtLeast(0) / 1000).toInt()).coerceIn(0, 3)
    } ?: 3
}
