package app.zerus.mobile

import java.io.IOException

/** Presentation for reads only; mutation receipt uncertainty remains separate. */
object ReadFailure {
    fun message(failure:Exception,saved:String):String = if(failure is IOException)
        "No connection. Showing saved $saved. Refresh to try again."
    else failure.message.orEmpty().ifBlank { "Could not refresh $saved. Refresh to try again." }
}
