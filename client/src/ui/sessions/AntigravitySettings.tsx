import { Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import { tr, uiLanguage } from "../../i18n";
import type { Action, Outcome, Target } from "../../types";
import { Dropdown } from "../lib/Dropdown";
import { Icon } from "../lib/icons";
import { modelChoices, text } from "./session-model-settings";
import type { Session } from "./types";
import "../styles/newsession.css";

const record = (value: unknown): Record<string, unknown> => value && typeof value === "object" && !Array.isArray(value)
  ? value as Record<string, unknown> : {};
const revision = (value: unknown) => typeof value === "string"
  && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value) && /[1-9a-f]/.test(value) ? value : "";
const providerLabel = (value: unknown) => text(value).replace(/[\p{Cc}\p{Cf}\p{Zl}\p{Zp}]/gu, " ").trim().slice(0, 256);
const tokens = (value: unknown) => typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : undefined;

function observedSettings(row: Session, followed: Record<string, unknown> | undefined, online: boolean) {
  const live = row.state === "running" && !row.archiveId;
  const d = online && live && row.runId && row.conversationId && row.sessionId
    && followed?.name === row.name && followed.run_id === row.runId && followed.conversation_id === row.conversationId
    && followed.session_id === row.sessionId && followed.agent === "agy" && followed.process_state === "running"
    ? followed : undefined;
  const native = record(d?.agy_settings);
  const confirmed = native.status === "confirmed";
  const current = confirmed || native.status === "legacy";
  const model = current ? text(d?.settings_model) : "";
  const effort = confirmed ? text(d?.effort) : "";
  const options = current ? modelChoices(d?.model_options) : [];
  const modelOption = options.find((option) => option.id === model);
  const choice = Array.isArray(d?.model_options) ? d.model_options.slice(0, 256).find((entry) => record(entry).id === model) : undefined;
  const effortLabels = record(record(choice).effort_labels);
  const efforts = confirmed && modelOption ? modelOption.effort_options.map((value) => ({
    value, label: providerLabel(effortLabels[value]) || providerLabel(value),
  })) : [];
  const currentRevision = confirmed ? revision(native.revision) : "";
  const target: Target = { computer: row.computer, name: row.name, run_id: row.runId ?? "", conversation_id: row.conversationId ?? "" };
  const scope = JSON.stringify([target.computer, target.name, row.sessionId, target.run_id, target.conversation_id]);
  const canChange = !!currentRevision && !!model && d?.settings_apply_when === "now" && d?.settings_change_supported === true
    && d.activity === "idle" && d.phase === "idle" && d.auth_required !== true;
  const context = record(record(d?.session_usage).context);
  return { target, sessionId: row.sessionId ?? "", scope, live, d, native, model, effort, currentRevision,
    models: options.map(({ id, label }) => ({ value: id, label: providerLabel(label) || providerLabel(id) })), efforts,
    modelLabel: providerLabel(modelOption?.label) || providerLabel(model),
    effortLabel: efforts.find(({ value }) => value === effort)?.label || providerLabel(effort),
    mode: confirmed ? providerLabel(record(native.mode).label) || providerLabel(record(native.mode).id) : "",
    modelChange: canChange && d?.model_change_supported === true,
    effortChange: canChange && d?.effort_change_supported === true,
    used: context.used_source === "agy_acp" ? tokens(context.used) : undefined,
    limit: context.limit_source === "agy_acp" ? tokens(context.limit) : undefined };
}

type AntigravityAction = Extract<Action, { kind: "antigravity_settings" }>;
type Choice = { scope: string; revision: string; kind: "model" | "effort"; value: string };
type Attempt = { scope: string; revision: string; state: "sending" | "applied" | "uncertain" | "refused" };

