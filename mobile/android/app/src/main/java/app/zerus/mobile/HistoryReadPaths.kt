package app.zerus.mobile

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

object HistoryReadPaths {
    data class Result(val page:HistoryPage,val verified:Boolean)
    suspend fun read(target:Target,network:suspend ()->HistoryPage,cached:suspend ()->JSONObject?):Result {
        try { return Result(network(),true) } catch(error:CancellationException) { throw error }
        catch(error:Exception) {
            val raw=cached() ?: throw error
            return Result(withContext(Dispatchers.Default) { HistoryPage.cached(target,raw) },false)
        }
    }
}
