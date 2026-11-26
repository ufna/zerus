// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { createComponent } from "solid-js";
import type { AccountSetupHost } from "../../bridge";
import AccountSetupDialog from "./AccountSetupDialog";

const fixture = vi.hoisted(() => ({ language: "en" as "en" | "ru", copy: vi.fn(async () => {}) }));
vi.mock("../../bridge", async (original) => ({ ...(await original<typeof import("../../bridge")>()), copyText: fixture.copy }));
vi.mock("../../i18n", async (original) => {
  const i18n = await original<typeof import("../../i18n")>();
  const ru = (await import("../../i18n.ru.json")).default;
  return { ...i18n, tr: (source: string, ...args: (string | number)[]) =>
    i18n.translateWithCatalog(ru, fixture.language, source, ...args) };
});

afterEach(() => { fixture.copy.mockClear(); document.body.replaceChildren(); });

it.each(["en", "ru"] as const)("keeps the Antigravity sign-in bound to the selected machine in %s", async language => {
  fixture.language = language;
  const machine = (key: string, label: string, extra: Partial<AccountSetupHost> = {}): AccountSetupHost => ({
    key, machine: label, label, online: true, providers: ["codex", "agy"], listed: ["native-agy"],
    saved: [], deepseek_installed: false, antigravity_installed: true, antigravity_sign_in: true, ...extra,
  });
  const origin = document.body.appendChild(document.createElement("button"));
  const host = document.body.appendChild(document.createElement("div"));
  const confirm = vi.fn();
  const dispose = render(() => createComponent(AccountSetupDialog, {
    prompt: { handle: "test-plan", kind: "add", host: "", provider: "codex", label: "", origin, preferredHost: "",
      hosts: [machine("", "Linux fixture"), machine("mac-fixture", "Mac fixture"),
        machine("older", "Earlier CLI", { antigravity_sign_in: false }),
        machine("missing", "Missing CLI", { antigravity_installed: false, antigravity_sign_in: false }),
        machine("offline", "Offline fixture", { online: false })] },
    busy: false, close: vi.fn(), confirm,
  }), host);
  const label = (en: string, ru: string) => language === "ru" ? ru : en;
  const button = (text: string) => [...host.querySelectorAll<HTMLButtonElement>("button")].find(b => b.textContent?.trim() === text);
  const dialog = host.querySelector("dialog")!;
  const choose = (name: string) => {
    host.querySelector<HTMLElement>("#aa-host")!.click();
    [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(option => option.textContent?.includes(name))!.click();
  };
  const submit = () => host.querySelector<HTMLButtonElement>('[type="submit"]');
  try {
    const name = host.querySelector<HTMLInputElement>("#aa-name")!;
    name.value = "Stale custom name";
    name.dispatchEvent(new Event("input", { bubbles: true }));
    [...host.querySelectorAll<HTMLButtonElement>(".acct-agent")].find(b => b.textContent?.includes("Google Antigravity"))!.click();
    expect(host.textContent).toContain(label("Add agent account", "Добавить учётную запись агента"));
    expect(host.textContent).toContain(label("Native Google sign-in", "Вход в Google через Antigravity"));
    expect(host.querySelector("#aa-name")).toBeNull();
    expect(host.querySelector("#aa-saved")).toBeNull();
    expect(confirm).not.toHaveBeenCalled();
    expect(submit()?.textContent).toContain("Linux fixture");
    choose("Mac fixture");
    expect(submit()?.textContent).toContain("Mac fixture");
    dialog.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    expect(confirm).toHaveBeenCalledExactlyOnceWith({ host: "mac-fixture", provider: "agy" }, "test-plan");
    choose("Earlier CLI");
    expect(submit()).toBeNull();
    button(label("Copy sign-in command", "Скопировать команду входа"))!.click();
    await vi.waitFor(() => expect(fixture.copy).toHaveBeenCalledExactlyOnceWith("agy acp --login"));
    await vi.waitFor(() => expect(host.textContent).toContain(label("Copied", "Скопировано")));
    choose("Missing CLI");
    expect(host.textContent).toContain(label("Install Antigravity CLI on Missing CLI", "Установите Antigravity CLI на Missing CLI"));
    expect(button(label("Copy sign-in command", "Скопировать команду входа"))).toBeUndefined();
    choose("Offline fixture");
    expect(submit()?.disabled).toBe(true);
    dialog.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    expect(confirm).toHaveBeenCalledTimes(1);
  } finally {
    dispose();
  }
});
