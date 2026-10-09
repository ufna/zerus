package app.zerus.mobile

import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import java.io.File
import java.nio.file.Files
import java.nio.file.attribute.PosixFilePermission
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

class ConversationCacheTest {
    private lateinit var temporary: File
    private lateinit var root: File
    private lateinit var key: SecretKey
    private lateinit var cache: EncryptedConversationFiles
    private var clock = 1_000_000L
    private val target = Target("computer", "session/name", "run", "conversation", "workspace")
    private fun raw(destination: Target = target, text: String = "Synthetic private history") = JSONObject()
        .put("name", destination.session).put("run_id", destination.run).put("conversation_id", destination.conversation)
        .also { if (destination.archiveId.isNotBlank()) it.put("archive_id", destination.archiveId).put("state", "archived") }
        .put("events", JSONArray().put(JSONObject().put("type", "UserPromptSubmit").put("seq", 1).put("at", 1).put("detail", text)))
    private fun engine(entries: Int = 20, bytes: Long = 20L * 1024 * 1024, rawBytes: Int = 1024 * 1024, ttl: Long = 7L * 86400 * 1000) =
        EncryptedConversationFiles(root, { key }, { clock }, entries, bytes, rawBytes, ttl)

    @Before fun setup() {
        temporary = Files.createTempDirectory("zerus-history-cache-test-").toFile()
        root = File(temporary, "cache")
        key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
        cache = engine()
    }
    @After fun cleanup() { temporary.deleteRecursively() }

