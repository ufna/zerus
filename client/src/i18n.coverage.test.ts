import { describe, expect, it } from "vitest";
import catalog from "./i18n.ru.json";
import english from "./i18n.english.json";

// Every static tr()/trNoop() source is a reviewed Qt catalog entry or an
// explicit Next-only English string (i18n.english.json, read by this test only).
// tr() of a value stays in the files below: their values come from their own
// trNoop() tables or from literal component props, never from session text,
// agent output, identities or file names.
const DYNAMIC_FILES: Record<string, string> = {
  "./App.tsx": "connection state names from its STATE_TEXT table",
  "./ui/components/Rail.tsx": "its own screen table",
  "./ui/screens/Machines.tsx": "field labels passed as literal props",
  "./ui/screens/MachineConnectionDialog.tsx": "its own option tables",
  "./ui/screens/Settings.tsx": "the section table",
  "./ui/screens/settings/ui.tsx": "titles, labels and hints passed as literal props",
  "./ui/sessions/SessionList.tsx": "its filter and scope tables",
  "./ui/sessions/SessionCard.tsx": "the core's closed caption vocabulary and child states",
  "./ui/sessions/SavedDrafts.tsx": "the Saved drafts store's TEXTS, REASONS and SECTIONS tables and its own Show more table",
  "./ui/sessions/ArchiveBar.tsx": "the archive period table",
  "./ui/sessions/archive-dates.ts": "the archive period table",
};

const SOURCES = import.meta.glob(["./**/*.ts", "./**/*.tsx", "!./**/*.test.ts", "!./i18n.ts"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const STATIC = /\btr(?:Noop)?\(\s*("(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|`[^`$]*`)/g;
const DYNAMIC = /\btr\(\s*(?!["'])(?!`[^`$]*`)/;

function literal(token: string): string {
  if (token.startsWith("`")) return token.slice(1, -1);
  if (token.startsWith("'")) return JSON.parse(`"${token.slice(1, -1).replace(/\\'/g, "'").replace(/"/g, '\\"')}"`);
  return JSON.parse(token);
}

const sources = new Map<string, string[]>();
for (const [file, text] of Object.entries(SOURCES)) {
  for (const match of text.matchAll(STATIC)) {
    const source = literal(match[1]);
    sources.set(source, [...(sources.get(source) ?? []), file]);
  }
}
const translated = catalog as Record<string, unknown>;
const englishOnly = new Set(english as string[]);

describe("interface string coverage", () => {
  it("translates every wrapped literal or lists it as English-only", () => {
    const missing = [...sources].filter(([source]) => !(source in translated) && !englishOnly.has(source)).map(([source, files]) => `${JSON.stringify(source)} in ${files[0]}`);
    expect(missing).toEqual([]);
  });

  it("keeps the English-only list current", () => {
    expect([...englishOnly].filter((source) => source in translated)).toEqual([]);
    expect([...englishOnly].filter((source) => !sources.has(source))).toEqual([]);
    expect(english as string[]).toEqual([...english as string[]].sort());
  });

  it("translates computed values only in reviewed files", () => {
    const unreviewed = Object.entries(SOURCES).filter(([file, text]) => DYNAMIC.test(text) && !(file in DYNAMIC_FILES)).map(([file]) => file);
    expect(unreviewed).toEqual([]);
  });
});
