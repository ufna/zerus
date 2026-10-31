import { createSignal } from "solid-js";
import { accountEditOpen, accountEditApply, accountEditClose, type AccountEditKind, type AccountEditView, type AccountEditResult } from "../bridge";
import { notify } from "./actions";

export interface AccountPrompt extends AccountEditView { origin: HTMLElement }
interface Port {
  open(account: string, profile: string, kind: AccountEditKind): Promise<AccountEditView>;
  apply(handle: string, label: string): Promise<AccountEditResult>;
  close(handle: string): Promise<void>;
  notice(error: boolean, text: string): void;
}
export function createAccountManagement(port: Port) {
  const [prompt, setPrompt] = createSignal<AccountPrompt>();
  const [busy, setBusy] = createSignal(false);
  const [notice, setNotice] = createSignal("");
  let generation = 0;
  const close = () => {
    generation++;
    const current = prompt(); setPrompt(undefined);
    if (current) void port.close(current.handle).catch(() => {});
  };
  const apply = async (view: AccountEditView, label = "") => {
    setPrompt(undefined); setBusy(true);
    try {
      const answer = await port.apply(view.handle, label);
      answer.notices.forEach(text => { setNotice(text); port.notice(false, text); });
      answer.errors.forEach(text => port.notice(true, text));
    } catch (error) { port.notice(true, String(error)); }
    finally { setBusy(false); }
  };
  return {
    prompt, busy, close, notice, clearNotice: () => setNotice(""),
    async open(kind: AccountEditKind, account: string, profile: string, origin: HTMLElement) {
      if (busy()) return;
      setNotice("");
      close(); const current = generation; setBusy(true);
      try {
        const view = await port.open(account, profile, kind);
        if (current !== generation) { await port.close(view.handle); return; }
        if (kind === "default") await apply(view);
        else setPrompt({ ...view, origin });
      } catch (error) { if (current === generation) port.notice(true, String(error)); }
      finally { setBusy(false); }
    },
    async confirm(label = "") {
      const view = prompt();
      if (!view || busy()) return;
      if (view.kind === "rename" && label.trim() === view.label) { close(); return; }
      await apply(view, label);
    },
  };
}
export const accountManagement = createAccountManagement({
  open: accountEditOpen, apply: accountEditApply, close: accountEditClose,
  // The mockup keeps successful account edits inside the selected card.
  // Qt's failures go through the existing sticky notice bus.
  notice: (error, text) => { if (error) notify("error", text, true); },
});
