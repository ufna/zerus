#!/usr/bin/env node
// Runs the client's pacing scenarios, or the client's idle windows, in a
// Chromium engine: Google Chrome, a prebuilt CEF sample (cefsimple) or a CEF
// spike. Beads zerus-5wmh.27, the Chromium counterpart of run-webkitgtk.py.
//
//   node client/bench/run-chromium.mjs --engine chrome --display wayland --hz 144 \
//       --mode harness --dist client/bench/build/harness --out FILE.json
//   node client/bench/run-chromium.mjs --engine cefsimple --binary DIR/cefsimple ...
//   node client/bench/run-chromium.mjs --mode client-idle --dist client/dist ...
//
// It serves --dist from 127.0.0.1 with COOP and COEP (cross-origin isolation
// gives performance.now() its fine resolution), starts the engine with a fresh
// profile and remote debugging on 127.0.0.1, and drives the page over the
// DevTools protocol:
//   harness      globalThis.zerusPerf.run() on the synthetic harness, then 10 s
//                of process-tree CPU at idle (as run-webkitgtk.py)
//   client       the same on the client frontend (?perf=1, built-in replay)
//   client-idle  the client frontend as displayed, with Animate working sessions
//                off, with Reduce motion, and as displayed again: 10 s of
//                process-tree CPU each, the running animations and frames
// --display picks where the window goes:
//   xvfb      the caller's private Xvfb (DISPLAY=:N, WAYLAND_DISPLAY=zerus-none),
//             --ozone-platform=x11; never the desktop
//   wayland   the desktop's compositor, --ozone-platform=wayland
//   xwayland  the desktop through XWayland, --ozone-platform=x11
// Real-display runs only with the coordinator told (client/bench/README.md);
// run-chromium.sh checks the session is unlocked and holds the display lock.
// Profiles, logs and the report go to --work (default: a new directory next to
// --out); nothing reads or writes the user's browser profile.
import { spawn } from "node:child_process";
import { createReadStream, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync, openSync } from "node:fs";
import { createServer } from "node:http";
import { createServer as createNetServer } from "node:net";
import { dirname, extname, join, normalize, resolve, sep } from "node:path";
import { parseArgs } from "node:util";

const { values: opt } = parseArgs({
  options: {
    engine: { type: "string", default: "chrome" },
    binary: { type: "string" },
    display: { type: "string" },
    mode: { type: "string", default: "harness" },
    dist: { type: "string" },
    out: { type: "string" },
    work: { type: "string" },
    label: { type: "string" },
    hz: { type: "string", default: "60" },
    "warmup-ms": { type: "string", default: "2000" },
    "run-ms": { type: "string", default: "10000" },
    runs: { type: "string", default: "3" },
    scenarios: { type: "string" },
    "idle-seconds": { type: "string", default: "10" },
    "settle-seconds": { type: "string", default: "8" },
    timeout: { type: "string", default: "1200" },
    arg: { type: "string", multiple: true, default: [] },
  },
});

