package app.zerus.mobile

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/** One ordered writer. UI reducers are immediate; only acknowledged critical reducers release transport. */
class OrderedStatePersistence<S>(initial: S, scope: CoroutineScope,
    private val write: suspend (S) -> Unit,
    private val uiDispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
    private val debounceMillis: Long = 200,
    private val maxDelayMillis: Long = 800,
    private val onResult: (Throwable?) -> Unit = {}) {
    private sealed interface Command<S> {
        data class Edit<S>(val sequence: Long, val reduce: (S) -> S) : Command<S>
        data class Durable<S>(val sequence: Long, val reduce: (S) -> S, val acknowledge: (S, S) -> S, val reply: CompletableDeferred<S>) : Command<S>
        data class Flush<S>(val sequence: Long, val reply: CompletableDeferred<S>?) : Command<S>
    }
    private val lock = Any()
    private var acceptedSequence = 0L
    private val mutable = MutableStateFlow(initial)
    val state: StateFlow<S> = mutable
    @Volatile var durableState: S = initial; private set
    private val commands = Channel<Command<S>>(Channel.UNLIMITED)
    init { scope.launch(Dispatchers.IO) { runWriter(initial) } }
    fun edit(reduce: (S) -> S): S = synchronized(lock) {
        val next = reduce(mutable.value)
        check(commands.trySend(Command.Edit(++acceptedSequence, reduce)).isSuccess) { "Private state writer is unavailable." }
        mutable.value = next
        next
    }
    suspend fun durable(reduce: (S) -> S, acknowledge: (S, S) -> S = { current, _ -> reduce(current) }): S {
        val reply = CompletableDeferred<S>()
        synchronized(lock) { check(commands.trySend(Command.Durable(++acceptedSequence, reduce, acknowledge, reply)).isSuccess) }
        return withContext(NonCancellable) { reply.await() }
    }
    fun flushAsync() { synchronized(lock) { commands.trySend(Command.Flush(++acceptedSequence, null)) } }
    suspend fun flush(): S {
        val reply = CompletableDeferred<S>()
        synchronized(lock) { check(commands.trySend(Command.Flush(++acceptedSequence, reply)).isSuccess) }
        return reply.await()
    }
    private suspend fun runWriter(initial: S) {
        var current = initial
        var dirty = false
        var firstDirty = 0L
        var deadline = 0L
        fun now() = System.nanoTime() / 1_000_000
        suspend fun persisted(value: S) {
            write(value)
            durableState = value
            withContext(uiDispatcher) { onResult(null) }
        }
        suspend fun publishCaughtUp(sequence: Long, value: S) {
            withContext(uiDispatcher) { synchronized(lock) { if (sequence == acceptedSequence) mutable.value = value } }
        }
        suspend fun failure(error: Throwable) { withContext(uiDispatcher) { onResult(error) } }
        while (true) {
            val command = if (!dirty) commands.receive() else withTimeoutOrNull((deadline - now()).coerceAtLeast(1)) { commands.receive() }
            if (command == null) {
                try { persisted(current) } catch (error: Exception) { failure(error) }
                dirty = false // Retry only on a new edit, explicit flush or a critical user action.
                continue
            }
            when (command) {
                is Command.Edit -> {
                    try {
                        current = command.reduce(current)
                        publishCaughtUp(command.sequence, current)
                        if (!dirty) firstDirty = now()
                        dirty = true
                        deadline = minOf(now() + debounceMillis, firstDirty + maxDelayMillis)
                    } catch (error: Exception) { failure(error) }
                }
                is Command.Durable -> {
                    try {
                        val next = command.reduce(current)
                        persisted(next)
                        current = next; dirty = false
                        withContext(uiDispatcher) { synchronized(lock) { mutable.value = command.acknowledge(mutable.value, next) } }
                        command.reply.complete(next)
                    } catch (error: Exception) { failure(error); command.reply.completeExceptionally(error) }
                }
                is Command.Flush -> {
                    try { persisted(current); dirty = false; publishCaughtUp(command.sequence, current); command.reply?.complete(current) }
                    catch (error: Exception) { dirty = false; failure(error); command.reply?.completeExceptionally(error) }
                }
            }
        }
    }
}
