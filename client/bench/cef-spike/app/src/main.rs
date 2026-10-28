//! Zerus CEF spike: the smallest Tauri 3 application on the CEF runtime that the
//! Zerus CEF evaluation (Beads zerus-5wmh.27) needs.
//!
//! - `ZERUS_SPIKE_URL=<http url>` loads that page instead of the bundled frontend
//!   (the synthetic pacing harness served by a COOP/COEP server).
//! - Without it the bundled frontend (the Zerus client `dist`) is served by the
//!   Tauri asset protocol at `http://tauri.localhost/`, with the COOP/COEP headers
//!   of `tauri.conf.json`, and a probe script reports `crossOriginIsolated`, the
//!   `performance.now()` step and one `invoke` round trip whose reply is a raw
//!   JSON body (`tauri::ipc::Response`).
//! - `ZERUS_SPIKE_DEBUG_PORT=<port>` starts Chromium's DevTools protocol server on
//!   127.0.0.1:<port> (`Cef::remote_debugging`, which also works in release builds).
//! - `ZERUS_SPIKE_CACHE_DIR=<dir>` is the Chromium profile (default: `spike-profile`
//!   next to the executable). Two instances must not share one: Chromium's process
//!   singleton hands the second one's launch to the first.
//! - `ZERUS_SPIKE_SANDBOX=auto|required|disabled` (default auto).
//! - `ZERUS_SPIKE_CHROMIUM_ARGS="--switch=value --flag"` extra Chromium switches.
//! - `ZERUS_SPIKE_EXIT_AFTER_MS=<ms>` exits that long after the first page probe
//!   (for scripted cold/warm start measurements).
//!
//! Every line the spike prints starts with `zerus-cef-spike` and the milliseconds
//! since `main` started.

use std::{
  path::PathBuf,
  sync::OnceLock,
  time::{Instant, SystemTime, UNIX_EPOCH},
};

use tauri::{WebviewUrl, WebviewWindowBuilder};
use tauri_runtime_cef::{Cef, RemoteDebugging, SandboxPolicy, WebviewWindowBuilderCefExt};

static START: OnceLock<Instant> = OnceLock::new();

fn ms() -> f64 {
  START.get().map(|s| s.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0)
}

fn epoch_ms() -> f64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs_f64() * 1000.0)
    .unwrap_or(0.0)
}

macro_rules! say {
  ($($arg:tt)*) => {
    eprintln!("zerus-cef-spike +{:.1}ms {}", ms(), format!($($arg)*))
  };
}

