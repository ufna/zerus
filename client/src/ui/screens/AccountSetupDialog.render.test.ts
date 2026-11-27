// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { createComponent } from "solid-js";
import type { AccountSetupHost } from "../../bridge";
import AccountSetupDialog from "./AccountSetupDialog";

const fixture = vi.hoisted(() => ({ language: "en" as "en" | "ru" }));
vi.mock("../../i18n", async (original) => {
  const i18n = await original<typeof import("../../i18n")>();
  const ru = (await import("../../i18n.ru.json")).default;
  return { ...i18n, tr: (source: string, ...args: (string | number)[]) =>
    i18n.translateWithCatalog(ru, fixture.language, source, ...args) };
});

afterEach(() => { document.body.replaceChildren(); });

it.each(["en", "ru"] as const)("omits retired Antigravity setup and keeps Codex available in %s", async language => {
  fixture.language = language;
  const machine = (key: string, label: string): AccountSetupHost => ({
    key, machine: label, label, online: true, providers: ["agy", "codex"], listed: ["native-agy"],
    saved: [], deepseek_installed: false,
  });
  const origin = document.body.appendChild(document.createElement("button"));
  const host = document.body.appendChild(document.createElement("div"));
  const confirm = vi.fn();
  const dispose = render(() => createComponent(AccountSetupDialog, {
    prompt: { handle: "test-plan", kind: "add", host: "", provider: "agy", label: "", origin, preferredHost: "",
      hosts: [machine("", "Linux fixture"), machine("mac-fixture", "Mac fixture")] },
    busy: false, close: vi.fn(), confirm,
  }), host);
  const choose = (name: string) => {
    host.querySelector<HTMLElement>("#aa-host")!.click();
    [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(option => option.textContent?.includes(name))!.click();
  };
  try {
    const agents = [...host.querySelectorAll<HTMLButtonElement>(".acct-agent")];
    expect(agents).toHaveLength(1);
    expect(agents[0].textContent).toContain("Codex");
    expect(agents[0].textContent).not.toContain("Antigravity");
    expect(agents[0].getAttribute("aria-selected")).toBe("true");
    const name = host.querySelector<HTMLInputElement>("#aa-name")!;
    name.value = "Work";
    name.dispatchEvent(new Event("input", { bubbles: true }));
    choose("Mac fixture");
    host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    expect(confirm).toHaveBeenCalledExactlyOnceWith({ host: "mac-fixture", provider: "codex", saved: "", label: "Work", api_key: "" }, "test-plan");
  } finally {
    dispose();
  }
});
