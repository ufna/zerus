package app.zerus.mobile

import android.os.SystemClock
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowLeft
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.*
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject

@Composable internal fun QuestionsPane(model: ZerusViewModel, target: Target, questions: List<Question>, available: Boolean,
    visible: Boolean, viewport: Rect?, maxHeight: Dp, onReview: (Draft) -> Unit) {
    var chosen by remember(target.key) { mutableStateOf("") }
    val pages = remember(target.key) { mutableStateMapOf<String, Int>() }
    val queue = remember(questions, target, chosen) { QuestionPolicies.queue(questions, target, chosen) }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        queue.submitted.forEach { question ->
            var show by remember(target.key, question.id, question.hash) { mutableStateOf(false) }
            Text("Answer submitted. Waiting for the agent to record it.", color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall)
            TextButton(onClick = { show = !show }) { Text(if (show) "Hide submitted answer" else "Show submitted answer") }
            if (show) NativeQuestionCard(model, target, question, false, visible, viewport, maxHeight,
                pages[QuestionPolicies.key(question)] ?: 0, { pages[QuestionPolicies.key(question)] = it }, onReview)
        }
        queue.selected?.let { question ->
            if (question.optional && queue.optional.size > 1) Row(Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = { chosen = QuestionPolicies.key(queue.optional[queue.index - 1]) }, enabled = queue.index > 0) { Text("Previous") }
                Text("Question ${queue.index + 1}/${queue.optional.size}", style = MaterialTheme.typography.labelMedium)
                TextButton(onClick = { chosen = QuestionPolicies.key(queue.optional[queue.index + 1]) }, enabled = queue.index + 1 < queue.optional.size) { Text("Next") }
            }
            NativeQuestionCard(model, target, question, available, visible, viewport, maxHeight,
                pages[QuestionPolicies.key(question)] ?: 0, { page ->
                    if (pages.size >= 64 && QuestionPolicies.key(question) !in pages) pages.keys.firstOrNull()?.let(pages::remove)
                    pages[QuestionPolicies.key(question)] = page
                }, onReview)
        }
    }
}

