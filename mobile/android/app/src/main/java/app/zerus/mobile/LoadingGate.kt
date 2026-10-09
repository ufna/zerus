package app.zerus.mobile

/** Read-operation ownership and delayed presentation, using an injected monotonic clock. */
class LoadingGate(private val now: () -> Long, private val delayMillis: Long = 5_000) {
    data class Operation(val id: Long, val scope: String, val startedAt: Long, val explicit: Boolean)
    data class Start(val operation: Operation, val newRequest: Boolean)
    private var sequence = 0L
    var current: Operation? = null
        private set

    fun begin(scope: String, explicit: Boolean = false): Start {
        current?.takeIf { it.scope == scope }?.let { existing ->
            val promoted = existing.copy(explicit = existing.explicit || explicit)
            current = promoted
            return Start(promoted, false)
        }
        val operation = Operation(++sequence, scope, now(), explicit)
        current = operation
        return Start(operation, true)
    }
    fun owns(id: Long) = current?.id == id
    fun visible() = current?.let { it.explicit || now() - it.startedAt >= delayMillis } ?: false
    fun remaining(id: Long): Long = current?.takeIf { it.id == id }?.let {
        if (it.explicit) 0 else (delayMillis - (now() - it.startedAt)).coerceAtLeast(0)
    } ?: 0
    fun finish(id: Long): Boolean {
        if (!owns(id)) return false
        current = null
        return true
    }
    fun cancel() { current = null }
}