export function AntigravitySettings(props: {
  row: Session;
  followed?: Record<string, unknown>;
  online: boolean;
  busy: boolean;
  apply: (target: Target, action: AntigravityAction) => Promise<Outcome>;
  refresh: (target: Target) => boolean;
}) {
  const state = createMemo(() => observedSettings(props.row, props.followed, props.online));
  const scope = createMemo(() => state().scope);
  const [opened, setOpened] = createSignal(false);
  const [selection, setSelection] = createSignal<Choice>();
  const [attempt, setAttempt] = createSignal<Attempt>();
  let anchor!: HTMLButtonElement;
  createEffect(on(scope, () => { setOpened(false); setSelection(undefined); }, { defer: true }));

  const choice = () => {
    const value = selection(), s = state();
    return value?.scope === s.scope && value.revision === s.currentRevision ? value : undefined;
  };
  const activeAttempt = () => {
    const value = attempt(), s = state();
    return value?.scope === s.scope ? value : undefined;
  };
  const pending = () => {
    const value = activeAttempt(), current = state().currentRevision;
    return !!value && (value.state === "sending" || value.state !== "refused" && (!current || current === value.revision));
  };
  const canApply = () => {
    const s = state(), c = choice();
    if (!opened() || !c || props.busy || pending()) return false;
    return c.kind === "model" ? s.modelChange && c.value !== s.model && s.models.some(({ value }) => value === c.value)
      : s.effortChange && c.value !== s.effort && s.efforts.some(({ value }) => value === c.value);
  };
  const choose = (kind: Choice["kind"], value: string) => {
    const s = state();
    setSelection(value === s[kind] ? undefined : { scope: s.scope, revision: s.currentRevision, kind, value });
  };
  async function apply() {
    if (!canApply()) return;
    const s = state(), c = choice()!;
    const action: AntigravityAction = { kind: "antigravity_settings", expected_session_id: s.sessionId, expected_config_revision: c.revision,
      model: c.kind === "model" ? c.value : s.model, ...(c.kind === "effort" ? { effort: c.value } : {}) };
    const original: Attempt = { scope: s.scope, revision: c.revision, state: "sending" };
    setAttempt(original);
    setSelection(undefined);
    let outcome: Outcome | undefined;
    try { outcome = await props.apply(s.target, action); } catch { /* An unconfirmed call cannot authorize another write. */ }
    if (attempt() !== original) return;
    if (outcome?.outcome === "done" && record(outcome.receipt).changed === false) {
      setAttempt(undefined);
      return;
    }
    setAttempt({ ...original, state: outcome?.outcome === "done" ? "applied" : outcome?.outcome === "failed" ? "refused" : "uncertain" });
  }

  const hint = () => {
    const s = state(), value = activeAttempt();
    if (value?.state === "sending") return tr("Updating model settings…");
    if (pending()) return value?.state === "uncertain"
      ? tr("Antigravity settings change is unconfirmed. Check Activity before trying again.")
      : tr("Waiting for Antigravity to report the updated settings.");
    if (value?.state === "refused" && s.currentRevision === value.revision) return tr("Antigravity refused the change. Refresh Activity.");
    if (!props.online) return tr("This computer is offline. Current Antigravity settings are unavailable.");
    if (!s.live) return tr("Current Antigravity settings are available only while the session is running.");
    if (!s.d) return tr("Waiting for current Antigravity settings.");
    if (s.d.auth_required === true) return tr("Sign in to Antigravity to see the session settings.");
    if (s.native.reason === "awaiting_native_result") return tr("Waiting for Antigravity to report the updated settings.");
    if (!s.model) return tr("This Antigravity worker does not report its current model settings.");
    if (s.native.status === "legacy") return tr("This Antigravity worker reports its model. Changes are unavailable in Activity.");
    if (props.busy || s.d.activity === "busy") return tr("Wait for the Antigravity turn to finish before changing settings.");
    if (!s.modelChange && !s.effortChange) return tr("Antigravity does not offer settings changes for this session.");
    if (choice()?.kind === "model") return tr("Apply the model first to see its available thinking levels.");
    return tr("Applies to this session only.");
  };
  const usage = () => {
    const { used, limit } = state();
    const format = (value: number) => value.toLocaleString(uiLanguage());
    if (used !== undefined && limit !== undefined && limit > 0) return tr("%1 of %2 tokens", format(used), format(limit));
    if (used !== undefined) return tr("%1 tokens used", format(used));
    if (limit !== undefined && limit > 0) return tr("%1-token limit", format(limit));
    return tr("Not reported");
  };
  const close = () => setOpened(false);
  function Dialog() {
    let dialog!: HTMLDialogElement;
    onMount(() => {
      dialog.showModal();
      dialog.querySelector<HTMLElement>('[role="combobox"]:not(:disabled)')?.focus();
    });
    onCleanup(() => { if (dialog.open) dialog.close(); queueMicrotask(() => anchor?.focus()); });
    const displayed = (kind: Choice["kind"]) => choice()?.kind === kind ? choice()!.value : state()[kind];
    const choices = (kind: Choice["kind"]) => {
      const s = state(), all = kind === "model" ? s.models : s.efforts;
      return all.some(({ value }) => value === s[kind]) ? all : [{ value: s[kind],
        label: (kind === "model" ? s.modelLabel : s.effortLabel) || tr("Not reported"), disabled: true }, ...all];
    };
    return <dialog ref={dialog} class="dlg cmp-settings-dialog" aria-labelledby="agy-settings-title"
      onCancel={(event) => { event.preventDefault(); close(); }}>
      <div class="dlg-head"><Icon name="sparkles" size={18} class="accent" /><h2 id="agy-settings-title">{tr("Antigravity model settings")}</h2>
        <span class="grow" /><button type="button" class="icon-btn" aria-label={tr("Close")} onClick={close}><Icon name="x" size={16} /></button></div>
      <div class="dlg-body">
        <div class="form-row"><label for="agy-model">{tr("Model")}</label>
          <Dropdown id="agy-model" class="field" aria-label={tr("Model")} value={displayed("model")} options={choices("model")}
            disabled={!state().modelChange || !state().models.length || props.busy || pending()} onChange={(value) => choose("model", value)} /></div>
        <div class="form-row"><label for="agy-effort">{tr("Thinking level")}</label>
          <Dropdown id="agy-effort" class="field" aria-label={tr("Thinking level")} value={displayed("effort")} options={choices("effort")}
            disabled={!state().effortChange || !state().efforts.length || props.busy || pending() || choice()?.kind === "model"}
            onChange={(value) => choose("effort", value)} /></div>
        <Show when={state().mode}>{(mode) => <div class="form-row" role="group" aria-label={tr("%1: %2", tr("Execution mode"), mode())}>
          <span aria-hidden="true">{tr("Execution mode")}</span><span aria-hidden="true">{mode()}</span>
        </div>}</Show>
        <p class="cmp-settings-hint hint" role="status">{hint()}</p>
        <Show when={state().native.reason === "legacy_worker" && state().d?.resumable === true}>
          <p class="hint">{tr("To load settings for this conversation, wait until it is idle. In Session actions, choose Stop session, then Resume session.")}</p>
        </Show>
        <div class="form-row" role="group" aria-label={tr("%1: %2", tr("Context usage"), usage())}>
          <span aria-hidden="true">{tr("Context usage")}</span><span aria-hidden="true">{usage()}</span>
        </div>
        <div class="form-row" role="group" aria-label={tr("%1: %2", tr("Account quota"), tr("Unavailable in Activity"))}>
          <span aria-hidden="true">{tr("Account quota")}</span><span aria-hidden="true">{tr("Unavailable in Activity")}</span>
        </div>
        <p class="hint">{tr("For Google account quota, enter /usage in the interactive Antigravity CLI on this computer.")}</p>
      </div>
      <div class="dlg-foot">
        <Show when={state().d && (activeAttempt() || state().native.reason === "legacy_worker")}><button type="button" class="btn ghost" disabled={activeAttempt()?.state === "sending"}
          onClick={() => { setSelection(undefined); props.refresh(state().target); }}>{tr("Check Activity")}</button></Show>
        <span class="spacer" /><button type="button" class="btn" onClick={close}>{tr("Close")}</button>
        <button type="button" class="btn primary" disabled={!canApply()} onClick={() => void apply()}>{tr("Apply")}</button>
      </div>
    </dialog>;
  }
  return <>
    <button ref={anchor} type="button" class="btn ghost small cmp-model" aria-label={tr("Antigravity model settings")}
      aria-haspopup="dialog" aria-expanded={opened()} title={tr("Antigravity model settings")}
      onClick={() => { setSelection(undefined); setOpened(true); }}>
      <span class="ell cmp-model-name">{state().modelLabel || tr("Model unavailable")}</span>
      <Show when={state().effortLabel}>{(effort) => <span class="dim cmp-model-suffix">{effort()}</span>}</Show>
      <Icon name="chevron-down" size={12} />
    </button>
    <Show when={opened()}><Dialog /></Show>
  </>;
}
