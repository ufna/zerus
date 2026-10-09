package app.zerus.mobile
import java.net.UnknownHostException
import org.junit.Assert.*
import org.junit.Test
class ReadFailureTest {
    @Test fun readNetworkFailureDoesNotExposeEndpointOrPlatformException() {
        val text=ReadFailure.message(UnknownHostException("private.invalid: No address"),"conversation")
        assertEquals("No connection. Showing saved conversation. Refresh to try again.",text)
        assertFalse(text.contains("private.invalid"))
    }
    @Test fun nativeOrRelayReasonRemainsMeaningful() {
        assertEquals("Session is no longer available.",ReadFailure.message(RelayException(404,"Session is no longer available."),"conversation"))
    }
}
