package app.zerus.mobile

import kotlinx.coroutines.*
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.IOException

class MachineColorsTest {
    private val key = MachineKey("workspace", "node")

    @Test fun paletteAndAutomaticHashMatchDesktopAndIgnoreEditableNames() {
        assertEquals(listOf("#5fbfa5", "#8ea9f4", "#c098e6", "#e7ae66", "#e38f9e", "#75bfcf", "#acc577"), MachineColors.palette)
        assertEquals("#c098e6", MachineColors.automatic(key))
        assertEquals("#e38f9e", MachineColors.automatic(MachineKey("workspace", "550e8400-e29b-41d4-a716-446655440000")))
        assertEquals("#5fbfa5", MachineColors.automatic(MachineKey("workspace", "🖥️")))
        val before = MachineColors.color(MessageState(), key)
        val renamed = MachineNames.set(MessageState(), key, "A different label")
        assertEquals(before, MachineColors.color(renamed, key))
        assertEquals(before, MachineColors.color(MessageCodec.state(MessageCodec.stateJson(renamed)), key))
    }

    @Test fun desktopTonalBlendsHaveOpaqueReadableForegroundAndBackground() {
        assertEquals(0xff9cd7c7.toInt(), MachineColors.foreground("#5fbfa5", true))
        assertEquals(0xff274141.toInt(), MachineColors.background("#5fbfa5", true))
        assertEquals(0xff3a6f65.toInt(), MachineColors.foreground("#5fbfa5", false))
        assertEquals(0xffe4f4f0.toInt(), MachineColors.background("#5fbfa5", false))
    }

    @Test fun overridesAreExactWorkspaceAndComputerScopedAndResetKeepsOtherProfiles() {
        val other = MachineKey("other-workspace", "node")
        val state = MachineColors.set(MachineColors.set(MessageState(), other, "#e7ae66"), key, "#8EA9F4")
        val restored = MessageCodec.state(MessageCodec.stateJson(state))
        assertEquals("#8ea9f4", MachineColors.color(restored, key))
        assertEquals("#e7ae66", MachineColors.color(restored, other))
        assertNull(MachineColors.overrideHex(restored, MachineKey("workspace", "another-node")))
        val reset = MachineColors.set(restored, key, null)
        assertNull(MachineColors.overrideHex(reset, key))
        assertEquals(MachineColors.automatic(key), MachineColors.color(reset, key))
        assertEquals("#e7ae66", MachineColors.overrideHex(reset, other))
    }

    @Test fun malformedOrLegacyAppearanceCannotDamagePrivateConversationState() {
        assertTrue(MessageCodec.state(JSONObject()).machineColors.isEmpty())
        for (hex in listOf("red", "#123", "#12345678", "#12gg34", " #5fbfa5", "#٥fbfa5")) {
            assertThrows(IllegalArgumentException::class.java) { MachineColors.checked(hex) }
        }
        val value = JSONObject().put("machine_colors", JSONArray().put("invalid").put(JSONObject()
            .put("connection", "workspace").put("machine", 123).put("color", "#5fbfa5")))
        assertTrue(MessageCodec.state(value).machineColors.isEmpty())
        val target = Target("node", "session", "run", "conversation", "workspace")
        val draft = Draft(target, "Keep this text")
        val receipt = OutgoingMessage("request", target, "Submitted", emptyList(), 1.0, "uncertain")
        val state = MachineColors.set(MachineNames.set(MessageState(listOf(draft), listOf(receipt)), key, "Alias"), key, "#c098e6")
        assertEquals(draft, state.drafts.single()); assertEquals(receipt, state.outgoing.single())
        assertEquals("Alias", state.machineAliases.single().name)
        assertEquals(target, state.outgoing.single().target)
    }

    @Test fun offlineOrderedSavePreservesConcurrentTypingAndOtherAppearanceEdits() = runBlocking {
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        try {
            val target = Target("node", "session", "run", "conversation", "workspace")
            val draft = Draft(target, "Before")
            val entered = CompletableDeferred<Unit>(); val release = CompletableDeferred<Unit>()
            val actor = OrderedStatePersistence(MessageState(listOf(draft)), scope,
                { entered.complete(Unit); release.await() }, Dispatchers.Unconfined)
            val save = async { actor.durable({ MachineColors.set(it, key, "#e38f9e") },
                { current, _ -> MachineColors.set(current, key, "#e38f9e") }) }
            entered.await()
            actor.edit { MachineNames.set(it.copy(drafts = listOf(draft.copy(text = "Typed while saving"))), key, "New name") }
            release.complete(Unit); save.await(); actor.flush()
            val persisted = MessageCodec.state(MessageCodec.stateJson(actor.durableState))
            assertEquals("Typed while saving", persisted.drafts.single().text)
            assertEquals("New name", persisted.machineAliases.single().name)
            assertEquals("#e38f9e", MachineColors.color(persisted, key))
            val failed = OrderedStatePersistence(MessageState(), scope, { throw IOException("synthetic failure") }, Dispatchers.Unconfined)
            assertTrue(runCatching { failed.durable({ MachineColors.set(it, key, "#acc577") }) }.isFailure)
            assertNull(MachineColors.overrideHex(failed.state.value, key))
        } finally { scope.cancel() }
    }
}
