// @vitest-environment happy-dom
// zerus-uiy9p G11: checking the same folder again shows the newest answer.
// The audit saw "Not a Git folder:" with an empty reason beside an enabled
// Create team after a failed check was followed by a passing one.
import { afterEach, expect, it, vi } from "vitest";
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { NewTeam } from "./NewTeam";
import { catalog } from "./fixtures";

let dispose: (() => void) | undefined;
afterEach(() => { dispose?.(); dispose = undefined; document.body.replaceChildren(); });

const HEAD = "0123456789abcdef0123456789abcdef01234567";
const find = (host: HTMLElement, label: string) =>
  [...host.querySelectorAll("button")].find((button) => button.textContent === label) as HTMLButtonElement;

function setup(answers: { git: boolean; head: string; branch: string; reason: string }[]) {
  const host = document.createElement("div");
  document.body.append(host);
  const asked: string[] = [];
  dispose = render(() => createComponent(NewTeam, {
    busy: false, machineId: "00000000-0000-4000-8000-0000000000aa", catalog,
    defaultProject: "example-project", defaultFolder: "/workspace/example",
    onCheckFolder: async (path: string) => { asked.push(path); return answers[Math.min(asked.length - 1, answers.length - 1)]; },
    onCreate: async (input) => ({ outcome: "done", receipt: {}, request_id: input.request_id }), onSaveTemplate: async () => {}, onClose: () => {},
  }), host);
  const name = host.querySelector<HTMLInputElement>('input[placeholder="Fixes"]')!;
  name.value = "Fixes"; name.dispatchEvent(new Event("input", { bubbles: true }));
  return { host, asked };
}

it("a passing check after a failed one of the same folder shows the Git folder", async () => {
  const { host, asked } = setup([
    { git: false, head: "", branch: "", reason: "not a Git repository" },
    { git: true, head: HEAD, branch: "main", reason: "" },
  ]);
  find(host, "Check folder").click();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-bad]")?.textContent).toBe("Not a Git folder: not a Git repository"));
  find(host, "Check folder").click();
  await vi.waitFor(() => expect(asked).toHaveLength(2));
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")?.textContent).toBe("A Git folder at main, base commit 0123456789ab."));
  expect(host.querySelector("[data-folder-bad]")).toBeNull();
  expect(find(host, "Create team").disabled).toBe(false);
});

it("a failed check after a passing one of the same folder says why and keeps Create closed", async () => {
  const { host, asked } = setup([
    { git: true, head: HEAD, branch: "main", reason: "" },
    { git: false, head: "", branch: "", reason: "the folder was removed" },
  ]);
  find(host, "Check folder").click();
  await vi.waitFor(() => expect(host.querySelector("[data-folder-ok]")).not.toBeNull());
  find(host, "Check folder").click();
  await vi.waitFor(() => expect(asked).toHaveLength(2));
  await vi.waitFor(() => expect(host.querySelector("[data-folder-bad]")?.textContent).toBe("Not a Git folder: the folder was removed"));
  expect(host.querySelector("[data-folder-ok]")).toBeNull();
  expect(find(host, "Create team").disabled).toBe(true);
});
