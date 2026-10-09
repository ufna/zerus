package app.zerus.mobile

import com.google.zxing.BarcodeFormat
import com.google.zxing.qrcode.QRCodeWriter
import org.junit.Assert.*
import org.junit.Test
import java.nio.ByteBuffer

class PairingQrDecoderTest {
    private val payload = "zerus://pair?server=https%3A%2F%2Frelay.zerus.dev&code=synthetic_invitation_01234567890123456789"
    @Test fun decodesQrAcrossRotationAndPaddedCameraPlane() {
        val size = 320
        val matrix = QRCodeWriter().encode(payload,BarcodeFormat.QR_CODE,size,size)
        var pixels = ByteArray(size*size) { i -> if(matrix[i%size,i/size]) 0 else -1 }
        repeat(4) {
            val stride = (size+8)*2
            val storage = ByteArray(stride*(size+3)+7) { 77 }
            for(y in 0 until size) for(x in 0 until size) storage[7+(y+2)*stride+(x+3)*2] = pixels[y*size+x]
            val buffer = ByteBuffer.wrap(storage).apply { position(7) }
            val copied = PairingQrDecoder.luminance(buffer,stride,2,3,2,size,size)!!
            assertEquals(7,buffer.position())
            assertEquals(payload,PairingQrDecoder.decode(copied,size,size))
            val old = pixels;pixels = ByteArray(size*size)
            for(y in 0 until size) for(x in 0 until size) pixels[(size-1-x)*size+y] = old[y*size+x]
        }
    }
    @Test fun invalidAndOversizedFramesStayBounded() {
        assertNull(PairingQrDecoder.luminance(ByteBuffer.allocate(4),2,1,0,0,4,4))
        assertNull(PairingQrDecoder.luminance(ByteBuffer.allocate(4),2,1,0,0,Int.MAX_VALUE,Int.MAX_VALUE))
        assertNull(PairingQrDecoder.decode(ByteArray(10000) { -1 },100,100))
        assertNull(PairingQrDecoder.decode(ByteArray(1),100,100))
    }
    @Test fun scannerCanOnlyPublishOneResumedResult() {
        val gate = PairingScanGate()
        assertFalse(gate.claim());gate.resumed();gate.paused();assertFalse(gate.claim())
        gate.resumed();assertTrue(gate.claim());assertFalse(gate.claim());gate.paused();gate.resumed();assertFalse(gate.claim())
    }
    @Test fun explicitCancellationIsTerminalEvenBeforeUiDisposal() {
        val gate = PairingScanGate();gate.resumed();gate.cancelled();gate.resumed()
        assertFalse(gate.isActive());assertFalse(gate.claim())
    }

}
