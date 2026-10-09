package app.zerus.mobile

import kotlinx.coroutines.CancellationException

/** Retries reads only after the original mutation receipt is already confirmed. */
object ConfirmedTargetResolution {
    suspend fun <T> read(attempts:Int=4,active:()->Boolean,pause:suspend ()->Unit,inspect:suspend ()->T):T {
        require(attempts in 1..8)
        var last:Exception?=null
        repeat(attempts) { index ->
            if(!active()) throw CancellationException("Target opening was canceled.")
            try { return inspect() } catch(error:CancellationException) { throw error }
            catch(error:Exception) { last=error }
            if(index+1<attempts) pause()
        }
        throw checkNotNull(last)
    }
}
