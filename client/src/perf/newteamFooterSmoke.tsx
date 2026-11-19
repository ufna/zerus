// zerus-26ltc: the production New team form (NewTeamPage, NewTeam and its
// NewTeamFooter) in the real dialog classes, with a refusal passed the way
// NewTeamDialog passes the store's failure. Native calls are unavailable;
// callbacks only record. The WebKitGTK driver beside the HTML entry sets the
// window size and text zoom and collects one result per load.
import "../ui/styles/base.css";
import "../ui/styles/app.css";
import "../ui/styles/sessions.css";
import "../ui/styles/detail.css";
import "../ui/styles/client.css";
import "../ui/styles/goto.css";
import "../ui/styles/screens.css";
import "../ui/styles/machines.css";
import "../ui/styles/motion.css";
import "../ui/styles/settings.css";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { catalog, projectsView } from "../ui/teams/fixtures";

const params = new URLSearchParams(location.search);
const HERE = "00000000-0000-4000-8000-0000000000aa";
const HEAD = "0123456789abcdef0123456789abcdef01234567";
// The team store's own refusal (src/teams/store/mod.rs); the long one is that
// sentence repeated with a long unbroken path, about 2,200 characters, as in
// the bead's reproduction. Synthetic text, not an observed backend reply.
const SHORT = "The team store is busy. Nothing was written; try again.";
// Underscores give the path no break opportunity; only overflow-wrap can wrap it.
const LONG = `${Array.from({ length: 36 }, () => SHORT).join(" ")} /workspace/${"very_long_unbroken_folder_name_".repeat(6)}end`;
const refusal = params.get("refusal") === "long" ? LONG : params.get("refusal") === "short" ? SHORT : "";

const checks: { name: string; ok: boolean }[] = [], failures: string[] = [], closes: number[] = [];
const check = (name: string, ok: unknown) => { checks.push({ name, ok: !!ok }); if (!ok) failures.push(name); };
const frame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
const settle = async () => { for (let i = 0; i < 6; i++) await frame(); };
const box = (element: Element) => { const r = element.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height, right: r.right, bottom: r.bottom }; };
const emit = (value: unknown) => (window as unknown as { webkit: { messageHandlers: { proof: { postMessage: (message: string) => void } } } }).webkit.messageHandlers.proof.postMessage(JSON.stringify(value));

async function run() {
  const { settingsReady, setSetting } = await import("../store/settings");
  await settingsReady;
  if (params.get("lang") === "ru") {
    await setSetting("workspace/language", "ru", { persist: false });
    const { installUiLanguage, uiLanguage } = await import("../i18n");
    installUiLanguage();
    for (let i = 0; i < 100 && uiLanguage() !== "ru"; i++) await frame();
    check("Russian catalog loaded", uiLanguage() === "ru");
    document.documentElement.lang = "ru";
  }
  const { applySkin, skins } = await import("../skin/runtime");
  applySkin(skins[0]);
  document.documentElement.dataset.motion = "off";
  const { NewTeam } = await import("../ui/teams/NewTeam");
  const { NewTeamPage } = await import("../ui/teams/NewTeamPage");
  const host = document.getElementById("root")!;
  let shown!: HTMLDialogElement;
  render(() => {
    const element = <dialog ref={shown} class="dlg newteam-page-dialog" aria-labelledby="new-team-title">
      <NewTeamPage onClose={() => closes.push(Date.now())}>{createComponent(NewTeam, {
        catalog, busy: false, machineId: HERE, defaultProject: "", defaultFolder: "/workspace/example", projects: projectsView.projects,
        onCheckFolder: async () => ({ git: true, head: HEAD, branch: "main", reason: "" }),
        onCreate: async (input) => ({ outcome: "done", receipt: {}, request_id: input.request_id }),
        onSaveTemplate: async () => {}, onClose: () => closes.push(Date.now()), failure: refusal || undefined,
      })}</NewTeamPage>
    </dialog>;
    return element;
  }, host);
  shown.showModal();
  await document.fonts.ready;
  await settle();
  measure();
}

/** The person can see and press the element: inside the visible dialog and
 * the window, and the topmost element at its centre. */
function reachable(element: Element) {
  const r = element.getBoundingClientRect(), d = dialog().getBoundingClientRect();
  const inside = r.top >= Math.max(0, d.top) - 1 && r.bottom <= Math.min(innerHeight, d.bottom) + 1
    && r.left >= Math.max(0, d.left) - 1 && r.right <= Math.min(innerWidth, d.right) + 1;
  const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
  return { inside, hit: !!hit && (hit === element || element.contains(hit)) };
}
const dialog = () => document.querySelector<HTMLDialogElement>("dialog.newteam-page-dialog")!;

