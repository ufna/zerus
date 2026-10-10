package app.zerus.mobile

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

object HistoryReadPaths {
    data class Result(val page:HistoryPage,val verified:Boolean)
    /** A corrupt or incompatible display cache is a miss, never evidence for a live action. */
    suspend fun saved(target:Target,cached:suspend ()->JSONObject?):HistoryPage? {
        try {
            val raw=cached() ?: return null
            return withContext(Dispatchers.Default) { HistoryPage.cached(target,raw) }
        } catch(error:CancellationException) { throw error }
        catch(error:Exception) { return null }
    }
    suspend fun read(target:Target,network:suspend ()->HistoryPage,cached:suspend ()->JSONObject?):Result {
        try { return Result(network(),true) } catch(error:CancellationException) { throw error }
        catch(error:Exception) {
            val page=saved(target,cached) ?: throw error
            return Result(page,false)
        }
    }
}
