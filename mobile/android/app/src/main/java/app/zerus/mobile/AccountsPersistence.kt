package app.zerus.mobile

import android.content.Context
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** Independent process-lifetime writer: account refresh never increases typing snapshots. */
class AccountsPersistence private constructor(context:Context) {
    private val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
    private val mutableError=MutableStateFlow<Throwable?>(null)
    val error:StateFlow<Throwable?> = mutableError
    private val initialized=scope.async {
        val store=PrivateStore(context.applicationContext)
        val initial=try { store.accounts() } catch(e:Exception) { mutableError.value=e;emptyList() }
        OrderedStatePersistence(initial,scope,{ store.saveAccounts(it) },onResult={ mutableError.value=it })
    }
    suspend fun open()=initialized.await()
    companion object {
        @Volatile private var instance:AccountsPersistence?=null
        fun get(context:Context)=instance ?: synchronized(this) { instance ?: AccountsPersistence(context).also { instance=it } }
    }
}
