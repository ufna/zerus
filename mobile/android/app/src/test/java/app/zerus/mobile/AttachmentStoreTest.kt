package app.zerus.mobile

import kotlinx.coroutines.CancellationException
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.File
import java.io.IOException
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermission
import java.util.Base64
import java.util.UUID
import javax.crypto.KeyGenerator

/** Synthetic streams and an injected software AES key; no device files or keys. */
class AttachmentStoreTest {
    private lateinit var temporary: File
    private lateinit var root: File
    private lateinit var files: EncryptedAttachmentFiles
    private var syncCount = 0

    @Before fun setup() {
        temporary = Files.createTempDirectory("zerus-attachment-test-").toFile()
        root = File(temporary, "attachments")
        val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
        files = EncryptedAttachmentFiles(root, { key }) { syncCount++ }
    }

    @After fun cleanup() { temporary.deleteRecursively() }

    private fun saved(name: String = "fixture.txt", data: ByteArray = "private fixture body".toByteArray()) =
        files.copyFrom(ByteArrayInputStream(data), name, "text/plain", emptyList())

    private fun metadata(bytes: Long = 1) = Attachment(UUID.randomUUID().toString(), "fixture.bin",
        "application/octet-stream", bytes, "0".repeat(64))

    private fun failure(block: () -> Unit): AttachmentException {
        try { block() } catch (error: AttachmentException) { return error }
        throw AssertionError("Expected attachment rejection")
    }

    @Test fun immutableEncryptedCopySurvivesSourceChangesAndHasNoWirePathOrReference() {
        val source = "synthetic secret fixture".toByteArray()
        val original = source.copyOf()
        val attachment = saved(data = source)
        source.fill(0)
        val blob = File(root, "${attachment.id}.bin").readBytes()
        assertFalse(String(blob, Charsets.ISO_8859_1).contains(String(original)))
        val wire = files.wire(attachment)
        assertArrayEquals(original, Base64.getDecoder().decode(wire.getString("data_base64")))
        assertEquals(setOf("name", "mime", "data_base64"), wire.keys().asSequence().toSet())
        assertEquals(1, syncCount)
        assertEquals(Attachment.fromJson(attachment.toJson()), attachment)
        assertEquals(setOf(PosixFilePermission.OWNER_READ, PosixFilePermission.OWNER_WRITE),
            Files.getPosixFilePermissions(File(root, "${attachment.id}.bin").toPath()))
    }

    @Test fun emptyAndOverLimitInputsLeaveNoPartOrPublishedFile() {
        assertTrue(failure { saved(data = byteArrayOf()) }.message!!.contains("Empty"))
        assertEquals(0, root.listFiles()!!.size)
        val data = ByteArray(AttachmentPolicy.MAX_FILE_BYTES.toInt() + 1)
        assertTrue(failure { saved(data = data) }.message!!.contains("10 MiB"))
        assertEquals(0, root.listFiles()!!.size)
    }

    @Test fun exactPerFileBoundaryImportsAndRevalidates() {
        val data = ByteArray(AttachmentPolicy.MAX_FILE_BYTES.toInt()) { (it % 251).toByte() }
        val attachment = saved(data = data)
        assertEquals(AttachmentPolicy.MAX_FILE_BYTES, attachment.bytes)
        assertArrayEquals(data, Base64.getDecoder().decode(files.wire(attachment).getString("data_base64")))
    }

    @Test fun aggregateAndCountBoundsUseActualBytesNotProviderMetadata() {
        val current = listOf(metadata(AttachmentPolicy.MAX_FILE_BYTES), metadata(AttachmentPolicy.MAX_FILE_BYTES))
        assertTrue(failure { files.copyFrom(ByteArrayInputStream(byteArrayOf(1)), "small.bin", null, current) }
            .message!!.contains("20 MiB"))
        assertEquals(0, root.listFiles()!!.size)
        assertTrue(failure { files.copyFrom(ByteArrayInputStream(byteArrayOf(1)), "small.bin", null,
            List(8) { metadata() }) }.message!!.contains("8 files"))
    }

    @Test fun cancellationAndReadFailureCleanAllPartialFiles() {
        var checkpoints = 0
        try {
            files.copyFrom(ByteArrayInputStream(ByteArray(200_000)), "fixture.bin", null, emptyList()) {
                if (++checkpoints > 2) throw CancellationException("synthetic cancellation")
            }
            fail("Expected cancellation")
        } catch (_: CancellationException) { }
        assertEquals(0, root.listFiles()!!.size)
        val broken = object : ByteArrayInputStream(ByteArray(200_000)) {
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int { throw IOException("synthetic failure") }
        }
        try { files.copyFrom(broken, "fixture.bin", null, emptyList()); fail("Expected read failure")
        } catch (_: IOException) { }
        assertEquals(0, root.listFiles()!!.size)
    }

