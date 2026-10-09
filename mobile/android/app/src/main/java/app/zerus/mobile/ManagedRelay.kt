package app.zerus.mobile

import java.net.URI

/** A transport alias, never a connection, workspace or request identity migration. */
object ManagedRelay {
    const val CURRENT = "https://relay.zerus.dev"
    private const val LEGACY_HOST = "zerus.dev.guthub.dev"
    fun transport(stored: String): String {
        val uri = runCatching { URI(stored) }.getOrNull() ?: return stored
        return if (uri.scheme == "https" && uri.host.equals(LEGACY_HOST, ignoreCase = true) &&
            uri.port in setOf(-1, 443) && uri.rawUserInfo == null && uri.rawQuery == null &&
            uri.rawFragment == null && uri.rawPath in setOf("", "/")) CURRENT else stored
    }
}
