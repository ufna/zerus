package app.zerus.mobile

/** Persistence echoes never edit an active IME buffer. Only a new composition
 * generation (send clear, restore or explicit replacement) owns a text reset.
 */
data class ComposerSync(val generation: String)
data class ComposerUpdate(val sync: ComposerSync, val replacement: String? = null)
object ComposerReconciler {
    fun external(sync: ComposerSync, generation: String, text: String): ComposerUpdate =
        if (sync.generation == generation) ComposerUpdate(sync)
        else ComposerUpdate(ComposerSync(generation), text)
}
