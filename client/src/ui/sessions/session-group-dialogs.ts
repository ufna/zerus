import { createSignal } from "solid-js";
import type { Session } from "./types";

export interface SessionGroupPrompt { project: string; rows: Session[]; origin: HTMLElement | null }
export const [sessionGroupPrompt, setSessionGroupPrompt] = createSignal<SessionGroupPrompt>();
export const openSessionGroups = (project: string, rows: readonly Session[] = []) =>
  setSessionGroupPrompt({ project, rows: rows.map((s) => ({ ...s })), origin: document.activeElement instanceof HTMLElement ? document.activeElement : null });