    @Test fun failedDirectoryCommitDoesNotPublishAnUnownedFile() {
        val key = KeyGenerator.getInstance("AES").generateKey()
        val broken = EncryptedAttachmentFiles(root, { key }) { throw IOException("synthetic fsync failure") }
        try { broken.copyFrom(ByteArrayInputStream(byteArrayOf(1)), "fixture.bin", null, emptyList())
            fail("Expected durable commit failure")
        } catch (_: IOException) { }
        assertEquals(0, root.listFiles()!!.size)
    }

    @Test fun changedDigestSizeNameMimeAndMissingFileAreRejectedBeforeWire() {
        val attachment = saved()
        failure { files.wire(attachment.copy(sha256 = "f".repeat(64))) }
        failure { files.wire(attachment.copy(bytes = attachment.bytes + 1)) }
        failure { files.wire(attachment.copy(name = "changed-name.txt")) }
        failure { files.wire(attachment.copy(mime = "application/json")) }
        File(root, "${attachment.id}.bin").delete()
        failure { files.wire(attachment) }
    }

    @Test fun ciphertextTagMutationTruncationAndCrossReferenceSwapCannotBeSent() {
        val first = saved()
        val firstFile = File(root, "${first.id}.bin")
        val original = firstFile.readBytes()
        original[original.lastIndex] = (original.last().toInt() xor 1).toByte()
        firstFile.writeBytes(original)
        failure { files.wire(first) }
        firstFile.writeBytes(original.copyOf(20))
        failure { files.wire(first) }
        val second = saved(data = byteArrayOf(1, 2, 3))
        firstFile.writeBytes(File(root, "${second.id}.bin").readBytes())
        failure { files.wire(first.copy(bytes = second.bytes, sha256 = second.sha256)) }
    }

    @Test fun removingProtectedObjectsNeverDeletesDraftOrInFlightCopies() {
        val attachment = saved()
        assertFalse(files.remove(attachment.id, setOf(attachment.id)))
        assertTrue(File(root, "${attachment.id}.bin").exists())
        assertTrue(files.remove(attachment.id, emptySet()))
        assertFalse(files.remove(attachment.id, emptySet()))
        assertEquals(2, syncCount)
        failure { files.remove("../fixture", emptySet()) }
    }

    @Test fun symlinkObjectAndSymlinkDirectoryAreRejected() {
        val attachment = saved()
        val blob = File(root, "${attachment.id}.bin")
        val outside = File(temporary, "outside").apply { writeBytes(blob.readBytes()) }
        blob.delete()
        Files.createSymbolicLink(blob.toPath(), outside.toPath())
        failure { files.wire(attachment) }
        val linkedRoot = File(temporary, "linked")
        Files.createSymbolicLink(linkedRoot.toPath(), root.toPath())
        val key = KeyGenerator.getInstance("AES").generateKey()
        failure { EncryptedAttachmentFiles(linkedRoot, { key }) {}.copyFrom(
            ByteArrayInputStream(byteArrayOf(1)), "fixture.bin", null, emptyList()) }
    }

    @Test fun diskReopenWithSameKeyWorksAndWrongKeyNeverReturnsWire() {
        val key = KeyGenerator.getInstance("AES").generateKey()
        val writer = EncryptedAttachmentFiles(root, { key }) {}
        val attachment = writer.copyFrom(ByteArrayInputStream(byteArrayOf(1, 2)), "fixture.bin", null, emptyList())
        val reopened = EncryptedAttachmentFiles(root, { key }) {}
        assertArrayEquals(byteArrayOf(1, 2), Base64.getDecoder().decode(reopened.wire(attachment).getString("data_base64")))
        val wrong = KeyGenerator.getInstance("AES").generateKey()
        failure { EncryptedAttachmentFiles(root, { wrong }) {}.wire(attachment) }
    }

    @Test fun namesAreSafeUtf8BoundedAndMimeMatchesNativeHundredByteLimit() {
        assertEquals("fixture_.txt", AttachmentPolicy.filename("C:\\private\\fixture\n.txt"))
        assertEquals("attachment", AttachmentPolicy.filename("../.."))
        val long = AttachmentPolicy.filename("😀".repeat(200))
        assertTrue(long.toByteArray(Charsets.UTF_8).size <= 255)
        assertFalse(long.contains('\uFFFD'))
        assertEquals("application/octet-stream", AttachmentPolicy.mime("x/" + "a".repeat(101)))
        assertEquals("application/octet-stream", AttachmentPolicy.mime("text/plain\r\ninjected"))
        assertEquals("image/png", AttachmentPolicy.mime("IMAGE/PNG"))
        failure { AttachmentPolicy.selection(listOf(metadata().copy(id = "../evil"))) }
        val same = metadata()
        failure { AttachmentPolicy.selection(listOf(same, same)) }
    }
}