@Composable private fun NativeQuestionCard(model: ZerusViewModel, target: Target, question: Question, available: Boolean,
    visible: Boolean, viewport: Rect?, maxHeight: Dp, page: Int, onPage: (Int) -> Unit, onReview: (Draft) -> Unit) {
    val draft = model.draft(target, question)
    val native = QuestionPolicies.submitted(question, target)
    val saved = remember(draft.answers, native) {
        if (native != null) native.answers.associate { it.questionId to it }
        else runCatching { JSONArray(draft.answers).objects().associate { answer ->
            answer.string("question_id") to NativeQuestionAnswer(answer.string("question_id"), answer.optJSONArray("selected_option_ids")?.let { ids ->
                (0 until ids.length()).mapNotNull { ids.opt(it) as? String }
            }.orEmpty(), answer.string("text"), answer.optBoolean("skip"))
        } }.getOrDefault(emptyMap())
    }
    var options by remember(target.key, question.id, question.hash, native, draft.generation) {
        mutableStateOf(question.prompts.associate { it.id to saved[it.id]?.optionIds.orEmpty().toSet() })
    }
    var texts by remember(target.key, question.id, question.hash, native, draft.generation) {
        mutableStateOf(question.prompts.associate { it.id to saved[it.id]?.text.orEmpty() })
    }
    fun answers() = QuestionPolicies.answers(question.prompts, options, texts)
    fun persist() { model.edit(target, texts.values.filter { it.isNotBlank() }.joinToString("\n"), question, answers().toString()) }
    val editing = draft.status == "editing" && native == null
    val error = QuestionPolicies.answerError(question.prompts, options, texts)
    val complete = QuestionPolicies.complete(question.prompts, options, texts)
    val submitGate = remember(target.key,question.id,question.hash,draft.generation) { QuestionSubmitGate() }
    var submitPending by remember(submitGate) { mutableStateOf(false) }
    val submitScope = rememberCoroutineScope()
    val sendEnabled = question.canAnswer && editing && complete && available && !submitPending
    fun submitAnswer() {
        val allowed = question.canAnswer && editing && available && !submitPending &&
            model.draft(target,question).status == "editing" && QuestionPolicies.complete(question.prompts,options,texts)
        if(!submitGate.claim(allowed)) return
        submitPending = true
        persist()
        val job = model.send(target,question,answers())
        submitScope.launch { try { job.join() } finally { submitGate.finish();submitPending = false } }
    }
    val approvalSupported = QuestionPolicies.approvalSupported(question)
    val window = LocalWindowInfo.current
    val view = LocalView.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var foreground by remember(lifecycle) { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, _ -> foreground = lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }
    var cardInWindow by remember(target.key, question.id, question.hash) { mutableStateOf(false) }
    val reviewEligible = visible && LocalConversationObscuration.current?.obscured != true && cardInWindow && foreground && window.isWindowFocused && available && editing && question.canAnswer && approvalSupported
    var review by remember(target.key, question.id, question.hash) { mutableStateOf(ApprovalReview()) }
    var reviewNow by remember(target.key, question.id, question.hash) { mutableLongStateOf(SystemClock.elapsedRealtime()) }
    LaunchedEffect(reviewEligible, question.approval) {
        reviewNow = SystemClock.elapsedRealtime(); review = review.observe(reviewEligible, reviewNow)
        if (question.approval && reviewEligible) while (true) {
            delay(100); reviewNow = SystemClock.elapsedRealtime(); review = review.observe(true, reviewNow)
        }
    }
    val remaining = review.secondsRemaining(reviewNow)
    val pageIndex = page.coerceIn(0, (question.prompts.size - 1).coerceAtLeast(0))
    val currentPrompts = question.prompts.getOrNull(pageIndex)?.let(::listOf).orEmpty()
    val otherPrompts = currentPrompts.filter { it.other && !question.approval }
    val lowerActions = question.prompts.size > 1 || question.canSkip || otherPrompts.isEmpty()
    val density = LocalDensity.current
    var cardHeightPx by remember(question.id,question.hash,pageIndex) { mutableIntStateOf(0) }
    var bodyHeightPx by remember(question.id,question.hash,pageIndex) { mutableIntStateOf(0) }
    val initialReserve = 24.dp + 24.dp + 12.dp + 64.dp * otherPrompts.size + if(lowerActions) 48.dp else 0.dp
    val ordinaryReserve = if(cardHeightPx > bodyHeightPx && bodyHeightPx > 0) with(density) { (cardHeightPx-bodyHeightPx).toDp() } else initialReserve
    val approvalReserve = if(currentPrompts.any { it.other }) 156.dp else 100.dp
    val bodyHeight = (maxHeight - if(question.approval) approvalReserve else ordinaryReserve).coerceAtLeast(120.dp)
    val bodyScroll = remember(question.id, question.hash, pageIndex) { androidx.compose.foundation.ScrollState(0) }
    Card(colors = CardDefaults.cardColors(containerColor = Color(0xFF2B2923)), modifier = Modifier.onGloballyPositioned { coordinates ->
        val bounds = coordinates.boundsInWindow()
        val display = android.graphics.Rect(); view.getWindowVisibleDisplayFrame(display)
        val screen = IntArray(2); val inWindow = IntArray(2)
        view.getLocationOnScreen(screen); view.getLocationInWindow(inWindow)
        val dx = screen[0] - inWindow[0]; val dy = screen[1] - inWindow[1]
        cardInWindow = viewport != null && bounds.overlaps(viewport) && bounds.width > 0 && bounds.height > 0 && bounds.right + dx > display.left &&
            bounds.left + dx < display.right && bounds.bottom + dy > display.top && bounds.top + dy < display.bottom
    }) {
        Column(Modifier.onSizeChanged { if(!question.approval) cardHeightPx = it.height }.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(if (question.approval || question.trustRequest) "Needs approval" else if (question.optional) "Optional question" else "Needs input",
                    Modifier.weight(1f), color = Color(0xFFFFCB7D), style = MaterialTheme.typography.labelLarge)
                if (question.createdAt > 0) Text(java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.SHORT)
                    .format(java.util.Date((question.createdAt * 1000).toLong())), color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.labelSmall)
            }
            // Full disclosure scrolls; the answer field and compact controls stay visible.
            Box(Modifier.fillMaxWidth().onSizeChanged { if(!question.approval) bodyHeightPx = it.height }) {
            Column(Modifier.fillMaxWidth().heightIn(max = bodyHeight).verticalScroll(bodyScroll)
                .padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                currentPrompts.forEach { prompt ->
                    SelectionContainer { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (prompt.header.isNotBlank()) Text(prompt.header, style = MaterialTheme.typography.labelLarge)
                        Text(prompt.text, fontWeight = FontWeight.SemiBold)
                        if (prompt.body.isNotBlank()) Text(prompt.body, fontFamily = if (question.approval) FontFamily.Monospace else FontFamily.Default)
                    } }
                    if (prompt.otherDescription.isNotBlank()) SelectionContainer { Text(prompt.otherDescription, style = MaterialTheme.typography.bodySmall) }
                    prompt.choices.forEach { choice ->
                        if (question.approval) {
                            SelectionContainer { Text(choice.label, style = MaterialTheme.typography.bodySmall) }
                            if (choice.description.isNotBlank()) SelectionContainer { Text(choice.description, style = MaterialTheme.typography.bodySmall) }
                        } else QuestionOption(choice, prompt.multi, choice.id in options[prompt.id].orEmpty(), editing) {
                            val selected = options[prompt.id].orEmpty()
                            options = options + (prompt.id to if (prompt.multi) {
                                if (choice.id in selected) selected - choice.id else selected + choice.id
                            } else setOf(choice.id))
                            if (!prompt.multi) texts = texts + (prompt.id to "")
                            persist()
                        }
                    }
                }
            }
            if (bodyScroll.canScrollForward) Surface(color = Color(0xFF2B2923), modifier = Modifier.align(Alignment.BottomCenter)) {
                Text("Scroll for options ↓", Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                    color = Color(0xFFFFCB7D), style = MaterialTheme.typography.labelSmall)
            }
            }
            otherPrompts.forEachIndexed { index,prompt ->
                var focused by remember(target.key,question.id,question.hash,prompt.id) { mutableStateOf(false) }
                Row(Modifier.fillMaxWidth(),horizontalArrangement = Arrangement.spacedBy(8.dp),verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(texts[prompt.id].orEmpty(), { value ->
                    texts = texts + (prompt.id to value)
                    if (!prompt.multi && value.isNotBlank()) options = options + (prompt.id to emptySet())
                    persist()
                }, Modifier.weight(1f).onFocusChanged { focused = it.isFocused }.onPreviewKeyEvent { event ->
                    if(!focused || event.key !in setOf(Key.Enter,Key.NumPadEnter)) false
                    else {
                        if(event.type == KeyEventType.KeyDown && event.nativeKeyEvent.repeatCount == 0 &&
                            !event.isShiftPressed && !event.isCtrlPressed && !event.isAltPressed && !event.isMetaPressed) submitAnswer()
                        true // Consume key up/repeats too; the field must not generate a second IME action.
                    }
                }, label = { Text(prompt.otherLabel.ifBlank { if (question.prompts.size > 1) "Other: ${prompt.header.ifBlank { prompt.text }}" else "Other answer" }) },
                    singleLine = true, readOnly = !editing,
                    keyboardOptions = KeyboardOptions.Default.copy(imeAction = ImeAction.Send),
                    keyboardActions = KeyboardActions(onSend = { if(focused) submitAnswer() }))
                if(index == otherPrompts.lastIndex) QuestionSendButton(sendEnabled,question.prompts.size > 1,::submitAnswer)
                }
            }
            if (native != null) {
                Text("Answer submitted. Waiting for the agent to record it.", style = MaterialTheme.typography.bodySmall)
                native.answers.filter { it.skip }.forEach { Text("Skipped", style = MaterialTheme.typography.bodySmall) }
            } else if (draft.status != "editing") {
                Text("Answer delivery is ${draft.status}.", color = Color(0xFFFFCB7D))
                TextButton(onClick = { onReview(draft) }, enabled = draft.status != "submitting" && draft.key !in model.receiptFlights) { Text("Review delivery") }
            }
            if (error.isNotBlank()) Text(error, color = Color(0xFFFFCB7D), style = MaterialTheme.typography.bodySmall)
            if (question.approval) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    fun submit(option: String) { model.send(target, question, JSONArray().put(JSONObject()
                        .put("question_id", question.prompts.single().id).put("selected_option_ids", JSONArray().put(option)).put("text", ""))) }
                    OutlinedButton(onClick = { submit("deny") }, enabled = approvalSupported && question.canAnswer && editing && available) { Text("Deny") }
                    Button(onClick = { submit("allow") }, enabled = reviewEligible && remaining == 0) { Text(if (remaining > 0) "Approve once ($remaining)" else "Approve once") }
                }
                Text("Approval applies only to this command. Review it before continuing.", style = MaterialTheme.typography.bodySmall)
            } else if(lowerActions) Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
                if (question.prompts.size > 1) {
                    IconButton(onClick = { onPage(pageIndex - 1) }, enabled = pageIndex > 0, modifier = Modifier.size(32.dp)) {
                        Icon(Icons.Default.KeyboardArrowLeft, "Previous part")
                    }
                    Text("${pageIndex + 1}/${question.prompts.size}", style = MaterialTheme.typography.labelMedium)
                    IconButton(onClick = { onPage(pageIndex + 1) }, enabled = pageIndex + 1 < question.prompts.size, modifier = Modifier.size(32.dp)) {
                        Icon(Icons.Default.KeyboardArrowRight, "Next part")
                    }
                }
                Spacer(Modifier.weight(1f))
                if (question.canSkip) TextButton(onClick = { model.send(target, question, JSONArray(question.prompts.map {
                    JSONObject().put("question_id", it.id).put("skip", true).put("selected_option_ids", JSONArray()).put("text", "")
                })) }, enabled = editing && available, contentPadding = PaddingValues(horizontal = 8.dp)) { Text("Skip") }
                if(otherPrompts.isEmpty()) QuestionSendButton(sendEnabled,question.prompts.size > 1,::submitAnswer)
            }
            if (!available) Text("Refresh to verify this request. Your answer is kept.", style = MaterialTheme.typography.bodySmall)
            else if (!question.canAnswer) SelectionContainer { Text(question.unavailableReason.ifBlank { "Complete this request on your machine." }, style = MaterialTheme.typography.bodySmall) }
            if (question.approval && !approvalSupported) Text("This approval cannot be answered here. Continue on your machine.", style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable private fun QuestionSendButton(enabled: Boolean,multiple: Boolean,onClick: () -> Unit) {
    FilledIconButton(onClick = onClick,enabled = enabled,modifier = Modifier.size(44.dp),shape = CircleShape,
        colors = IconButtonDefaults.filledIconButtonColors(containerColor = Color(0xFF67E8CB),contentColor = Color(0xFF101517),
            disabledContainerColor = Color(0xFF2B3B3B),disabledContentColor = Color(0xFF9AACB2))) {
        Icon(Icons.AutoMirrored.Filled.Send,if(multiple) "Send all" else "Send answer",Modifier.size(22.dp))
    }
}

@Composable private fun QuestionOption(choice: Choice, multi: Boolean, selected: Boolean, enabled: Boolean, onClick: () -> Unit) {
    val select = if (multi) Modifier.toggleable(selected, enabled, Role.Checkbox) { onClick() }
        else Modifier.selectable(selected, enabled, Role.RadioButton, onClick)
    Surface(color = MaterialTheme.colorScheme.surface, shape = MaterialTheme.shapes.small,
        modifier = Modifier.fillMaxWidth().semantics(mergeDescendants = true) {}.then(select)) {
        Row(Modifier.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
            if (multi) Checkbox(selected, null) else RadioButton(selected, null)
            Column(Modifier.padding(start = 8.dp)) {
                Text(choice.label)
                if (choice.description.isNotBlank()) SelectionContainer { Text(choice.description, style = MaterialTheme.typography.bodySmall) }
            }
        }
    }
}
