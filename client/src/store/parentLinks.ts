import { createSignal } from "solid-js";
import { setParent, type ParentChange, type ParentProjectConsent } from "../bridge";
import { catalog, computerFresh, orderedRows } from "./catalog";
import { machineCards } from "./machines";
import { grouping, groups, headerOf } from "./organization";
import { toast } from "./notices";
import { parentIndex } from "../ui/sessions/model";
import type { Session } from "../ui/sessions/types";
import { tr } from "../i18n";

// Desktop changes of parent links (zerus-sa9qn.9): Detach, Set parent… and
// drag to nest. Every change is confirmed in a dialog first; the session's
// computer checks the exact session, the chosen parent and cycles.

export type ParentDialog =
  | { kind: "detach"; child: Session }
  | { kind: "set"; child: Session; parent?: Session };
export const [parentDialog, setParentDialog] = createSignal<ParentDialog>();
const [busy, setBusy] = createSignal(false);
export const parentChangeBusy = busy;

/** Agents whose sessions carry parent links (the host refuses others). */
const LINKED = new Set(["claude", "codex", "kimi", "dsh", "opencode"]);
export const parentLinkEligible = (s: Session) => !!s.sessionId && !!s.runId && !!s.conversationId && s.state !== "archived" && LINKED.has(s.cmd);

/** Whether `child` may be made a child of `parent`: both identified, on one computer, not the same session. */
export const nestable = (child: Session, parent: Session) =>
  child.key !== parent.key && child.computer === parent.computer && parentLinkEligible(child) && parentLinkEligible(parent);

/**
 * The sessions Set parent… offers for `child` among `rows`: same computer,
 * identified, never the child's own descendants (a bounded walk up each
 * row's parents; the host refuses any cycle it still finds).
 */
export function candidatesAmong(child: Session, rows: readonly Session[], machineOf: (computer: string) => string | undefined): Session[] {
  const keyOf = parentIndex(rows, machineOf);
  const byKey = new Map(rows.map((row) => [row.key, row] as const));
  const below = (row: Session) => {
    let at: Session | undefined = row;
    for (let depth = 0; at && depth < 8; depth++) {
      const up = keyOf(at);
      if (!up) return false;
      if (up === child.key) return true;
      at = byKey.get(up);
    }
    return false;
  };
  return rows.filter((s) => nestable(child, s) && !below(s));
}

export const parentCandidates = (child: Session) => candidatesAmong(child, orderedRows(), (computer) => machineCards[computer]?.machine_id);

const target = (s: Session) => ({ computer: s.computer, name: s.name, run_id: s.runId ?? "", conversation_id: s.conversationId ?? "" });
const short = (name: string) => name.split("/").pop() || name;

export function parentProjects(child: Session, parent: Session) {
  const machine = machineCards[child.computer]?.machine_id;
  const from = headerOf(groups[child.key]?.group ?? ""), to = headerOf(groups[parent.key]?.group ?? "");
  if (!machine || !grouping().loaded || !from || !to) return undefined;
  return { machine, from, to, fallback: grouping().default_project };
}

function currentInstance(s: Session) {
  const latest = catalog.rows[s.key];
  return latest && latest.name === s.name && latest.sessionId === s.sessionId
    && latest.runId === s.runId && latest.conversationId === s.conversationId && latest.state !== "archived";
}

async function change(child: Session, request: ParentChange, done: string) {
  if (busy()) return false;
  setBusy(true);
  try {
    const reply = await setParent(target(child), request);
    if (reply.project_status === "uncertain") {
      toast(tr("Parent session"), tr("The parent was set. Check the session’s project because the move could not be confirmed."), "error", true);
    } else {
      toast(tr("Parent session"), done);
    }
    return true;
  } catch (error) {
    const detail = String(error instanceof Error ? error.message : error);
    toast(tr("Parent session"), detail, "error", true);
    // The answer may have been lost after the parent or project write.
    // Require a new dialog and fresh facts before offering another choice.
    return request.kind === "set" && !!request.project;
  } finally {
    setBusy(false);
  }
}

/** Removes the child's link after its Detach confirmation. */
export const detachParent = (child: Session) =>
  change(child, { kind: "clear" }, tr("%1 is no longer a child session.", short(child.title)));

/** Makes `parent` the child's parent after its confirmation; replacing another parent was part of that confirmation. */
export type ParentProjects = NonNullable<ReturnType<typeof parentProjects>>;
export type ParentProjectChoice = { projects: ParentProjects; choice: ParentProjectConsent["choice"] };

export function chooseParent(child: Session, parent: Session, placement: ParentProjectChoice) {
  if (busy()) return false;
  const now = parentProjects(child, parent);
  const before = placement.projects;
  const moved = now?.from.id !== before.from.id || now.to.id !== before.to.id
    || now.fallback !== before.fallback || now.machine !== before.machine;
  if (!nestable(child, parent) || !currentInstance(child) || !currentInstance(parent) || moved) {
    toast(tr("Parent session"), tr("The sessions or projects changed. Open Set parent again."), "error", true);
    return true;
  }
  if (!grouping().ready || grouping().busy || !computerFresh(child.computer)) return false;

  const previous = child.parent;
  const project: ParentProjectConsent = {
    choice: placement.choice,
    machine_id: placement.projects.machine, child_id: child.sessionId ?? "",
    child_project: placement.projects.from.id, parent_project: placement.projects.to.id, default_project: placement.projects.fallback,
    previous: previous ? { machine: previous.machine, session_id: previous.session_id ?? "",
      run_id: previous.run_id ?? "", conversation_id: previous.conversation_id ?? "" } : null,
  };
  return change(child, {
    kind: "set", session_id: parent.sessionId ?? "", name: parent.name, conversation_id: parent.conversationId ?? "",
    run_id: parent.runId ?? "", replace: !!previous, project,
  }, placement.choice === "move"
    ? tr("%1 is now a child of %2 and belongs to %3.", short(child.title), short(parent.title), placement.projects.to.name)
    : tr("%1 is now a child of %2.", short(child.title), short(parent.title)));
}
