import { afterEach, describe, expect, it, vi } from "vitest";
import { Window } from "happy-dom";
import { continueList, listNewline } from "./list-edit";

function newline(value: string, start = value.length, end = start) {
  const edit = listNewline(value, start, end);
  return edit ? value.slice(0, edit.start) + edit.text + value.slice(edit.end) : undefined;
}

describe("list newlines", () => {
  it.each([
    ["1. first", "1. first\n2. "],
    ["9) ninth", "9) ninth\n10) "],
    ["Intro\n  12. nested", "Intro\n  12. nested\n  13. "],
    ["- item", "- item\n- "],
    ["\t* item", "\t* item\n\t* "],
    ["+ item", "+ item\n+ "],
    ["- [x] done", "- [x] done\n- [ ] "],
    ["2. [X] done", "2. [X] done\n3. [ ] "],
    ["1. first\n2. ", "1. first\n"],
    ["  - [ ] ", "  "],
    ["* \t", ""],
    ["```md\n1. literal\n```\n1. real", "```md\n1. literal\n```\n1. real\n2. "],
  ])("continues or ends %j", (before, after) => expect(newline(before)).toBe(after));

  it.each(["ordinary text", "version 1. text", "1.no space", "-word", "---", "```md\n1. code", "~~~~\n~~~\n- code", "```\n~~~\n1. code"])("leaves %j to native editing", value => {
    expect(newline(value)).toBeUndefined();
  });

  it("splits item text at the caret and replaces only a same-line selection", () => {
    expect(newline("1. first second\nother", 8)).toBe("1. first\n2.  second\nother");
    expect(newline("1. first selected rest", 8, 17)).toBe("1. first\n2.  rest");
    expect(newline("1. first\n2. second", 8, 12)).toBeUndefined();
    expect(newline("1. first", 0)).toBeUndefined();
    expect(newline("1. first", 3)).toBeUndefined();
  });
});

const dom = new Window();
vi.stubGlobal("document", dom.document);
const command = vi.fn();
Object.assign(dom.document, { execCommand: command });
afterEach(() => { dom.document.body.replaceChildren(); command.mockReset(); });

function field(value: string) {
  const editor = dom.document.createElement("textarea") as unknown as HTMLTextAreaElement;
  document.body.append(editor); editor.value = value; editor.focus(); editor.setSelectionRange(value.length, value.length);
  editor.addEventListener("beforeinput", event => continueList(event as InputEvent & { currentTarget: HTMLTextAreaElement }));
  return editor;
}
function input(editor: HTMLTextAreaElement, init: Pick<InputEventInit, "inputType" | "cancelable" | "isComposing"> = {}) {
  const event = new dom.InputEvent("beforeinput", { inputType: "insertLineBreak", bubbles: true, cancelable: true, ...init });
  editor.dispatchEvent(event as unknown as Event);
  return event;
}

describe("native newline transaction", () => {
  it("inserts the prefix with the native command and emits the ordinary input", () => {
    const editor = field("1. first"), changed = vi.fn();
    editor.addEventListener("input", changed);
    command.mockImplementation((name, _ui, text) => {
      expect(name).toBe("insertText");
      const cursor = editor.selectionStart + text.length;
      editor.value = editor.value.slice(0, editor.selectionStart) + text + editor.value.slice(editor.selectionEnd);
      editor.setSelectionRange(cursor, cursor);
      editor.dispatchEvent(new dom.Event("input", { bubbles: true }) as unknown as Event);
      return true;
    });
    expect(input(editor).defaultPrevented).toBe(true);
    expect(editor.value).toBe("1. first\n2. "); expect(editor.selectionStart).toBe(editor.value.length);
    expect(changed).toHaveBeenCalledOnce();
    expect(input(editor).defaultPrevented).toBe(true);
    expect(editor.value).toBe("1. first\n");
    expect(command).toHaveBeenLastCalledWith("insertText", false, "\n");
  });

  it("preserves selection if the native command refuses", () => {
    const editor = field("1. first\n2. "); command.mockReturnValue(false);
    expect(input(editor).defaultPrevented).toBe(false);
    expect(editor.selectionStart).toBe(12); expect(editor.selectionEnd).toBe(12);
    expect(editor.value).toBe("1. first\n2. ");
  });

  it("leaves paste, history, IME, locked fields and non-cancelable input alone", () => {
    const editor = field("1. first");
    for (const inputType of ["insertFromPaste", "insertText", "historyUndo", "historyRedo"]) input(editor, { inputType });
    input(editor, { isComposing: true }); input(editor, { cancelable: false });
    editor.readOnly = true; input(editor); editor.readOnly = false;
    editor.disabled = true; input(editor);
    expect(command).not.toHaveBeenCalled();
  });
});
