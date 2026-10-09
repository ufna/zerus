package app.zerus.mobile

/** Text comparison only; callers must prove scope, native types, time and a unique pair. */
object NativeStopExcerpt {
    private const val JOURNAL_DETAIL_LIMIT = 1200

    fun matches(providerText: String, hookText: String): Boolean {
        if (hookText.isBlank() || hookText.trim().all { it == '…' }) return false
        if (providerText.contains(hookText)) return true
        // journal::clipped takes Unicode scalar characters, then adds U+2026 only
        // when more text remains. Short, meaningful literal ellipses stay literal.
        if (!hookText.endsWith('…') ||
            hookText.codePointCount(0, hookText.length) != JOURNAL_DETAIL_LIMIT + 1) return false
        val excerpt = hookText.dropLast(1)
        return excerpt.isNotBlank() && providerText.contains(excerpt)
    }
}
