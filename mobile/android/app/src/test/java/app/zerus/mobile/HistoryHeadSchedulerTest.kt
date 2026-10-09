package app.zerus.mobile

import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test

class HistoryHeadSchedulerTest {
    private val targets=(1..6).map { Target("computer","codex/project/chat$it","run","conversation$it","workspace") }
    @Test fun incompleteAndFailedFirstSourcesDoNotStarveLaterTrackedChats() {
        val scheduler=HistoryHeadScheduler()
        assertEquals(targets.take(2),scheduler.choose(targets,0))
        scheduler.result(targets[0],0,false);scheduler.result(targets[1],0,true)
        assertEquals(targets.drop(2).take(2),scheduler.choose(targets,5000))
        assertEquals(targets.drop(4),scheduler.choose(targets,10000))
        assertEquals(targets.take(2),scheduler.choose(targets,15000))
    }
    @Test fun repeatedFailingSourceBacksOffWhileOtherConversationsProgress() {
        val scheduler=HistoryHeadScheduler()
        assertEquals(listOf(targets[0]),scheduler.choose(targets.take(1),0))
        scheduler.result(targets[0],0,false)
        assertTrue(scheduler.choose(targets.take(1),5000).isEmpty())
        assertEquals(listOf(targets[1]),scheduler.choose(targets.take(2),5000))
    }
    @Test fun earlyIncompleteIndexCannotReplaceCachedNewestTail()=runBlocking {
        val newest=listOf(Event("latest","AgentMessage","new reply", ""))
        val old=listOf(Event("old","AgentMessage","early index row", ""))
        var cached=newest
        val page=HistoryPage(targets[0],"epoch",old,null,null,false,true,null,null,false,true,true,JSONObject())
        HistoryPageRetention.retain(100,page) { cached=it.events }
        assertEquals("new reply",cached.single().text)
        HistoryPageRetention.retain(100,page,"before") { cached=it.events }
        assertEquals("early index row",cached.single().text)
    }
    @Test fun metadataOnlyProbeCannotReplaceFullCachedHead()=runBlocking {
        fun page(rows:Int)=HistoryPage(targets[0],"epoch",(1..rows).map { Event("history:$it","AgentMessage","reply", "",historyId="$it",historyCursor="$it",historyEpoch="epoch") },null,null,false,false,100,100,true,false,false,JSONObject())
        var cached:HistoryPage?=null
        HistoryPageRetention.retain(100,page(100)) { cached=it }
        HistoryPageRetention.retain(1,page(1)) { cached=it }
        assertEquals(100,cached!!.events.size)
    }
}
