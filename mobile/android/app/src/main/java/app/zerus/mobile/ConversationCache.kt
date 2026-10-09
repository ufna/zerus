package app.zerus.mobile

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.system.Os
import android.system.OsConstants
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.StandardCopyOption
import java.nio.file.attribute.PosixFilePermission
import java.security.KeyStore
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Presentation history only. Cache data never verifies permission to send. */
class ConversationCache(context: Context) {
    private val root = File(context.applicationContext.noBackupFilesDir.canonicalFile, "conversation-cache-v1")
    private val files = ConversationCacheEngines.get(root) {
        EncryptedConversationFiles(root, ::key, syncDirectory = {
            val fd = Os.open(root.absolutePath, OsConstants.O_RDONLY, 0)
            try { Os.fsync(fd) } finally { Os.close(fd) }
        })
    }

    private fun key(): SecretKey = synchronized(KEY_LOCK) {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(KEY_ALIAS, null) as? SecretKey) ?: KeyGenerator.getInstance("AES", "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true).build())
        }.generateKey()
    }

    /** Immutable bytes already in memory; no filesystem or JSON work on the caller thread. */
    fun getMemory(target: Target): String? = files.getMemory(target)
    suspend fun read(target: Target): JSONObject? = withContext(Dispatchers.IO) { files.read(target)?.let(::JSONObject) }
    suspend fun write(target: Target, raw: JSONObject, observedAt: Long = System.currentTimeMillis()) {
        val generation = files.generation(target.connectionId)
        withContext(Dispatchers.IO) { files.write(target, raw, observedAt, generation) }
    }
    suspend fun purgeWorkspace(connectionId: String) {
        files.invalidate(connectionId)
        withContext(NonCancellable + Dispatchers.IO) { files.purgeWorkspace(connectionId) }
    }
    suspend fun clear() {
        files.invalidate(null)
        withContext(NonCancellable + Dispatchers.IO) { files.clear() }
    }
    companion object {
        private const val KEY_ALIAS = "zerus_conversation_cache_v1"
        private val KEY_LOCK = Any()
    }
}

/** ViewModel replacement shares memory and invalidation, not just the disk lock. */
internal object ConversationCacheEngines {
    private val engines = ConcurrentHashMap<String, EncryptedConversationFiles>()
    fun get(root: File, create: () -> EncryptedConversationFiles): EncryptedConversationFiles =
        engines.computeIfAbsent(root.canonicalPath) { create() }
}

