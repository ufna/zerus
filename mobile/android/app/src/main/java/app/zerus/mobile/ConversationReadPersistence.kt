package app.zerus.mobile

import android.content.Context
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** Read positions and receipts have independent writers, keeping typing snapshots small. */
class ConversationReadPersistence private constructor(context:Context) {
    private val mutableError=MutableStateFlow<Throwable?>(null)
    val error:StateFlow<Throwable?> = mutableError
    private val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
    private val initialized=scope.async {
        val store=PrivateStore(context.applicationContext)
        OrderedStatePersistence(store.readState(),scope,{ store.saveReadState(it) },onResult={ mutableError.value=it })
    }
    suspend fun open()=initialized.await()
    companion object {
        @Volatile private var instance:ConversationReadPersistence?=null
        fun get(context:Context)=instance ?: synchronized(this) { instance ?: ConversationReadPersistence(context).also { instance=it } }
    }
}
