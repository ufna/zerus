package app.zerus.mobile

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap

/** Serialize provider switches so an older FCM request finishes before chosen UP setup. */
object PushSetupLocks {
    private val locks = ConcurrentHashMap<String, Mutex>()
    suspend fun <T> withConnection(connection: String, block: suspend () -> T): T =
        locks.computeIfAbsent(connection) { Mutex() }.withLock { block() }
}