function measure() {
  const name = params.get("case") ?? "case";
  const d = dialog(), page = d.querySelector<HTMLElement>(".newteam-page")!;
  const body = d.querySelector<HTMLElement>("[data-new-team-form]")!, foot = d.querySelector<HTMLElement>("[data-new-team-footer]")!;
  const buttons = [...foot.querySelectorAll<HTMLButtonElement>(".nt-foot-row > button")];
  const notice = foot.querySelector<HTMLElement>("[data-create-error]");
  const record: Record<string, unknown> = {
    name, viewport: [innerWidth, innerHeight], dialog: box(d), body: box(body), footer: box(foot),
    bodyClient: [body.clientHeight, body.scrollHeight], page: [page.clientHeight, page.scrollHeight],
    buttons: buttons.map((b) => ({ text: b.textContent, box: box(b), ...reachable(b) })),
  };
  check(`${name}: three footer buttons`, buttons.length === 3);
  // A window below the app's smallest (900 x 560) may scroll the whole form
  // as the last resort; any window the app can have keeps the actions in view.
  const scrolled = page.scrollHeight > page.clientHeight + 1;
  record.pageScrolls = scrolled;
  check(`${name}: the page needs no scrolling of its own`, !scrolled || params.get("below") === "1");
  if (scrolled) page.scrollTop = page.scrollHeight;
  check(`${name}: the dialog fits the window`, d.getBoundingClientRect().top >= -1 && d.getBoundingClientRect().bottom <= innerHeight + 1);
  for (const b of buttons) {
    const where = reachable(b);
    check(`${name}: ${b.textContent} is inside the dialog`, where.inside);
    check(`${name}: ${b.textContent} is not covered`, where.hit);
  }
  // A window the app can have keeps a usable form (a field and its label,
  // about four lines) beside any refusal; a smaller one keeps at least a line.
  check(`${name}: the form body keeps room`, body.clientHeight >= (params.get("below") === "1" ? 47 : 96));
  check(`${name}: the body ends where the footer starts`, body.getBoundingClientRect().bottom <= foot.getBoundingClientRect().top + 1);
  if (refusal) {
    check(`${name}: the refusal is shown`, !!notice);
    if (notice) {
      const text = (notice.textContent ?? "").replace(/\s+/g, " ");
      check(`${name}: the whole refusal is in the footer`, text.includes(refusal.replace(/\s+/g, " ")));
      const n = notice.getBoundingClientRect(), f = foot.getBoundingClientRect();
      check(`${name}: the refusal box is inside the footer`, n.top >= f.top - 1 && n.bottom <= f.bottom + 1);
      check(`${name}: the refusal does not overflow sideways`, notice.scrollWidth <= notice.clientWidth + 1);
      const scrolls = notice.scrollHeight > notice.clientHeight + 1;
      record.refusal = { box: box(notice), client: notice.clientHeight, scroll: notice.scrollHeight, scrolls, overflowY: getComputedStyle(notice).overflowY, tabIndex: notice.tabIndex };
      if (scrolls) {
        check(`${name}: a long refusal scrolls in its box`, ["auto", "scroll"].includes(getComputedStyle(notice).overflowY));
        check(`${name}: a long refusal can be scrolled from the keyboard`, notice.tabIndex >= 0);
        // Its start: a centred box would overflow upwards, out of scroll reach.
        notice.scrollTop = 0;
        const label = notice.querySelector("b")!.getBoundingClientRect();
        check(`${name}: the refusal's start is in view at the top`, label.top >= notice.getBoundingClientRect().top - 1);
        notice.scrollTop = notice.scrollHeight;
        check(`${name}: the refusal's end can be reached`, notice.scrollTop + notice.clientHeight >= notice.scrollHeight - 1);
      }
      // Below the app's smallest window even a short refusal may scroll in its box.
      if (params.get("refusal") === "short" && params.get("below") !== "1") check(`${name}: a short refusal shows whole without scrolling`, !scrolls);
      if (params.get("refusal") === "short") record.shortScrolls = scrolls;
    }
  }
  // Cancel works where it is: a click at its centre reaches it.
  const cancel = buttons[buttons.length - 1];
  if (cancel) {
    const r = cancel.getBoundingClientRect();
    (document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2) as HTMLElement | null)?.click();
    check(`${name}: Cancel closes`, closes.length === 1);
  }
  emit({ type: "result", ok: failures.length === 0, name, checks, failures, record });
}

void run().catch((error) => emit({ type: "result", ok: false, name: params.get("case"), checks, failures: [...failures, `fixture error: ${String(error)}`], record: {} }));