fn env(name: &str) -> Option<String> {
  std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// A raw JSON body of about `kib` KiB, returned without JSON-in-JS serialization:
/// the same `tauri::ipc::Response` path the client's `next_batch` uses.
#[tauri::command]
fn spike_raw_json(kib: Option<u32>) -> tauri::ipc::Response {
  let kib = kib.unwrap_or(64).min(16 * 1024) as usize;
  let row = r#"{"key":"computer-0/session-00000","name":"synthetic session","state":"idle","unread":0}"#;
  let mut body = String::with_capacity(kib * 1024 + 128);
  body.push_str(r#"{"ok":true,"kind":"raw","rows":["#);
  let mut first = true;
  while body.len() < kib * 1024 {
    if !first {
      body.push(',');
    }
    first = false;
    body.push_str(row);
  }
  body.push_str("]}");
  tauri::ipc::Response::new(body.into_bytes())
}

/// Lets a driver end the run from the page.
#[tauri::command]
fn spike_exit(app: tauri::AppHandle) {
  say!("exit requested by the page");
  app.exit(0);
}

/// The client asks for this first; failing makes it fall back to its built-in replay.
#[tauri::command]
fn start() -> Result<String, String> {
  Err("the CEF spike has no live data source".into())
}

/// Runs in every document before its scripts. On the bundled frontend it also
/// probes cross-origin isolation, the timer step and one raw `invoke`.
const PROBE_SCRIPT: &str = r#"
(() => {
  const say = (tag, value) => console.log('SPIKE_' + tag + ' ' + JSON.stringify(value));
  const timerStep = () => {
    let min = Infinity, last = performance.now();
    const end = last + 30;
    while (true) {
      const now = performance.now();
      if (now > end) break;
      if (now !== last) { min = Math.min(min, now - last); last = now; }
    }
    return min;
  };
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        say('PAINT', { name: e.name, startTime: e.startTime, epoch: performance.timeOrigin + e.startTime, url: location.href });
      }
    }).observe({ type: 'paint', buffered: true });
  } catch (_) {}
  const firstFrame = () => requestAnimationFrame(() => requestAnimationFrame((t) => {
    say('FRAME', { t, epoch: performance.timeOrigin + performance.now(), url: location.href });
  }));
  const probe = async () => {
    const r = {
      url: location.href,
      crossOriginIsolated: self.crossOriginIsolated,
      isSecureContext: self.isSecureContext,
      sharedArrayBuffer: typeof SharedArrayBuffer !== 'undefined',
      timerStepMs: timerStep(),
      userAgent: navigator.userAgent,
      tauri: '__TAURI_INTERNALS__' in self,
    };
    if (r.tauri && location.hostname === 'tauri.localhost') {
      try {
        const t0 = performance.now();
        const reply = await self.__TAURI_INTERNALS__.invoke('spike_raw_json', { kib: 64 });
        const t1 = performance.now();
        r.invoke = {
          replyType: Object.prototype.toString.call(reply),
          bytes: reply && reply.byteLength,
          parsedOk: JSON.parse(new TextDecoder().decode(reply)).ok === true,
          roundTripMs: t1 - t0,
        };
        const times = [];
        for (let i = 0; i < 20; i++) {
          const a = performance.now();
          await self.__TAURI_INTERNALS__.invoke('spike_raw_json', { kib: 1 });
          times.push(performance.now() - a);
        }
        times.sort((a, b) => a - b);
        r.invoke.small1KiBMedianMs = times[10];
        r.invoke.small1KiBMaxMs = times[19];
      } catch (e) {
        r.invoke = { error: String(e) };
      }
    }
    say('PROBE', r);
  };
  // The runtime first loads an internal data: placeholder; only real documents are probed.
  if (self === top && location.protocol !== 'data:') {
    if (document.readyState === 'complete') { firstFrame(); setTimeout(probe, 0); }
    else addEventListener('load', () => { firstFrame(); setTimeout(probe, 0); }, { once: true });
  }
})();
"#;

fn cache_dir() -> PathBuf {
  if let Some(dir) = env("ZERUS_SPIKE_CACHE_DIR") {
    return PathBuf::from(dir);
  }
  std::env::current_exe()
    .ok()
    .and_then(|exe| exe.parent().map(|dir| dir.join("spike-profile")))
    .unwrap_or_else(|| PathBuf::from("spike-profile"))
}

fn chromium_args() -> Vec<(String, Option<String>)> {
  env("ZERUS_SPIKE_CHROMIUM_ARGS")
    .unwrap_or_default()
    .split_whitespace()
    .filter_map(|arg| {
      let arg = arg.trim_start_matches('-');
      if arg.is_empty() {
        return None;
      }
      Some(match arg.split_once('=') {
        Some((k, v)) => (k.to_string(), Some(v.to_string())),
        None => (arg.to_string(), None),
      })
    })
    .collect()
}

