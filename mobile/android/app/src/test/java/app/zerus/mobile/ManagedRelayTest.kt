package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody

class ManagedRelayTest {
    @Test fun knownReceiptAndPushUseCanonicalTransportWithoutReplacingRequest() = runBlocking {
        val seen=mutableListOf<okhttp3.Request>()
        val api=RelayApi(OkHttpClient.Builder().addInterceptor { chain ->
            seen+=chain.request()
            Response.Builder().request(chain.request()).protocol(okhttp3.Protocol.HTTP_1_1).code(200).message("OK")
                .body("{\"request_id\":\"original\",\"state\":\"completed\"}".toResponseBody()).build()
        }.build())
        val connection=Connection("original-identity","Workspace","https://zerus.dev.guthub.dev","synthetic")
        api.receipt(connection,"original")
        api.call(connection.url,connection.token,"/v1/push",org.json.JSONObject().put("provider","fcm").put("token","synthetic-token"))
        assertEquals(listOf("relay.zerus.dev","relay.zerus.dev"),seen.map { it.url.host })
        assertEquals("GET",seen[0].method);assertEquals("/v1/requests/original",seen[0].url.encodedPath)
        assertEquals("Bearer synthetic",seen[0].header("Authorization"))
        assertEquals(2,seen.size)
    }
    @Test fun onlyExactManagedHttpsRootMigrates() {
        listOf("https://zerus.dev.guthub.dev","https://zerus.dev.guthub.dev/","https://ZERUS.DEV.GUTHUB.DEV:443/").forEach { assertEquals(ManagedRelay.CURRENT,ManagedRelay.transport(it)) }
        listOf("http://zerus.dev.guthub.dev","https://zerus.dev.guthub.dev:444","https://zerus.dev.guthub.dev/path","https://zerus.dev.guthub.dev?x=1","https://zerus.dev.guthub.dev#x","https://synthetic-user" + "@" + "zerus.dev.guthub.dev","https://zerus.dev.guthub.dev.example","https://custom.example").forEach { assertEquals(it,ManagedRelay.transport(it)) }
    }
    @Test fun effectiveEndpointDoesNotRekeyDraftOutboxOrKnownReceipt() {
        val connection=Connection("saved-workspace","","https://zerus.dev.guthub.dev","synthetic")
        val target=Target("node","session","run","conversation",connection.id)
        val draft=Draft(target,"Saved draft").begin()
        val outgoing=OutgoingMessage("known-request",target,"Saved message",emptyList(),1.0,"uncertain")
        val state=MessageState(listOf(draft),listOf(outgoing))
        assertEquals("relay.zerus.dev",connection.displayName)
        assertEquals("relay.zerus.dev",connection.copy(label="zerus.dev.guthub.dev").displayName)
        assertEquals("My workspace",connection.copy(label="My workspace").displayName)
        assertEquals("https://zerus.dev.guthub.dev",connection.url)
        assertEquals(ManagedRelay.CURRENT,connection.endpoint)
        val restored=MessageCodec.state(MessageCodec.stateJson(state))
        assertEquals(target.key,restored.drafts.single().target.key)
        assertEquals(draft.requestId,restored.drafts.single().requestId)
        assertEquals("uncertain",restored.drafts.single().status)
        assertEquals("known-request",restored.outgoing.single().requestId)
        assertEquals("uncertain",restored.outgoing.single().status)
    }
}
