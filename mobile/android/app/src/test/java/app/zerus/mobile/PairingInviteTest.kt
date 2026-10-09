package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test
import java.net.URLEncoder

class PairingInviteTest {
    private val code = "synthetic_invitation_01234567890123456789"
    private fun link(server: String = "https://relay.zerus.dev", code: String = this.code) = "zerus://pair?server=${URLEncoder.encode(server,"UTF-8")}&code=$code"
    @Test fun knownInvitePreservesCodeAndCustomGateway() {
        assertEquals(PairingInvite("https://private.example/gateway",code),PairingInvite.parse(link("https://private.example/gateway/")))
        assertEquals(PairingInvite(ManagedRelay.CURRENT,code),PairingInvite.parse(link("https://zerus.dev.guthub.dev")))
    }
    @Test fun rejectsUnrelatedMalformedOrAmbiguousLinks() {
        listOf("https://example.test",link().replace("zerus://pair","zerus://other"),link()+"&code=$code",link()+"&%63ode=$code",
            link()+"&extra=1",link()+"#fragment",link().replace("pair?","pair:80?"),link().replace("pair?","user@pair?"),
            link().replace("pair?","pair/path?"),"zerus://pair?code=$code",link()+"&",link().replace("server=","server=%GG")).forEach { assertNull(it,PairingInvite.parse(it)) }
    }
    @Test fun rejectsUnsafeGatewayAndCodeWithoutChangingManualForm() {
        listOf("http://relay.zerus.dev","https://u:p@example.test","https://example.test?q=1","https://example.test#fragment","https://example.test:0","https://example.test:65536","https://example.test/ a").forEach { assertNull(PairingInvite.parse(link(it))) }
        listOf("short","a".repeat(129),"a".repeat(16)+"%0A","a".repeat(16)+"%FF","a".repeat(16)+"%2B").forEach { assertNull(PairingInvite.parse(link(code=it))) }
        assertNull(PairingInvite.parse(link()+"x".repeat(4096)))
    }
    @Test fun managedAliasRequiresExactOrigin() {
        assertEquals("https://zerus.dev.guthub.dev/custom",PairingInvite.parse(link("https://zerus.dev.guthub.dev/custom"))?.server)
        assertEquals("https://zerus.dev.guthub.dev.example",PairingInvite.parse(link("https://zerus.dev.guthub.dev.example"))?.server)
    }
}
