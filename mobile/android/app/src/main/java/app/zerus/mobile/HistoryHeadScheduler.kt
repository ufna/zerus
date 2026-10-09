package app.zerus.mobile

/** Fair bounded reads: an unavailable or indexing source cannot starve later chats. */
class HistoryHeadScheduler {
    private data class Attempt(val at:Long,val failures:Int=0,val next:Long=0)
    private val attempts=mutableMapOf<String,Attempt>()
    fun choose(targets:List<Target>,now:Long,limit:Int=2):List<Target> = targets.distinctBy(ConversationReadPolicies::key)
        .filter { (attempts[ConversationReadPolicies.key(it)]?.next ?: 0)<=now }
        .sortedBy { attempts[ConversationReadPolicies.key(it)]?.at ?: Long.MIN_VALUE }.take(limit).onEach {
            val key=ConversationReadPolicies.key(it);val previous=attempts[key]
            attempts[key]=Attempt(now,previous?.failures ?: 0,now+5000)
        }
    fun result(target:Target,now:Long,success:Boolean) {
        val key=ConversationReadPolicies.key(target);val previous=attempts[key] ?: return
        val failures=if(success) 0 else (previous.failures+1).coerceAtMost(5)
        attempts[key]=Attempt(previous.at,failures,now+if(success) 5000 else (5000L shl failures).coerceAtMost(120000))
    }
}
object HistoryPageRetention {
    suspend fun retain(limit:Int,page:HistoryPage,direction:String="",write:suspend (HistoryPage)->Unit) {
        // Metadata-only probes use the same newest selector but cannot replace a full page.
        if(limit==100 && (page.complete || direction.isNotEmpty())) write(page)
    }
}
