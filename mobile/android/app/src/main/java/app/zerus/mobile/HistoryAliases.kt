package app.zerus.mobile

/** Native-provided provenance only; no text or clock heuristic for durable read identity. */
object HistoryAliases {
    fun exact(events:List<Event>,known:Set<String>):Map<String,String> {
        val aliases=mutableMapOf<String,MutableSet<String>>()
        events.filter { it.historyId.isNotBlank() }.forEach { event ->
            (event.originalIds+event.originalId).filter { it.isNotBlank() && it in known && it!=event.id }.distinct().forEach { old -> aliases.getOrPut(old) { mutableSetOf() }.add(event.id) }
        }
        return aliases.mapNotNull { (old,targets) -> targets.singleOrNull()?.let { old to it } }.toMap()
    }
    fun boundary(id:String,aliases:Map<String,String>)=aliases[id] ?: id
}
