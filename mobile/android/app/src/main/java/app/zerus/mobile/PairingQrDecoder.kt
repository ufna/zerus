package app.zerus.mobile

import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean

/** One scanner lifetime: a paused or dismissed scanner cannot publish an old frame. */
internal class PairingScanGate {
    private val active = AtomicBoolean(false)
    private val delivered = AtomicBoolean(false)
    private val cancelled = AtomicBoolean(false)
    fun resumed() { if (!delivered.get() && !cancelled.get()) active.set(true) }
    fun paused() { active.set(false) }
    fun cancelled() { cancelled.set(true);active.set(false) }
    fun claim(): Boolean = active.get() && !cancelled.get() && delivered.compareAndSet(false, true)
    fun isActive(): Boolean = active.get() && !cancelled.get() && !delivered.get()
}

internal object PairingQrDecoder {
    const val MAX_PIXELS = 1920 * 1080
    /** Copies only luminance, honoring camera padding/crop/pixel stride without mutating its buffer. */
    fun luminance(buffer: ByteBuffer, rowStride: Int, pixelStride: Int, left: Int, top: Int, width: Int, height: Int): ByteArray? {
        if(width <= 0 || height <= 0 || width.toLong() * height > MAX_PIXELS || left < 0 || top < 0 || rowStride <= 0 || pixelStride <= 0) return null
        val base = buffer.position().toLong()
        val last = base + (top.toLong() + height - 1) * rowStride + (left.toLong() + width - 1) * pixelStride
        if(last >= buffer.limit() || (left.toLong() + width) * pixelStride > rowStride) return null
        return ByteArray(width * height).also { out ->
            for(y in 0 until height) for(x in 0 until width)
                out[y * width + x] = buffer.get((base + (top + y).toLong() * rowStride + (left + x).toLong() * pixelStride).toInt())
        }
    }
    fun decode(bytes: ByteArray, width: Int, height: Int): String? {
        if(width <= 0 || height <= 0 || width.toLong() * height > MAX_PIXELS || bytes.size != width * height) return null
        var pixels = bytes
        var w = width
        var h = height
        val reader = QRCodeReader()
        repeat(4) {
            try { return reader.decode(BinaryBitmap(HybridBinarizer(PlanarYUVLuminanceSource(pixels,w,h,0,0,w,h,false))), mapOf(DecodeHintType.CHARACTER_SET to "UTF-8")).text
                .takeIf { it.length <= PairingInvite.MAX_LENGTH } }
            catch(_: ReaderException) { } finally { reader.reset() }
            val rotated = ByteArray(pixels.size)
            for(y in 0 until h) for(x in 0 until w) rotated[(w - 1 - x) * h + y] = pixels[y * w + x]
            pixels = rotated
            val oldWidth = w;w = h;h = oldWidth
        }
        return null
    }
}
