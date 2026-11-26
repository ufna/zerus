import type { Tokens } from "marked";
import { lexMarkdown } from "../sessions/markdown";
import { textComposing } from "./text-edit";

type ListEdit = { start: number; end: number; text: string };

// List continuation is optional. Keep a native newline when the fence context
// or the current line extends past this synchronous input budget.
const MAX_LIST_CONTEXT = 4096;

/** Lex through the whole current line. Its innermost final block belongs to
 * the containing list item even when the fence begins on the marker line. */
function inFence(prefix: string): boolean {
  if (!/(?:`{3}|~{3})/.test(prefix)) return false;
  try {
    const tokens = lexMarkdown(prefix);
    if (!tokens) return true;
    let token = tokens.at(-1);
    while (token?.type === "list" || token?.type === "blockquote") {
      token = token.type === "list"
        ? (token as Tokens.List).items.at(-1)?.tokens.at(-1)
        : (token as Tokens.Blockquote).tokens.at(-1);
    }
    return token?.type === "code" && token.codeBlockStyle !== "indented";
  } catch {
    // An unclassified line keeps the browser's ordinary newline.
    return true;
  }
}

/** Replace only the current selection (or an empty item's line), so the
 * browser can keep the newline and its prefix in one native undo transaction. */
export function listNewline(value: string, start: number, end: number): ListEdit | undefined {
  if (start === 0 || end > MAX_LIST_CONTEXT) return;
  const context = value.slice(0, MAX_LIST_CONTEXT + 1);
  const lineStart = context.lastIndexOf("\n", start - 1) + 1;
  const nextLine = context.indexOf("\n", start);
  const lineEnd = nextLine < 0 ? context.length : nextLine;
  if (end > lineEnd || lineEnd > MAX_LIST_CONTEXT) return;
  const line = context.slice(lineStart, lineEnd);
  const item = /^([ \t]*)(?:(\d{1,9})([.)])|([-+*]))([ \t]+)(\[[ xX]\][ \t]+)?/.exec(line);
  if (!item || start < lineStart + item[0].length) return;
  if (/^[ \t]*(?:(?:-[ \t]*){3,}|(?:\*[ \t]*){3,})$/.test(line)) return;
  if (inFence(context.slice(0, lineEnd))) return;
  if (!line.slice(item[0].length).trim()) return lineStart > 0
    // Include the existing break in the native replacement. WebKit otherwise
    // drops the final empty line when only its marker is deleted.
    ? { start: lineStart - 1, end: lineEnd, text: `\n${item[1]}` }
    : { start: lineStart, end: lineEnd, text: item[1] };
  // Splitting before any item text should use the ordinary newline behavior.
  if (!line.slice(item[0].length, start - lineStart).trim()) return;
  const marker = item[2] ? `${Number(item[2]) + 1}${item[3]}` : item[4];
  return { start, end, text: `\n${item[1]}${marker}${item[5]}${item[6] ? "[ ] " : ""}` };
}

/** beforeinput runs only for an actual newline, after send/save shortcuts have
 * had their turn. Paste, IME, undo and ordinary typing retain native behavior. */
export function continueList(event: InputEvent & { currentTarget: HTMLTextAreaElement }) {
  const editor = event.currentTarget;
  const document = editor.ownerDocument;
  if (event.defaultPrevented || !event.cancelable || event.isComposing || textComposing(editor)
    || editor.readOnly || editor.disabled || document.activeElement !== editor
    || !["insertLineBreak", "insertParagraph"].includes(event.inputType) || typeof document.execCommand !== "function") return;
  const start = editor.selectionStart, end = editor.selectionEnd, direction = editor.selectionDirection;
  if (end > MAX_LIST_CONTEXT) return;
  const edit = listNewline(editor.value, start, end);
  if (!edit) return;
  if (edit.start !== start || edit.end !== end) editor.setSelectionRange(edit.start, edit.end);
  let accepted = false;
  try {
    accepted = edit.text ? document.execCommand("insertText", false, edit.text) : document.execCommand("delete");
  } catch {
    // A webview that rejects the command can still perform its own newline.
  }
  if (accepted) event.preventDefault();
  else editor.setSelectionRange(start, end, direction);
}