const fail = (message) => { console.error(`run-chromium: ${message}`); process.exit(2); };
if (!opt.out) fail("--out is required");
if (!["harness", "client", "client-idle"].includes(opt.mode)) fail(`unknown --mode ${opt.mode}`);
if (!["chrome", "cefsimple", "spike"].includes(opt.engine)) fail(`unknown --engine ${opt.engine}`);
const display = opt.display;
const wayland = process.env.WAYLAND_DISPLAY ?? "";
const x11 = process.env.DISPLAY ?? "";
// The same guard as run-webkitgtk.py: an Xvfb run must never reach the desktop.
if (display === "xvfb") {
  if (!/^:\d+$/.test(x11) || !["", "zerus-none"].includes(wayland) || process.env.ZERUS_BENCH_XVFB !== "1")
    fail(`refusing: xvfb mode needs ZERUS_BENCH_XVFB=1, an X11 DISPLAY (:N) and WAYLAND_DISPLAY=zerus-none (DISPLAY=${x11} WAYLAND_DISPLAY=${wayland})`);
} else if (display === "wayland") {
  if (!wayland || wayland === "zerus-none") fail("wayland mode needs the desktop's WAYLAND_DISPLAY");
} else if (display === "xwayland") {
  if (!x11 || process.env.ZERUS_BENCH_XVFB) fail("xwayland mode needs the desktop's DISPLAY");
} else fail("--display must be xvfb, wayland or xwayland");
const ozone = display === "wayland" ? "wayland" : "x11";
const representative = display !== "xvfb";
const hz = Number(opt.hz);
const label = opt.label ?? `${display}-${opt.engine}-${opt.mode}`;
const dist = resolve(opt.dist ?? (opt.mode === "harness" ? "client/bench/build/harness" : "client/dist"));
if (!existsSync(join(dist, "index.html"))) fail(`no index.html in ${dist}: build it first`);
const out = resolve(opt.out);
const work = resolve(opt.work ?? mkdtempSync(join(dirname(out), "chromium-run-")));
mkdirSync(work, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (line) => console.error(`run-chromium: ${line}`);

// ---- the page server: COOP and COEP on every response, no caching ----
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".png": "image/png", ".svg": "image/svg+xml", ".woff2": "font/woff2" };
const server = createServer((req, res) => {
  const path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  const file = normalize(join(dist, path === "/" ? "index.html" : path));
  const headers = { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp", "Cache-Control": "no-store" };
  if (!file.startsWith(dist + sep) || !existsSync(file) || !statSync(file).isFile()) { res.writeHead(404, headers); res.end(); return; }
  res.writeHead(200, { ...headers, "Content-Type": TYPES[extname(file)] ?? "application/octet-stream" });
  createReadStream(file).pipe(res);
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const origin = `http://127.0.0.1:${server.address().port}`;

async function freePort() {
  const probe = createNetServer();
  await new Promise((r) => probe.listen(0, "127.0.0.1", r));
  const { port } = probe.address();
  await new Promise((r) => probe.close(r));
  return port;
}

// ---- the engine ----
const port = await freePort();
const query = opt.mode === "harness" ? `label=${encodeURIComponent(label)}&representative=${representative ? 1 : 0}` : opt.mode === "client" ? "perf=1" : "";
const url = `${origin}/index.html${query ? `?${query}` : ""}`;
const profile = join(work, "profile");
mkdirSync(profile, { recursive: true });
const common = [
  `--remote-debugging-port=${port}`, "--remote-debugging-address=127.0.0.1", `--ozone-platform=${ozone}`,
  "--no-first-run", "--no-default-browser-check", "--disable-sync", "--disable-extensions",
  "--disable-component-update", "--disable-background-networking", "--password-store=basic",
];
let binary, args;
if (opt.engine === "chrome") {
  binary = opt.binary ?? "/usr/bin/google-chrome-stable";
  args = [`--user-data-dir=${profile}`, ...common, "--window-size=1440,900", `--app=${url}`];
} else if (opt.engine === "cefsimple") {
  if (!opt.binary) fail("--binary is required for cefsimple");
  binary = resolve(opt.binary);
  // cefsimple takes Chromium switches; --url is its start page.
  args = [`--user-data-dir=${profile}`, ...common, `--url=${url}`];
} else {
  // A Tauri app on the CEF runtime built for zerus-5wmh.27: it reads the page, the
  // DevTools port and the profile from the environment and picks X11 itself.
  if (!opt.binary) fail("--binary is required for spike");
  binary = resolve(opt.binary);
  args = [];
}
args.push(...opt.arg);
const engineEnv = opt.engine === "spike"
  ? { ...process.env, ZERUS_SPIKE_URL: url, ZERUS_SPIKE_DEBUG_PORT: String(port), ZERUS_SPIKE_CACHE_DIR: profile }
  : process.env;
const engineLog = join(work, "engine.log");
const logFd = openSync(engineLog, "w");
log(`${opt.engine} on ${display} (ozone ${ozone}, DISPLAY=${x11 || "-"}, WAYLAND_DISPLAY=${wayland || "-"}), mode ${opt.mode}, ${hz} Hz, work ${work}`);
const child = spawn(binary, args, { stdio: ["ignore", logFd, logFd], detached: true, cwd: work, env: engineEnv });
const root = child.pid;
let exited = false;
child.on("exit", (code, signal) => { exited = true; log(`engine exited (${code ?? signal})`); });
const stop = () => { try { process.kill(-root, "SIGTERM"); } catch { /* gone */ } };
process.on("exit", stop);
const cutoff = setTimeout(() => { log(`timeout after ${opt.timeout} s`); stop(); process.exit(1); }, Number(opt.timeout) * 1000);

async function getJson(path) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`);
  return response.json();
}
let version;
for (let i = 0; i < 300 && !version; i++) {
  if (exited) fail(`the engine exited before DevTools came up; see ${engineLog}`);
  try { version = await getJson("/json/version"); } catch { await sleep(100); }
}
if (!version) fail("DevTools did not come up in 30 s");

// ---- a minimal DevTools protocol client ----
function connect(wsUrl) {
  const socket = new WebSocket(wsUrl);
  let next = 1;
  const pending = new Map();
  const listeners = [];
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const { resolve: ok, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) reject(new Error(`${message.error.message} ${message.error.data ?? ""}`)); else ok(message.result);
    } else if (message.method) for (const l of listeners) l(message);
  };
  const ready = new Promise((ok, reject) => { socket.onopen = ok; socket.onerror = () => reject(new Error(`cannot connect ${wsUrl}`)); });
  return {
    ready,
    send(method, params = {}) {
      const id = next++;
      socket.send(JSON.stringify({ id, method, params }));
      return new Promise((ok, reject) => pending.set(id, { resolve: ok, reject }));
    },
    on(listener) { listeners.push(listener); },
    close() { socket.close(); },
  };
}

const browser = connect(version.webSocketDebuggerUrl);
await browser.ready;
let gpu = null;
try {
  const info = await browser.send("SystemInfo.getInfo");
  const aux = info.gpu?.auxAttributes ?? {};
  gpu = {
    devices: (info.gpu?.devices ?? []).map((d) => ({ vendor: d.vendorString || d.vendorId, device: d.deviceString || d.deviceId, driver: d.driverVersion, active: d.active })),
    feature_status: info.gpu?.featureStatus ?? null,
    gl_renderer: aux.glRenderer ?? null, gl_version: aux.glVersion ?? null, gl_implementation: aux.glImplementationParts ?? null,
    display_type: aux.displayType ?? null, ozone_platform: aux.ozonePlatform ?? null,
  };
} catch (error) { gpu = { error: String(error) }; }

let target;
for (let i = 0; i < 300 && !target; i++) {
  const list = await getJson("/json/list").catch(() => []);
  target = list.find((t) => t.type === "page" && t.url.startsWith(origin));
  if (!target) await sleep(100);
}
if (!target) fail("the page target did not appear");
const page = connect(target.webSocketDebuggerUrl);
await page.ready;
const consoleLines = [];
page.on((m) => {
  if (m.method === "Runtime.consoleAPICalled") consoleLines.push(m.params.args.map((a) => a.value ?? a.description ?? "").join(" ").slice(0, 300));
  if (m.method === "Runtime.exceptionThrown") consoleLines.push(`exception: ${m.params.exceptionDetails?.exception?.description ?? m.params.exceptionDetails?.text}`.slice(0, 300));
});
await page.send("Runtime.enable");
await page.send("Page.enable");

async function evaluate(expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
  return result.result.value;
}
async function whenLoaded() {
  for (let i = 0; i < 300; i++) {
    const state = await evaluate("document.readyState").catch(() => null);
    if (state === "complete") return;
    await sleep(100);
  }
  throw new Error("the page did not load");
}

// ---- the process tree: CPU over a window, RSS and PSS, roles from --type ----
const TICKS = 100; // SC_CLK_TCK on Linux x86_64
function stat(pid) {
  try {
    const text = readFileSync(`/proc/${pid}/stat`, "utf8");
    const close = text.lastIndexOf(")");
    const rest = text.slice(close + 2).split(" ");
    return { name: text.slice(text.indexOf("(") + 1, close), ppid: Number(rest[1]), ticks: Number(rest[11]) + Number(rest[12]) };
  } catch { return null; }
}
function kb(path, key) {
  try {
    const line = readFileSync(path, "utf8").split("\n").find((l) => l.startsWith(`${key}:`));
    return line ? Number(line.split(/\s+/)[1]) * 1024 : null;
  } catch { return null; }
}
function role(pid) {
  // Only the --type switch is kept, never the command line. Chromium rewrites
  // the process title, so its arguments can be separated by spaces, not NULs.
  try {
    const type = readFileSync(`/proc/${pid}/cmdline`, "utf8").split(/[\0 ]/).find((a) => a.startsWith("--type="));
    return type ? type.slice(7) : pid === root ? "browser" : "child";
  } catch { return "unknown"; }
}
function tree() {
  const parents = new Map();
  for (const entry of readdirSync("/proc")) {
    if (!/^\d+$/.test(entry)) continue;
    const s = stat(Number(entry));
    if (s) parents.set(Number(entry), s.ppid);
  }
  const members = new Set();
  const queue = [root];
  while (queue.length) {
    const pid = queue.pop();
    if (members.has(pid)) continue;
    members.add(pid);
    for (const [child, parent] of parents) if (parent === pid) queue.push(child);
  }
  const result = new Map();
  for (const pid of members) { const s = stat(pid); if (s) result.set(pid, s); }
  return result;
}
async function sample(seconds) {
  const first = tree();
  const started = performance.now();
  await sleep(seconds * 1000);
  const now = tree();
  const elapsed = (performance.now() - started) / 1000;
  const processes = [];
  for (const [pid, s] of now) {
    const before = first.get(pid);
    processes.push({
      role: role(pid), name: s.name,
      cpu_percent: before ? Math.round(((s.ticks - before.ticks) / TICKS / elapsed) * 100 * 1000) / 1000 : null,
      rss_bytes: kb(`/proc/${pid}/status`, "VmRSS"), pss_bytes: kb(`/proc/${pid}/smaps_rollup`, "Pss"),
    });
  }
  const sum = (key, list = processes) => list.reduce((a, p) => a + (p[key] ?? 0), 0);
  const byRole = {};
  for (const p of processes) {
    const r = (byRole[p.role] ??= { processes: 0, cpu_percent: 0, rss_bytes: 0, pss_bytes: 0 });
    r.processes++; r.cpu_percent = Math.round((r.cpu_percent + (p.cpu_percent ?? 0)) * 1000) / 1000;
    r.rss_bytes += p.rss_bytes ?? 0; r.pss_bytes += p.pss_bytes ?? 0;
  }
  return {
    window_ms: Math.round(elapsed * 1000),
    total: { processes: processes.length, cpu_percent: Math.round(sum("cpu_percent") * 1000) / 1000, rss_bytes: sum("rss_bytes"), pss_bytes: sum("pss_bytes") },
    by_role: byRole, processes,
  };
}
const brief = (s) => `${s.total.cpu_percent.toFixed(1)}% CPU (${Object.entries(s.by_role).map(([r, v]) => `${r} ${v.cpu_percent.toFixed(1)}%`).join(", ")}), PSS ${Math.round(s.total.pss_bytes / 1048576)} MiB`;

// ---- the page facts every mode records ----
const PAGE_FACTS = `({ visibility: document.visibilityState, focus: document.hasFocus(), coi: crossOriginIsolated,
  window: [innerWidth, innerHeight], dpr: devicePixelRatio, ua: navigator.userAgent })`;
const RUNNING = `(() => { const c = {}; for (const a of document.getAnimations()) { if (a.playState !== "running") continue;
  const n = a.animationName || a.constructor.name; c[n] = (c[n] || 0) + 1; } return c; })()`;
const FRAMES = (ms) => `new Promise((ok) => { let n = 0, run = true; const t = () => { if (!run) return; n++; requestAnimationFrame(t); };
  requestAnimationFrame(t); setTimeout(() => { run = false; ok(n); }, ${ms}); })`;

let report;
const plan = { hz, hzSource: "manual", warmupMs: Number(opt["warmup-ms"]), runMs: Number(opt["run-ms"]), runs: Number(opt.runs) };
if (opt.scenarios) plan.ids = opt.scenarios.split(",").filter(Boolean);
try {
  await whenLoaded();
  const facts = await evaluate(PAGE_FACTS);
  log(`page: ${JSON.stringify({ ...facts, ua: undefined })}; ${version.Browser}; GPU ${gpu?.gl_renderer ?? JSON.stringify(gpu?.feature_status ?? gpu)}`);
  if (opt.mode === "harness" || opt.mode === "client") {
    if (opt.mode === "client") await sleep(Number(opt["settle-seconds"]) * 1000);
    const json = await evaluate(`(async () => { const t0 = performance.now();
      while (!globalThis.zerusPerf) { if (performance.now() - t0 > 60000) throw new Error("zerusPerf did not appear"); await new Promise((r) => setTimeout(r, 100)); }
      return JSON.stringify(await globalThis.zerusPerf.run(${JSON.stringify(plan)})); })()`);
    report = JSON.parse(json);
    log("scenarios done; idle sample");
    report.resources = { ...(report.resources ?? {}), idle: await sample(Number(opt["idle-seconds"])) };
    log(`idle: ${brief(report.resources.idle)}`);
  } else {
    // client-idle: each setting through the client's own local-storage key, then a reload.
    const windows = [];
    const variants = [
      ["as displayed", {}],
      ["Animate working sessions off", { "workspace/animateActivity": false }],
      ["Reduce motion", { "workspace/reduceMotion": true }],
      ["as displayed again", {}],
    ];
    for (const [what, settings] of variants) {
      await evaluate(`localStorage.setItem("zerus-client-settings", ${JSON.stringify(JSON.stringify(settings))}), true`);
      await page.send("Page.reload", { ignoreCache: true });
      await sleep(500);
      await whenLoaded();
      await sleep(Number(opt["settle-seconds"]) * 1000);
      const animations = await evaluate(RUNNING);
      const frames = await evaluate(FRAMES(2000));
      await sleep(1000);
      const s = await sample(Number(opt["idle-seconds"]));
      const after = await evaluate(PAGE_FACTS);
      log(`${what}: ${brief(s)}; animations ${JSON.stringify(animations)}; rAF ${frames} in 2 s; ${after.visibility}`);
      windows.push({ what, settings, running_animations: animations, raf_frames_2s: frames, page: { ...after, ua: undefined }, sample: s });
    }
    report = {
      schema: "zerus-idle-v1", created_at: new Date().toISOString(),
      platform: { label, representative, hz, user_agent: facts.ua, cross_origin_isolated: facts.coi, dpr: facts.dpr, window: facts.window },
      windows,
    };
  }
} catch (error) {
  log(`failed: ${error.message}`);
  report = { error: String(error), console: consoleLines.slice(-40) };
  writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`);
  stop();
  process.exit(1);
}
clearTimeout(cutoff);

report.platform = {
  ...report.platform,
  engine: `${version.Browser} (${opt.engine})`,
  chromium: {
    engine: opt.engine, display, ozone, protocol: version["Protocol-Version"],
    gpu, extra_args: opt.arg, env: { DISPLAY: x11 || null, WAYLAND_DISPLAY: wayland || null },
  },
};
report.harness = `${opt.mode === "harness" ? "client/bench/harness (synthetic screen)" : "client frontend (built-in replay)"} in client/bench/run-chromium.mjs`;
report.console = consoleLines.slice(-20);
writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`);
for (const s of report.scenarios ?? []) {
  const w = s.worst?.interval_ms ?? s.worst?.wall_ms ?? s.worst?.latency_ms;
  log(`${s.id}: ${s.skipped ? `skipped (${s.skipped})` : `worst mean ${w?.mean?.toFixed(2)} ms, median ${w?.median?.toFixed(2)} ms${s.verdict ? `, passes ${s.verdict.all}` : ""}`}`);
}
log(`wrote ${out}`);
page.close();
browser.close();
server.close();
stop();
await sleep(1000);
process.exit(0);