    @Test fun encryptedAtomicHistoryRestoresAfterRestartAndMemoryIsImmutable() {
        val history = raw(); cache.write(target, history)
        history.put("name", "changed")
        assertEquals(target.session, JSONObject(cache.getMemory(target)!!).getString("name"))
        val blob = root.listFiles()!!.single()
        assertFalse(String(blob.readBytes(), Charsets.ISO_8859_1).contains("Synthetic private history"))
        assertEquals(setOf(PosixFilePermission.OWNER_READ, PosixFilePermission.OWNER_WRITE), Files.getPosixFilePermissions(blob.toPath()))
        val reopened = engine()
        assertNull(reopened.getMemory(target))
        assertEquals(raw().toString(), reopened.read(target))
        assertEquals(raw().toString(), reopened.getMemory(target))
    }
    @Test fun workspaceComputerNameRunConversationAndArchiveNeverAlias() {
        cache.write(target, raw())
        listOf(target.copy(connectionId = "other"), target.copy(computerId = "other"), target.copy(session = "other"),
            target.copy(run = "other"), target.copy(conversation = "other"), target.copy(archiveId = "55555555-5555-4555-8555-555555555555")).forEach {
            assertNull(cache.getMemory(it)); assertNull(cache.read(it))
        }
        val archive = target.copy(archiveId = "55555555-5555-4555-8555-555555555555")
        cache.write(archive, raw(archive, "Archive"))
        assertNotEquals(cache.read(target), cache.read(archive))
    }
    @Test fun mismatchedReturnedIdentityIsNeverAdmitted() {
        listOf(raw().put("name", "other"), raw().put("run_id", "other"), raw().put("conversation_id", "other"),
            raw().put("archive_id", "other"), raw().put("state", "archived"), raw().removeField("name")).forEach {
            cache.write(target, it); assertNull(cache.getMemory(target))
        }
        assertFalse(root.exists())
    }
    @Test fun expiredAndFutureDatedEntriesAreIgnored() {
        cache = engine(ttl = 100)
        cache.write(target, raw()); clock += 101
        assertNull(cache.getMemory(target)); assertNull(engine(ttl = 100).read(target))
        assertEquals(0, root.listFiles()!!.size)
        cache.write(target, raw()); clock--
        assertNull(cache.getMemory(target)); assertNull(engine(ttl = 100).read(target))
    }
    @Test fun corruptTruncatedWrongKeyAndFileSwapNeverReturnHistory() {
        cache.write(target, raw()); val file = root.listFiles()!!.single(); val encrypted = file.readBytes()
        file.writeBytes(encrypted.copyOf(20)); assertNull(engine().read(target))
        cache.write(target, raw()); val bytes = file.readBytes(); bytes[bytes.lastIndex] = (bytes.last().toInt() xor 1).toByte()
        file.writeBytes(bytes); assertNull(engine().read(target))
        cache.write(target, raw())
        val wrong = KeyGenerator.getInstance("AES").generateKey()
        assertNull(EncryptedConversationFiles(root, { wrong }, { clock }).read(target))
        cache.write(target, raw())
        val second = target.copy(run = "second"); cache.write(second, raw(second))
        val other = root.listFiles()!!.single { it != file }; file.writeBytes(other.readBytes())
        assertNull(engine().read(target))
    }
    @Test fun entryAndByteBudgetsEvictOldestConfirmedFilesAndBoundMemory() {
        cache = engine(entries = 2)
        val second = target.copy(run = "second"); val third = target.copy(run = "third")
        cache.write(target, raw()); clock++; cache.write(second, raw(second)); clock++
        engine(entries = 2).read(target); clock++; cache.write(third, raw(third))
        assertEquals(2, root.listFiles()!!.size)
        assertNull(engine(entries = 2).read(target)); assertNull(cache.getMemory(target))
        cache.clear(); cache.invalidate(null)
        cache = engine(bytes = 1100)
        repeat(8) { index -> clock++; val destination = target.copy(run = "run-$index"); cache.write(destination, raw(destination)) }
        assertTrue(root.listFiles()!!.sumOf { it.length() } <= 1100)
        assertTrue(root.listFiles()!!.size < 8)
    }
    @Test fun utf8RawLimitAndOverBudgetObjectCannotPublish() {
        cache = engine(rawBytes = 1024)
        cache.write(target, raw(text = "λ".repeat(600)))
        assertNull(cache.getMemory(target)); assertFalse(root.exists())
        cache = engine(bytes = 100)
        cache.write(target, raw()); assertNull(cache.getMemory(target)); assertEquals(0, root.listFiles()!!.size)
    }
    @Test fun outOfDateWritesCannotReplaceNewerConfirmedHistoryEvenAfterRestart() {
        cache.write(target, raw(text = "Newer"), clock)
        clock += 10
        engine().write(target, raw(text = "Older"), clock - 11)
        assertTrue(engine().read(target)!!.contains("Newer"))
        cache.write(target, raw(text = "Newer still"), clock)
        cache.write(target, raw(text = "Older"), clock - 1)
        assertTrue(cache.getMemory(target)!!.contains("Newer still"))
    }
    @Test fun disconnectPurgesOnlyOwnWorkspaceAndInvalidatesPendingWrites() {
        val other = target.copy(connectionId = "another")
        cache.write(target, raw()); cache.write(other, raw(other))
        val pending = cache.generation(target.connectionId)
        cache.invalidate(target.connectionId); cache.purgeWorkspace(target.connectionId)
        cache.write(target, raw(), clock, pending)
        assertNull(cache.getMemory(target)); assertNull(engine().read(target))
        assertNotNull(engine().read(other))
        val old = cache.generation(other.connectionId)
        cache.invalidate(null); cache.clear(); cache.write(other, raw(other), clock, old)
        assertNull(cache.getMemory(other)); assertEquals(0, root.listFiles()!!.size)
    }
    @Test fun readAfterInvalidateBeforeQueuedPurgeCannotRestoreHistory() {
        cache.write(target, raw())
        cache.invalidate(target.connectionId)
        assertNull(cache.read(target)); assertNull(cache.getMemory(target))
        cache.write(target, raw(text = "Late network callback"))
        assertNull(cache.getMemory(target))
        cache.purgeWorkspace(target.connectionId)
        assertNull(engine().read(target))
    }
    @Test fun readAfterGlobalInvalidateBeforeQueuedClearCannotRestoreHistory() {
        cache.write(target, raw()); cache.invalidate(null)
        assertNull(cache.read(target)); assertNull(cache.getMemory(target))
        cache.write(target, raw(text = "Late callback during clear"))
        assertNull(cache.getMemory(target))
        cache.clear(); assertNull(engine().read(target))
    }
    @Test fun viewModelReplacementSharesInvalidationAndRejectsOldInstanceWritesAfterPurge() {
        val original = ConversationCacheEngines.get(root) { engine() }
        original.write(target, raw())
        val oldPending = original.generation(target.connectionId)
        val replacement = ConversationCacheEngines.get(File(root.parentFile, "./" + root.name)) { engine() }
        assertSame(original, replacement)
        replacement.invalidate(target.connectionId); replacement.purgeWorkspace(target.connectionId)
        original.write(target, raw(text = "Old instance late callback"), clock, oldPending)
        original.write(target, raw(text = "Old instance freshly captured generation"))
        assertNull(original.getMemory(target)); assertNull(replacement.read(target)); assertNull(engine().read(target))
    }
    @Test fun invalidationAfterAtomicRenameCannotResurrectMemoryForDisconnectOrClear() {
        listOf(false, true).forEach { globally ->
            lateinit var raced: EncryptedConversationFiles
            var invalidate = true
            raced = EncryptedConversationFiles(root, { key }, { clock }, syncDirectory = {
                if (invalidate) { invalidate = false; raced.invalidate(if (globally) null else target.connectionId) }
            })
            raced.write(target, raw())
            assertNull(raced.getMemory(target))
            if (globally) raced.clear() else raced.purgeWorkspace(target.connectionId)
            assertNull(engine().read(target))
        }
    }
    @Test fun symlinkObjectIsRejectedAndCrashPartIsRemoved() {
        cache.write(target, raw()); val file = root.listFiles()!!.single()
        val outside = File(temporary, "outside").apply { writeBytes(file.readBytes()) }
        file.delete(); Files.createSymbolicLink(file.toPath(), outside.toPath())
        assertNull(engine().read(target)); assertTrue(outside.exists())
        File(root, ".synthetic.part").writeText("unfinished")
        engine().read(target)
        assertEquals(0, root.listFiles()!!.size)
    }
    private fun JSONObject.removeField(name: String): JSONObject = apply { remove(name) }
}
