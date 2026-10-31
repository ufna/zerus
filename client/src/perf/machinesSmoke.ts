import { invoke } from "@tauri-apps/api/core";
import { setPage } from "../store/ui";
import { machinesPage } from "../store/machines";
import { machineEditor } from "../store/machineEditor";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
async function until(test: () => boolean, reason: string) {
  const deadline = performance.now() + 15_000;
  while (!test()) { if (performance.now() > deadline) throw Error(reason); await sleep(16); }
}
const buttons = () => [...document.querySelectorAll<HTMLButtonElement>(".mach-page button, dialog button")];
function button(text: string) { const b = buttons().find((b) => b.textContent?.trim() === text); if (!b) throw Error(`Missing button ${text}`); return b; }
function click(text: string) { const b = button(text); if (b.disabled) throw Error(`Disabled button ${text}`); b.click(); }
function field(id: string, value: string) {
  const input = document.getElementById(id) as HTMLInputElement;
  if (!input || input.disabled) throw Error(`Unavailable field ${id}`);
  input.value = value; input.dispatchEvent(new Event("input", { bubbles: true }));
}
function require(ok: boolean, reason: string) { if (!ok) throw Error(reason); }

/** Explicit fake CLI/replay only; drives the actual lazy screen and dialogs. */
export async function machinesSmoke(allowed: boolean) {
  if (!allowed) return;
  const log = (text: string) => invoke("perf_log", { line: `machines-smoke: ${text}` });
  const capture = async (stage: string) => { await log(stage); await sleep(700); };
  try {
    setPage("machines");
    await until(() => machinesPage().loaded && !machinesPage().busy && !!document.querySelector(".mach-editor"), "Machines did not load");
    const peer = buttons().find((b) => b.classList.contains("mach-card") && b.textContent?.includes("fixture-peer"));
    require(!!peer, "private peer missing");peer!.click();
    await until(() => (document.getElementById("mach-host") as HTMLInputElement)?.value === "peer.invalid", "saved fields missing");
    await capture("EXISTING");
    field("mach-host", "edited.invalid");
    require(button("Test connection").disabled, "dirty connection can be tested");
    click("Add machine");
    await until(() => !!document.querySelector("dialog[open]"), "unsaved dialog missing");
    await capture("UNSAVED");click("Cancel");
    require((document.getElementById("mach-host") as HTMLInputElement).value === "edited.invalid", "cancel lost draft");
    const enabled = document.querySelector<HTMLInputElement>(".mach-editor input[type=checkbox]")!;
    enabled.checked = false;enabled.dispatchEvent(new Event("change", { bubbles: true }));
    click("Save connection");
    await until(() => !machineEditor.busy() && machineEditor.notice()?.text === "Connection saved for fixture-peer.", "save did not settle");
    require(machinesPage().machines.find((m) => m.alias === "fixture-peer")?.enabled === false, "polling choice lost");
    click("Reset SSH overrides");
    await until(() => !machineEditor.busy() && machineEditor.notice()?.text.includes("were reset") === true, "reset did not settle");
    click("Add machine");
    await until(() => machineEditor.isNew(), "new draft missing");
    await capture("NEW");
    field("mach-alias", "fixture-peer");require(button("Save connection").disabled, "duplicate alias enabled");
    field("mach-alias", "new-peer");field("mach-host", "new.invalid");
    click("Save connection");
    await until(() => !machineEditor.busy() && machineEditor.selected() === "new-peer", "new connection did not settle");
    require(machinesPage().machines.some((m) => m.alias === "new-peer" && m.hostname === "new.invalid"), "new row missing");
    click("Remove from Zerus");
    await until(() => !!document.querySelector("dialog[open]"), "remove dialog missing");
    require(document.activeElement?.textContent === "Cancel", "destructive default was not Cancel");
    await capture("REMOVE");click("Cancel");
    require(machinesPage().machines.some((m) => m.alias === "new-peer"), "Cancel removed row");
    click("Remove from Zerus");
    await until(() => !!document.querySelector("dialog[open]"), "reopened remove dialog missing");
    click("Remove machine");
    await until(() => !machineEditor.busy() && !machinesPage().machines.some((m) => m.alias === "new-peer"), "remove did not settle");
    const calls = await invoke<{argv:string[]}[]>("smoke_log");
    const writes = calls.filter((c) => c.argv[0] === "machine" && ["set", "reset", "remove"].includes(c.argv[1]));
    require(writes.length === 4, `expected four explicit mutations, got ${writes.length}`);
    require(writes.every((c) => c.argv.includes("--revision")), "revision omitted");
    require(writes[2].argv.includes("--create"), "new alias was not create-only");
    require(writes[3].argv[2] === "new-peer", "removal changed target");
    await log("PASS 12 (saved fields, dirty guard, Cancel draft, polling choice, reset, new draft, duplicate guard, create-only, remove confirmation, default Cancel, exact target, four original revisions)");
    await invoke("exit_app", { code: 0 });
  } catch (error) { await log(`FAIL ${String(error)}`); await invoke("exit_app", { code: 1 }); }
}

