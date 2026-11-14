type ListEdit = { start: number; end: number; text: string };

/** Replace only the current selection (or an empty item's line), so the
 * browser can keep the newline and its prefix in one native undo transaction. */
export function listNewline(value: string, start: number, end: number): ListEdit | undefined {
  const lineStart = value.lastIndexOf("\n", start - 1) + 1;
  const nextLine = value.indexOf("\n", start);
  const lineEnd = nextLine < 0 ? value.length : nextLine;
  if (end > lineEnd) return;
  const line = value.slice(lineStart, lineEnd);
  const item = /^([ \t]*)(?:(\d{1,9})([.)])|([-+*]))([ \t]+)(\[[ xX]\][ \t]+)?/.exec(line);
  if (!item || start < lineStart + item[0].length) return;
  // Code examples must keep their literal contents. Track both Markdown fence
  // styles, including longer fences that may contain shorter ones.
  let fence: string | undefined;
  for (const previous of value.slice(0, lineStart).split("\n")) {
    const match = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(previous);
    if (!match) continue;
    if (!fence) fence = match[1];
    else if (match[1][0] === fence[0] && match[1].length >= fence.length && !match[2].trim()) fence = undefined;
  }
  if (fence) return;
  if (!line.slice(item[0].length).trim()) return lineStart > 0
    // Include the existing break in the native replacement. WebKit otherwise
    // drops the final empty line when only its marker is deleted.
    ? { start: lineStart - 1, end: lineEnd, text: `\n${item[1]}` }
    : { start: lineStart, end: lineEnd, text: item[1] };
  // Splitting before any item text should use the ordinary newline behavior.
  if (!value.slice(lineStart + item[0].length, start).trim()) return;
  const marker = item[2] ? `${Number(item[2]) + 1}${item[3]}` : item[4];
  return { start, end, text: `\n${item[1]}${marker}${item[5]}${item[6] ? "[ ] " : ""}` };
}

/** beforeinput runs only for an actual newline, after send/save shortcuts have
 * had their turn. Paste, IME, undo and ordinary typing retain native behavior. */
export function continueList(event: InputEvent & { currentTarget: HTMLTextAreaElement }) {
  const editor = event.currentTarget;
  if (event.defaultPrevented || !event.cancelable || event.isComposing || editor.readOnly || editor.disabled
    || !["insertLineBreak", "insertParagraph"].includes(event.inputType) || typeof document.execCommand !== "function") return;
  const start = editor.selectionStart, end = editor.selectionEnd, direction = editor.selectionDirection;
  const edit = listNewline(editor.value, start, end);
  if (!edit) return;
  editor.setSelectionRange(edit.start, edit.end);
  if (document.execCommand("insertText", false, edit.text)) event.preventDefault();
  else editor.setSelectionRange(start, end, direction);
}
