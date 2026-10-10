package app.zerus.mobile

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap

/** Serialize push registration so a replaced request finishes before its successor registers. */
object PushSetupLocks {
    private val locks = ConcurrentHashMap<String, Mutex>()
    suspend fun <T> withConnection(connection: String, block: suspend () -> T): T =
        locks.computeIfAbsent(connection) { Mutex() }.withLock { block() }
}