#[tauri_runtime_cef::cef_entry_point]
fn main() {
  START.get_or_init(Instant::now);
  say!(
    "start epoch_ms={:.1} pid={} DISPLAY={:?} WAYLAND_DISPLAY={:?} GDK_BACKEND={:?} XDG_SESSION_TYPE={:?}",
    epoch_ms(),
    std::process::id(),
    std::env::var("DISPLAY").ok(),
    std::env::var("WAYLAND_DISPLAY").ok(),
    std::env::var("GDK_BACKEND").ok(),
    std::env::var("XDG_SESSION_TYPE").ok(),
  );

  let cache = cache_dir();
  let debug_port: Option<u16> = env("ZERUS_SPIKE_DEBUG_PORT").and_then(|p| p.parse().ok());
  let url = env("ZERUS_SPIKE_URL");
  let sandbox = match env("ZERUS_SPIKE_SANDBOX").as_deref() {
    Some("required") => SandboxPolicy::Required,
    Some("disabled") => SandboxPolicy::Disabled,
    _ => SandboxPolicy::Auto,
  };
  let extra_args = chromium_args();
  say!(
    "config url={url:?} debug_port={debug_port:?} cache={} sandbox={sandbox:?} chromium_args={extra_args:?}",
    cache.display()
  );

  let mut cef = Cef::default()
    .root_cache_path(&cache)
    .sandbox(sandbox)
    // Lets the coordinator also pass Chromium switches on the command line in release builds.
    .allow_chromium_command_line_args(true)
    .remote_debugging(match debug_port {
      Some(port) => RemoteDebugging::Port {
        port,
        allowed_origins: Vec::new(),
      },
      None => RemoteDebugging::Disabled,
    });
  if !extra_args.is_empty() {
    cef = cef.command_line_args(extra_args);
  }

  let exit_after: Option<u64> = env("ZERUS_SPIKE_EXIT_AFTER_MS").and_then(|v| v.parse().ok());

  tauri::Builder::default()
    .runtime(cef)
    .invoke_handler(tauri::generate_handler![spike_raw_json, spike_exit, start])
    .setup(move |app| {
      say!("setup");
      let webview_url = match &url {
        Some(url) => WebviewUrl::External(url.parse().expect("ZERUS_SPIKE_URL is not a URL")),
        None => WebviewUrl::App("index.html".into()),
      };
      let handle = app.handle().clone();
      // ZERUS_SPIKE_WINDOW_DELAY_MS creates the window later, so the idle cost of the
      // runtime without a browser can be sampled first.
      if let Some(delay) = env("ZERUS_SPIKE_WINDOW_DELAY_MS").and_then(|v| v.parse::<u64>().ok()) {
        let later = handle.clone();
        std::thread::spawn(move || {
          std::thread::sleep(std::time::Duration::from_millis(delay));
          say!("creating the window after ZERUS_SPIKE_WINDOW_DELAY_MS={delay}");
          if let Err(error) = build_window(&later, webview_url, exit_after) {
            say!("window failed: {error}");
          }
        });
        return Ok(());
      }
      let window = build_window(&handle, webview_url, exit_after)?;
      say!("window created label={}", window.label());
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("the CEF spike failed to start");
}

fn build_window(
  app: &tauri::AppHandle,
  webview_url: WebviewUrl,
  exit_after: Option<u64>,
) -> tauri::Result<tauri::WebviewWindow> {
      let handle = app.clone();
      WebviewWindowBuilder::new(app, "main", webview_url)
        .title("Zerus CEF spike")
        .inner_size(1440.0, 900.0)
        .initialization_script(PROBE_SCRIPT)
        .on_console_message(move |message| {
          let text = message.message;
          if text.starts_with("SPIKE_") {
            say!("console {text}");
            if text.starts_with("SPIKE_PROBE") {
              if let Some(after) = exit_after {
                let handle = handle.clone();
                std::thread::spawn(move || {
                  std::thread::sleep(std::time::Duration::from_millis(after));
                  say!("exiting after ZERUS_SPIKE_EXIT_AFTER_MS={after}");
                  handle.exit(0);
                });
              }
            }
          } else if std::env::var_os("ZERUS_SPIKE_ALL_CONSOLE").is_some() {
            say!("console {:?} {text} ({}:{})", message.level, message.source, message.line);
          }
        })
        .build()
}

