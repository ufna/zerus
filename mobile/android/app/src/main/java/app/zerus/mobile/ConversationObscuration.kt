package app.zerus.mobile

import androidx.compose.runtime.*

/** A modal can leave the Activity resumed while its conversation is no longer visible. */
internal class ConversationObscuration {
    var count by mutableIntStateOf(0)
    val obscured get() = count > 0
}
internal val LocalConversationObscuration = staticCompositionLocalOf<ConversationObscuration?> { null }

@Composable internal fun ObscureConversation() {
    val owner = LocalConversationObscuration.current
    DisposableEffect(owner) {
        owner?.let { it.count++ }
        onDispose { owner?.let { it.count = (it.count - 1).coerceAtLeast(0) } }
    }
}
