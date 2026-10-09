package app.zerus.mobile

/** A device-local review choice captures evidence; it never answers native questions. */
data class ReadAttentionEvidence(val target:Target,val fingerprint:String,val epoch:String,val head:Long?,val authoritative:Boolean,val loadedIds:Set<String>,val loadedThrough:Long?=null)
object ReadAttentionPolicies {
    fun mark(state:ConversationReadState,evidence:ReadAttentionEvidence):ConversationReadState {
        val old=state.record(evidence.target)
        val next=if(evidence.authoritative) {
            if(old==null || !old.headComplete || old.epoch!=evidence.epoch || old.headIncoming!=evidence.head || evidence.head==null) return state
            state.copy(records=state.records.map { if(ConversationReadPolicies.key(it.target)==ConversationReadPolicies.key(evidence.target)) it.copy(readThrough=maxOf(it.readThrough,evidence.head),reviewLater=false,established=true) else it })
        } else {
            var loaded=state.viewed(evidence.target,evidence.loadedIds).review(evidence.target,false)
            val through=evidence.loadedThrough
            if(evidence.loadedIds.isNotEmpty() && through!=null && through>=0 && old?.epoch?.isNotBlank()==true && old.epoch==evidence.epoch) {
                loaded=loaded.copy(records=loaded.records.map { record ->
                    if(ConversationReadPolicies.key(record.target)==ConversationReadPolicies.key(evidence.target)) record.copy(readThrough=maxOf(record.readThrough,through),established=true) else record
                })
            }
            loaded
        }
        return next
    }
}

/** Opening may know unread exists before its canonical page is loaded. */
object OpeningUnreadBoundary {
    data class Decision(val captured:Boolean,val eventId:String="")
    fun decide(ready:Boolean,record:ConversationReadRecord?,rows:List<Event>):Decision {
        if(!ready) return Decision(false)
        if(record?.established!=true || record.incomplete) return Decision(true)
        val candidate=rows.firstOrNull { ConversationReadPolicies.unread(record,it) }
        if(candidate!=null) return Decision(true,candidate.id)
        if(record.epoch.isNotBlank() && (record.headIncoming ?: 0)>record.readThrough) return Decision(false)
        return Decision(true)
    }
}
