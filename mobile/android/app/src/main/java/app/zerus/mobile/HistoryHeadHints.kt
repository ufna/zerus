package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import java.math.BigDecimal

/** JSONObject display copies may normalize 1.0 to 1; that is no native change. */
object HistoryHeadHints {
    fun value(raw:JSONObject?,key:String):Any {
        val value=raw?.opt(key) ?: return JSONObject.NULL
        return if(value is Number) runCatching { BigDecimal(value.toString()).stripTrailingZeros() }.getOrNull() ?: value else value
    }
    fun signature(preview:String,raw:JSONObject)=JSONArray(listOf(preview,value(raw,"last_event_at"),value(raw,"turn_started"),value(raw,"last_response_at"))).toString()
    fun readFingerprint(target:Target,raw:JSONObject?,record:ConversationReadRecord?)=JSONArray(listOf(target.key,raw?.string("phase"),
        value(raw,"last_event_at"),value(raw,"last_response_at"),raw?.string("prompt"),raw?.optJSONArray("mobile_attention"),record?.epoch,record?.headIncoming)).toString()
}
