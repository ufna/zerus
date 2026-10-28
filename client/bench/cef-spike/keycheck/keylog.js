// Keyboard and focus logger for the zerus-5wmh.27 key check, loaded before the app.
// Reports through the client's own perf_log command (stderr "zerus-client perf: ZKL {...}"),
// so the WebKitGTK and the CEF builds are observed the same way. Synthetic input only.
(() => {
  const invoke = (line) => {
    const t = window.__TAURI_INTERNALS__;
    if (t && t.invoke) t.invoke("perf_log", { line }).catch(() => {});
  };
  const rect = (el) => {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)];
  };
  const state = () => {
    const a = document.activeElement;
    const on = document.querySelector(".rail .on");
    const search = document.querySelector('input[aria-label="Search sessions"]');
    const toast = [...document.querySelectorAll(".toast")].map((t) => t.textContent.trim()).pop() || null;
    return {
      active: a ? (a.getAttribute("aria-label") || a.tagName.toLowerCase() + (a.className && typeof a.className === "string" ? "." + a.className.split(" ")[0] : "")).slice(0, 32) : null,
      hasFocus: document.hasFocus(), visibility: document.visibilityState,
      page: on ? (on.getAttribute("data-tip") || on.textContent.trim()).slice(0, 30) : null,
      search: search ? search.value : null, toast: toast ? toast.slice(0, 40) : null,
    };
  };
  let seq = 0;
  const log = (kind, extra) => invoke("ZKL " + JSON.stringify({ n: ++seq, t: Math.round(performance.now()), kind, ...extra, ...state() }));
  addEventListener("keydown", (e) => {
    log("keydown", { key: e.key, code: e.code, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey });
    // The state once the app has handled the key.
    setTimeout(() => log("after", { key: e.key }), 80);
  }, true);
  addEventListener("focus", () => log("window-focus"));
  addEventListener("blur", () => log("window-blur"));
  document.addEventListener("focusin", () => log("focusin"));
  document.addEventListener("visibilitychange", () => log("visibility"));
  addEventListener("pointerdown", (e) => log("pointerdown", { x: e.clientX, y: e.clientY }), true);
  // Layout for the driver: where to click, in page pixels, until the driver has it.
  const layout = () => invoke("ZKL " + JSON.stringify({ n: ++seq, kind: "layout", inner: [innerWidth, innerHeight], dpr: devicePixelRatio,
    screen: [screenX, screenY],
    searchRect: rect(document.querySelector('input[aria-label="Search sessions"]')),
    listRect: rect(document.querySelector(".sessions-pane .sp-scroll")),
    ...state() }));
  let beats = 0;
  const beat = setInterval(() => { layout(); if (++beats > 600) clearInterval(beat); }, 1000);
  log("armed", { ua: navigator.userAgent.slice(0, 120) });
})();