/** Native picker and configured terminal, still fake/private only. */
export async function machinesToolsSmoke(allowed: boolean) {
  if (!allowed) return;
  const log = (text: string) => invoke("perf_log", { line: `machine-tools-smoke: ${text}` });
  let assertions = 0;
  const check = (ok: boolean, reason: string) => { require(ok, reason); assertions++; };
  const calls = () => invoke<{argv: string[]}[]>("smoke_log");
  try {
    setPage("machines");
    await until(() => machinesPage().loaded && !machineEditor.busy() && !!document.querySelector(".mach-editor"), "Machines did not load");
    const peer = buttons().find((b) => b.classList.contains("mach-card") && b.textContent?.includes("fixture-peer"))!;
    peer.click();
    await until(() => (document.getElementById("mach-host") as HTMLInputElement)?.value === "peer.invalid", "saved fields missing");
    check(machineEditor.draft().enabled === false, "disabled peer fixture missing");
    click("Open SSH terminal");
    await until(() => !machineEditor.busy(), "terminal request did not settle");
    let recorded = await calls();
    for (let i = 0; i < 40 && !recorded.some((c) => c.argv[0] === "private-terminal"); i++) { await sleep(25); recorded = await calls(); }
    check(recorded.filter((c) => c.argv[0] === "private-terminal").length === 1, "terminal did not open exactly once");
    check(recorded.some((c) => c.argv[0] === "private-terminal" && c.argv.at(-1)?.endsWith(" machine ssh fixture-peer") === true), "terminal command lost exact saved alias");
    check(!recorded.some((c) => c.argv[0] === "machine" && c.argv[1] === "check"), "SSH login repair ran a remote probe");
    check(document.body.textContent?.includes("Opened a terminal: ssh fixture-peer") === true, "mockup SSH notice missing");
    await log("SSH_OPENED");await sleep(700);
    const original = machineEditor.draft();
    click("Browse…");
    await until(() => machineEditor.busy(), "key chooser did not start");
    await log("KEY_CANCEL_WAIT");
    await until(() => !machineEditor.busy(), "key chooser Cancel did not finish");
    check(machineEditor.draft() === original && !machineEditor.draft().dirty, "Cancel changed the key draft");
    click("Browse…");
    await until(() => machineEditor.busy(), "second chooser did not start");
    await log("KEY_PICK_WAIT");
    await until(() => !machineEditor.busy(), "key chooser selection did not finish");
    check(machineEditor.draft().identity_file.endsWith("/id_fixture") && machineEditor.draft().dirty, "accepted local key path missing");
    recorded = await calls();
    check(recorded.filter((c) => c.argv[0] === "machine" && c.argv[1] === "resolve").length === 1, "effective-connection reply started a resolve loop");
    check(!recorded.some((c) => c.argv[0] === "machine" && ["set", "remove", "reset"].includes(c.argv[1])), "key selection wrote a connection without Save");
    await log(`PICKED ${JSON.stringify({ key: machineEditor.draft().identity_file })}`);await sleep(400);
    await log(`PASS ${assertions}`);await invoke("exit_app", { code: 0 });
  } catch (error) { await log(`FAIL ${String(error)}`);await invoke("exit_app", { code: 1 }); }
}
