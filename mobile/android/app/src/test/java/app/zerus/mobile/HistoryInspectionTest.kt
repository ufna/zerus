package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking

class HistoryInspectionTest {
    private val target=Target("computer","codex/example/session","run","conversation","workspace")
    private fun journal(i:Int)=Event("journal:journal:AgentMessage:native:$i","AgentMessage","reply $i","",i.toDouble(),source="journal",nativeType="AgentMessage")
    private fun canonical(i:Int)=journal(i).copy(id="history:$i",historyId="$i",historyCursor="cursor:$i",historyEpoch="epoch",originalId=journal(i).id)
    private fun window()=HistoryWindow(target,"epoch",(1101..1200).map(::canonical),"cursor:1101","cursor:1200",true,false,true)
    @Test fun oldInspectedTailCannotFollowOrInvalidateNewerCanonicalHead() {
        val old=(401..500).map(::journal)
        val current=window()
        val next=current.inspection(old,old)
        assertEquals(current.events,next.events)
        assertFalse(HistoryInspection.changed(current,old,old))
        assertEquals("history:1200",next.events.last().id)
    }
    @Test fun newJournalSequenceAppearsWhileUnorderedProviderAwaitsCanonicalSync() {
        val old=(401..500).map(::journal)
        val provider=journal(1202).copy(id="provider:native_usage:AgentMessage:native:999999",source="native_usage")
        val next=window().inspection(old+provider+journal(1201),old)
        assertEquals(101,next.events.size)
        assertEquals(journal(1201).id,next.events.last().id)
        assertTrue(HistoryInspection.changed(window(),old+provider+journal(1201),old))
        assertEquals(1200L,HistoryInspection.journalWatermark(window().events))
    }
    @Test fun lateInspectionStartedBeforePagePublicationKeepsCurrentWindowAndExactAliases() {
        val current=window()
        val old=(401..500).map(::journal)
        assertEquals(current,HistoryInspection.retain(current,null,old,old))
        val refreshed=current.inspection(listOf(journal(1200).copy(text="full current body")))
        assertEquals(current.events.map { it.id },refreshed.events.map { it.id })
        assertEquals("history:1200",refreshed.events.last().id)
        assertEquals("full current body",refreshed.events.last().text)
        assertEquals("cursor:1200",refreshed.events.last().historyCursor)
    }
    @Test fun suspendedInspectionRebasesAgainBeforeWindowAndCachePublication()=runBlocking {
        val stale=window().copy(events=(401..500).map(::canonical))
        val page=window()
        val newer=page.copy(events=page.events+canonical(1201))
        var current:HistoryWindow?=page
        val entered=CompletableDeferred<Unit>();val release=CompletableDeferred<Unit>()
        var merges=0
        val flight=async(start=CoroutineStart.UNDISPATCHED) {
            HistoryInspection.rebase(stale,stale,{ current }) { latest ->
                merges++
                if(merges==1) { entered.complete(Unit);release.await() }
                HistoryInspection.retain(checkNotNull(latest),null,(401..500).map(::journal),(401..500).map(::journal))
            }
        }
        entered.await();current=newer;release.complete(Unit)
        val result=flight.await()
        assertSame(newer,result.base)
        assertEquals(2,merges)
        current=result.value
        val cached=current!!.events
        assertEquals("history:1201",cached.last().id)
        assertEquals(newer.events,cached)
    }
    @Test fun readingAnOlderWindowIsIndependentOfNewInspectionOrClockOrder() {
        val current=window().copy(hasAfter=true,atHead=false)
        assertEquals(current,HistoryInspection.retain(current,null,listOf(journal(1400)),emptyList()))
        val lowerClock=journal(1201).copy(at=1.0)
        assertEquals(lowerClock.id,window().inspection(listOf(lowerClock)).events.last().id)
    }
}
