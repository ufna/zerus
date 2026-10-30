import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { AgentBadge } from "./sessions/SessionCard";
import { Icon } from "./lib/icons";
import { UsageMeters, type UsageIndicator } from "./components/UsageMeter";
import { useNow } from "../store/latest";
import { followSessionUsage, refreshSessionUsage, sessionUsage, startSessionUsage, stopSessionUsage } from "../store/sessionUsage";
import { setAccountMachine, setRequestedProfile, loadAccounts } from "../store/accounts";
import { page, selected, setPage, subagentOpen } from "../store/ui";
import { computerOffline } from "../store/catalog";
import { text } from "../store/settings";
import { accountTooltip, summaryText, usageLoading, windowsAt } from "./lib/usage";
import { statusUsageProfile, statusUsageVisible } from "./status-usage-ui";

/** The selected session's account usage at the right of the window status bar. */
export function StatusUsage() {
  const now = useNow(30_000);
  const [ready, setReady] = createSignal(false);
  onMount(() => { startSessionUsage(); setReady(true); });
  const visible = () => ready() && statusUsageVisible(selected(), page(), !!subagentOpen());
  createEffect(() => {
    const s = selected();
    followSessionUsage(visible() && s ? { host: s.computer, name: s.name, run_id: s.runId ?? "" } : null, s?.cmd ?? "");
  });
  onCleanup(() => { followSessionUsage(null); stopSessionUsage(); });
  const u = sessionUsage;
  const openAccount = () => {
    const s = selected();
    if (!s) return;
    setAccountMachine(s.computer);
    setRequestedProfile(statusUsageProfile(s, u()));
    setPage("accounts");
    void loadAccounts();
  };
  return (
    <Show when={visible()}>
      <span class="status-account-usage">
        <button type="button" class="sb-usage account-usage" title={accountTooltip(u(), now())} aria-label={`Account usage\n${accountTooltip(u(), now())}`} onClick={openAccount}>
          <Show when={u().provider}><AgentBadge agent={selected()!.agent} /></Show>
          <UsageMeters windows={windowsAt(u().windows, now())} loading={usageLoading(u())} summary={summaryText(u())} indicator={text("workspace/usageIndicator") as UsageIndicator} />
        </button>
        <button type="button" class="icon-btn usage-refresh" disabled={computerOffline(selected()!.computer) || u().refreshing} aria-label="Refresh account usage"
          title={u().refreshing ? "Updating account usage…" : "Refresh account usage (automatically every 5 minutes)"} onClick={() => void refreshSessionUsage(true)}>
          <Show when={!u().refreshing} fallback={<span class="su-spin"><Icon name="refresh-cw" size={14} /></span>}><Icon name="refresh-cw" size={14} /></Show>
        </button>
      </span>
    </Show>
  );
}
