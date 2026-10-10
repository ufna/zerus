package app.zerus.mobile

enum class PendingCrashAction { Send, Delete, None }
data class CrashReportingPlan(val action: PendingCrashAction, val discardPending: Boolean, val capture: Boolean)

/** Public deletion is asynchronous without a completion handle. Retain the discard
 * boundary until a later startup confirms no reports remain; upload never races it. */
object CrashReportingPolicy {
    fun startup(enabled: Boolean, discardPending: Boolean, hasReports: Boolean): CrashReportingPlan = when {
        !enabled -> CrashReportingPlan(if (hasReports) PendingCrashAction.Delete else PendingCrashAction.None, true, false)
        discardPending && hasReports -> CrashReportingPlan(PendingCrashAction.Delete, true, false)
        discardPending -> CrashReportingPlan(PendingCrashAction.None, false, true)
        else -> CrashReportingPlan(if (hasReports) PendingCrashAction.Send else PendingCrashAction.None, false, true)
    }
}