/** Injectable disk engine used by JVM tests with software keys and synthetic histories. */
internal class EncryptedConversationFiles(
    private val root: File, private val key: () -> SecretKey,
    private val now: () -> Long = System::currentTimeMillis,
    private val maxEntries: Int = 20, private val maxBytes: Long = 20L * 1024 * 1024,
    private val maxRawBytes: Int = 1024 * 1024, private val ttlMillis: Long = 7L * 24 * 60 * 60 * 1000,
    private val syncDirectory: () -> Unit = {}
) {
    private data class Entry(val target: Target, val raw: String, val storedAt: Long, val observedAt: Long, val bytes: Int)
    private val memoryLock = Any()
    private val diskLock = DIRECTORY_LOCKS.computeIfAbsent(root.canonicalPath) { Any() }
    private val memory = LinkedHashMap<String, Entry>(20, .75f, true)
    private val generations = mutableMapOf<String, Long>()
    private val disconnected = mutableSetOf<String>()
    private var globalGeneration = 0L
    private var pendingClears = 0

    fun generation(connection: String): Pair<Long, Long> = synchronized(memoryLock) {
        globalGeneration to (generations[connection] ?: 0L)
    }
    fun invalidate(connection: String?) = synchronized(memoryLock) {
        if (connection == null) { globalGeneration++; pendingClears++; memory.clear() }
        else {
            disconnected += connection
            generations[connection] = (generations[connection] ?: 0L) + 1
            memory.entries.removeAll { it.value.target.connectionId == connection }
        }
    }
    fun getMemory(target: Target): String? = synchronized(memoryLock) {
        if (pendingClears > 0 || target.connectionId in disconnected) null
        else memory[target.key]?.takeIf { validAge(it.storedAt) }?.raw
    }
    private fun validAge(at: Long) = at > 0 && at <= now() && now() - at <= ttlMillis
    private fun identity(target: Target, raw: JSONObject): Boolean =
        target.connectionId.isNotBlank() && target.computerId.isNotBlank() && target.session.isNotBlank() &&
        target.key.toByteArray(Charsets.UTF_8).size <= 16 * 1024 &&
        raw.string("name") == target.session && raw.string("run_id") == target.run &&
        raw.string("conversation_id") == target.conversation && raw.string("archive_id") == target.archiveId &&
        (target.archiveId.isNotBlank() || raw.string("state") != "archived") &&
        raw.string("agent_id") == target.agentId && (target.agentId.isBlank() || raw.string("parent_conversation_id") == target.parentConversation)
    private fun name(target: Target) = MessageDigest.getInstance("SHA-256")
        .digest(target.key.toByteArray(Charsets.UTF_8)).joinToString("") { "%02x".format(it) } + ".bin"
    private fun ensureRoot() {
        if (!root.exists()) {
            check(root.mkdirs()) { "Cannot create history cache" }
            Files.setPosixFilePermissions(root.toPath(), setOf(PosixFilePermission.OWNER_READ, PosixFilePermission.OWNER_WRITE, PosixFilePermission.OWNER_EXECUTE))
        }
        check(Files.isDirectory(root.toPath(), LinkOption.NOFOLLOW_LINKS)) { "Invalid history cache directory" }
        root.listFiles()?.filter { it.name.startsWith(".") && it.name.endsWith(".part") }?.forEach { it.delete() }
    }
    private fun objects(): List<File> = root.listFiles()?.filter { it.name.matches(Regex("[0-9a-f]{64}\\.bin")) }.orEmpty()
    private fun decode(file: File): Entry? = runCatching {
        require(Files.isRegularFile(file.toPath(), LinkOption.NOFOLLOW_LINKS))
        require(file.length() in 29..(maxRawBytes.toLong() + 65536))
        val bytes = file.readBytes()
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes.copyOfRange(0, 12)))
        cipher.updateAAD(file.name.toByteArray(Charsets.UTF_8))
        val data = JSONObject(String(cipher.doFinal(bytes.copyOfRange(12, bytes.size)), Charsets.UTF_8))
        require(data.getInt("version") == 1)
        val target = MessageCodec.target(data.getJSONObject("target"))
        val raw = data.getJSONObject("raw")
        require(name(target) == file.name && identity(target, raw))
        val serialized = raw.toString()
        require(serialized.toByteArray(Charsets.UTF_8).size <= maxRawBytes)
        val entry = Entry(target, serialized, data.getLong("stored_at"), data.getLong("observed_at"), serialized.toByteArray(Charsets.UTF_8).size)
        require(validAge(entry.storedAt) && entry.observedAt > 0 && entry.observedAt <= entry.storedAt)
        entry
    }.getOrNull()
    private fun remember(entry: Entry, captured: Pair<Long, Long>): Boolean = synchronized(memoryLock) {
        if (pendingClears > 0 || entry.target.connectionId in disconnected || captured != generation(entry.target.connectionId)) return@synchronized false
        val old = memory[entry.target.key]
        if (old == null || old.observedAt <= entry.observedAt) memory[entry.target.key] = entry
        while (memory.size > maxEntries || memory.values.sumOf { it.bytes.toLong() } > maxBytes) {
            memory.remove(memory.keys.first())
        }
        true
    }
    fun read(target: Target): String? = synchronized(diskLock) {
        val generation = generation(target.connectionId)
        if (synchronized(memoryLock) { pendingClears > 0 || target.connectionId in disconnected }) return@synchronized null
        getMemory(target)?.let { return@synchronized it }
        if (runCatching { ensureRoot() }.isFailure) return@synchronized null
        prune()
        val file = File(root, name(target))
        val entry = decode(file)
        if (entry == null || entry.target != target) { file.delete(); return@synchronized null }
        if (remember(entry, generation)) entry.raw else null
    }
    fun write(target: Target, raw: JSONObject, observedAt: Long = now(), generation: Pair<Long, Long> = generation(target.connectionId)) = synchronized(diskLock) {
        if (synchronized(memoryLock) { pendingClears > 0 || target.connectionId in disconnected } ||
            generation != generation(target.connectionId) || !identity(target, raw)) return@synchronized
        val serialized = raw.toString()
        if (serialized.toByteArray(Charsets.UTF_8).size > maxRawBytes || observedAt <= 0 || observedAt > now()) return@synchronized
        ensureRoot()
        val file = File(root, name(target))
        val previous = decode(file)
        if (previous != null && previous.observedAt > observedAt) { remember(previous, generation); return@synchronized }
        val entry = Entry(target, serialized, now(), observedAt, serialized.toByteArray(Charsets.UTF_8).size)
        val envelope = JSONObject().put("version", 1).put("target", MessageCodec.targetJson(target))
            .put("raw", JSONObject(serialized)).put("stored_at", entry.storedAt).put("observed_at", observedAt)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        cipher.updateAAD(file.name.toByteArray(Charsets.UTF_8))
        val encrypted = cipher.iv + cipher.doFinal(envelope.toString().toByteArray(Charsets.UTF_8))
        if (encrypted.size > maxBytes || encrypted.size > maxRawBytes.toLong() + 65536) return@synchronized
        val temporary = File(root, ".${UUID.randomUUID()}.part")
        try {
            Files.createFile(temporary.toPath())
            Files.setPosixFilePermissions(temporary.toPath(), setOf(PosixFilePermission.OWNER_READ, PosixFilePermission.OWNER_WRITE))
            FileOutputStream(temporary).use { it.write(encrypted); it.fd.sync() }
            if (generation != generation(target.connectionId)) return@synchronized
            Files.move(temporary.toPath(), file.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            file.setLastModified(now())
            syncDirectory()
            remember(entry, generation)
            prune()
        } finally { temporary.delete() }
    }
    private fun prune() {
        synchronized(memoryLock) { memory.entries.removeAll { !validAge(it.value.storedAt) } }
        val rows = objects().mapNotNull { file ->
            // Disk age is the last confirmed inspection, never a presentation
            // read. Scan small filesystem metadata instead of decrypting 20 MiB.
            if (!Files.isRegularFile(file.toPath(), LinkOption.NOFOLLOW_LINKS) ||
                file.length() !in 29..(maxRawBytes.toLong() + 65536) || !validAge(file.lastModified())) {
                file.delete(); null
            } else file
        }.sortedBy { it.lastModified() }.toMutableList()
        var bytes = rows.sumOf { it.length() }
        while (rows.size > maxEntries || bytes > maxBytes) {
            val file = rows.removeAt(0); bytes -= file.length(); file.delete()
            synchronized(memoryLock) { memory.entries.removeAll { name(it.value.target) == file.name } }
        }
    }
    fun purgeWorkspace(connection: String) = synchronized(diskLock) {
        ensureRoot()
        objects().forEach { file -> if (decode(file)?.target?.connectionId.let { it == null || it == connection }) file.delete() }
        synchronized(memoryLock) { memory.entries.removeAll { it.value.target.connectionId == connection } }
        syncDirectory()
    }
    fun clear() = synchronized(diskLock) {
        ensureRoot(); root.listFiles()?.forEach { it.delete() }
        synchronized(memoryLock) { memory.clear(); if (pendingClears > 0) pendingClears-- }
        syncDirectory()
    }
    companion object {
        private val DIRECTORY_LOCKS = ConcurrentHashMap<String, Any>()
    }
}
