package app.zerus.mobile
import kotlinx.coroutines.*
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.TimeUnit
class RelayCancellationTest {
    @Test fun cancellationStopsBodyReadWithoutRetry()=runBlocking {
        val server=MockWebServer();server.start()
        val client=OkHttpClient.Builder().retryOnConnectionFailure(false).readTimeout(30,TimeUnit.SECONDS).build()
        server.enqueue(MockResponse().setBody("{\"ok\":true}").throttleBody(1,500,TimeUnit.MILLISECONDS))
        try {
            val job=launch { RelayApi(client).call(server.url("/").toString(),"","/v1/requests",org.json.JSONObject()) }
            withContext(Dispatchers.IO) { assertNotNull(server.takeRequest(3,TimeUnit.SECONDS)) }
            delay(100);job.cancelAndJoin()
            withTimeout(2000) { while(client.dispatcher.runningCallsCount()!=0) delay(20) }
            assertEquals(1,server.requestCount)
        } finally { client.connectionPool.evictAll();server.shutdown() }
    }
}
