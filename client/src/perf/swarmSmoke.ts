/** Private replay/fake-CLI native adapter walk. No production activation. */
import { invoke } from "@tauri-apps/api/core";
import { projectsPage } from "../store/projects";
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
interface View { id: string; busy: boolean; error: boolean; status: string; aliases: string[]; can_join: boolean; can_sync: boolean; preview: { same_swarm: boolean } | null }
export async function swarmSmoke(allowed: boolean) {
  if (!allowed) return;
  const log = (line: string) => invoke("perf_log", { line: "swarm-smoke: " + line });
  const require = (value: unknown, reason: string) => { if (!value) throw Error(reason); };
  async function until(test: () => boolean | Promise<boolean>, reason: string) { const deadline = performance.now() + 20_000; while (!await test()) { if (performance.now() > deadline) throw Error(reason); await sleep(30); } }
  try {
    await until(() => projectsPage().loaded && !!document.querySelector(".projects .su-head"), "Projects did not render");
    await log("PROJECTS"); await sleep(500);
    let view = await invoke<View>("project_swarm_open"); const id = view.id;
    const read = async () => { view = await invoke<View>("project_swarm_view", { id }); return view; };
    await until(async () => { await read(); return !view.busy && view.aliases.includes("fixture-peer"); }, "saved alias read did not settle");
    require(!view.preview, "opening offered a Join without Preview"); await log("OPEN");
    view = await invoke<View>("project_swarm_preview", { id, alias: "fixture-peer" });
    await until(async () => { await read(); return !view.busy && !!view.preview; }, "preview did not settle");
    require(view.preview?.same_swarm && view.can_join, "same-catalog preview was not pinned"); await log("PREVIEW");
    view = await invoke<View>("project_swarm_join", { id, mappings: [], keep: false });
    await until(async () => { await read(); return !view.busy && view.status === "Connected to fixture-peer."; }, "Join/source barrier did not settle");
    require(!view.preview && !view.error, "Join retained replay input or showed failure"); await log("JOIN");
    let refused = false; try { await invoke("project_swarm_join", { id, mappings: [], keep: false }); } catch { refused = true; }
    require(refused, "old preview could be reused");
    await invoke("project_swarm_sync", { id });
    await until(async () => { await read(); return !view.busy && view.status.startsWith("Synchronization finished."); }, "Sync/source barrier did not settle");
    await invoke("project_swarm_close", { id });
    await log("PASS main-window native API, saved alias, exact preview, one Join, source settlement, no replay and one Sync");
    await invoke("exit_app", { code: 0 });
  } catch (error) { await log("FAIL " + String(error) + (error instanceof Error ? "\n" + error.stack : "")); await invoke("exit_app", { code: 1 }); }
}
