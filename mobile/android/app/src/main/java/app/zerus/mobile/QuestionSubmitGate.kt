package app.zerus.mobile

/** One form attempt shared by icon, IME and hardware-key callbacks. The VM owns durability. */
internal class QuestionSubmitGate {
    private var pending = false
    fun claim(enabled: Boolean): Boolean {
        if(!enabled || pending) return false
        pending = true
        return true
    }
    fun finish() { pending = false }
}
