// Every operation the team store journals reads as a sentence in the Feed
// (zerus-rzvct G6). The kinds come from the store's own Operation enum, whose
// serde tag (snake_case) is the journal row's kind.
import { expect, it } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { journalSentence } from "./feed";

it("has a sentence for every operation the team store journals", () => {
  const source = readFileSync(fileURLToPath(new URL("../../../../src/teams/ops/types.rs", import.meta.url)), "utf8");
  const body = source.slice(source.indexOf("pub enum Operation {"));
  const ops = [...body.slice(0, body.indexOf("\n}\n")).matchAll(/^ {4}([A-Z][A-Za-z]+)\b/gm)]
    .map((match) => match[1].replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase());
  expect(ops.length).toBeGreaterThan(70);
  expect(ops).toContain("set_member_session");
  for (const op of ops) expect(journalSentence(op), op).not.toBe(op.replaceAll("_", " "));
  expect(new Set(ops.map(journalSentence)).size, "each operation reads as its own sentence").toBe(ops.length);
});

it("has a sentence for every kind a writer sets in place of its operation", () => {
  // `ctx.event = Some("...")` overrides the journal row's kind (ops/journal.rs).
  const root = fileURLToPath(new URL("../../../../src/teams/ops", import.meta.url));
  const files = readdirSync(root, { recursive: true, encoding: "utf8" }).filter((file) => file.endsWith(".rs") && !file.includes("test"));
  const kinds = new Set(files.flatMap((file) => [...readFileSync(join(root, file), "utf8").matchAll(/ctx\.event = Some\("([a-z_]+)"\)/g)].map((match) => match[1])));
  expect([...kinds].sort()).toEqual(expect.arrayContaining(["accepted_superseded", "person_override", "stale_card_decision"]));
  for (const kind of kinds) expect(journalSentence(kind), kind).not.toBe(kind.replaceAll("_", " "));
});
