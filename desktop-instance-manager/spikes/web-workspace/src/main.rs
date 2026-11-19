#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, PermissionResponse, WebviewBuilder};
use tauri::window::WindowBuilder;
use tauri::{LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl, Window};
use url::Url;

thread_local! {
    // Drop order in tuple: WebView drops before WebContext.
    static RAW_WRY_CHILDREN: RefCell<HashMap<String, (wry::WebView, wry::WebContext)>> = RefCell::new(HashMap::new());
}

#[derive(Default)]
struct AppState {
    ping_calls: AtomicUsize,
    ping_callers: Mutex<Vec<String>>,
    nav_events: Mutex<Vec<Value>>,
    new_window_events: Mutex<Vec<Value>>,
    permission_events: Mutex<Vec<Value>>,
    download_events: Mutex<Vec<Value>>,
}

#[tauri::command]
fn secret_backend_ping(caller: String, state: tauri::State<'_, AppState>) -> String {
    state.ping_calls.fetch_add(1, Ordering::SeqCst);
    state.ping_callers.lock().unwrap().push(caller.clone());
    format!("PONG_FROM_TAURI_BACKEND:{caller}")
}

struct ReportBus {
    items: Mutex<Vec<Value>>,
    cvar: Condvar,
}

impl ReportBus {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            items: Mutex::new(Vec::new()),
            cvar: Condvar::new(),
        })
    }

    fn push(&self, val: Value) {
        let mut guard = self.items.lock().unwrap();
        guard.push(val);
        self.cvar.notify_all();
    }

    fn wait_for<F>(&self, timeout: Duration, predicate: F) -> Option<Value>
    where
        F: Fn(&Value) -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut guard = self.items.lock().unwrap();
        loop {
            if let Some(pos) = guard.iter().position(&predicate) {
                return Some(guard.remove(pos));
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (g, res) = self.cvar.wait_timeout(guard, deadline - now).unwrap();
            guard = g;
            if res.timed_out() {
                if let Some(pos) = guard.iter().position(&predicate) {
                    return Some(guard.remove(pos));
                }
                return None;
            }
        }
    }
}

fn fixture_html(acct: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>Isolation Fixture - {acct}</title>
</head>
<body>
  <h2>Fixture Account: {acct}</h2>
  <pre id="out">ready</pre>
  <script>
    const ACCT = {acct_json};
    window.__instanceNonce = ACCT + '-' + Math.random().toString(36).slice(2) + '-' + Date.now();
    window.__ticks = 0;
    window.__visEvents = [{{ state: document.visibilityState, t: Date.now() }}];
    setInterval(() => {{ window.__ticks += 1; }}, 50);
    document.addEventListener('visibilitychange', () => {{
      window.__visEvents.push({{ state: document.visibilityState, t: Date.now() }});
    }});

    function idbOpen() {{
      return new Promise((resolve, reject) => {{
        const req = indexedDB.open('khoang_spike_db', 1);
        req.onupgradeneeded = () => {{
          const db = req.result;
          if (!db.objectStoreNames.contains('kv')) {{
            db.createObjectStore('kv', {{ keyPath: 'k' }});
          }}
        }};
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error);
      }});
    }}

    async function idbSet(k, v) {{
      const db = await idbOpen();
      return new Promise((resolve, reject) => {{
        const tx = db.transaction('kv', 'readwrite');
        tx.objectStore('kv').put({{ k, v }});
        tx.oncomplete = () => {{ db.close(); resolve(true); }};
        tx.onerror = () => {{ db.close(); reject(tx.error); }};
      }});
    }}

    async function idbGet(k) {{
      const db = await idbOpen();
      return new Promise((resolve, reject) => {{
        const tx = db.transaction('kv', 'readonly');
        const req = tx.objectStore('kv').get(k);
        req.onsuccess = () => {{ db.close(); resolve(req.result ? req.result.v : null); }};
        req.onerror = () => {{ db.close(); reject(req.error); }};
      }});
    }}

    async function postReport(payload) {{
      await fetch('/report', {{
        method: 'POST',
        headers: {{ 'Content-Type': 'application/json' }},
        body: JSON.stringify(payload)
      }});
    }}

    window.spikeWrite = async function(tag) {{
      localStorage.setItem('account_token', tag);
      document.cookie = 'session_id=' + encodeURIComponent(tag) + '; path=/; max-age=86400; SameSite=Lax';
      await idbSet('vault', tag);
      document.getElementById('out').textContent = 'wrote ' + tag;
      await postReport({{
        kind: 'write',
        acct: ACCT,
        tag,
        localStorage: localStorage.getItem('account_token'),
        cookie: document.cookie,
        idb: await idbGet('vault'),
        nonce: window.__instanceNonce
      }});
    }};

    window.spikeRead = async function(phase) {{
      const idbVal = await idbGet('vault');
      const data = {{
        kind: 'read',
        phase,
        acct: ACCT,
        origin: location.origin,
        localStorage: localStorage.getItem('account_token'),
        cookie: document.cookie,
        idb: idbVal,
        nonce: window.__instanceNonce,
        ticks: window.__ticks,
        visibilityState: document.visibilityState,
        visEventsCount: window.__visEvents.length
      }};
      document.getElementById('out').textContent = JSON.stringify(data, null, 2);
      await postReport(data);
    }};

    window.spikeSecurityProbe = async function(label) {{
      const res = {{
        kind: 'security',
        label,
        acct: ACCT,
        origin: location.origin,
        typeof_TAURI: typeof window.__TAURI__,
        typeof_TAURI_INTERNALS: typeof window.__TAURI_INTERNALS__,
        typeof_TAURI_IPC: typeof window.__TAURI_IPC__,
        typeof_window_ipc: typeof window.ipc,
        typeof_isTauri: typeof window.isTauri,
        isTauri_value: window.isTauri === true,
        internals_invoke_attempted: false,
        internals_invoke_ok: false,
        internals_invoke_result: null,
        internals_invoke_error: null,
        global_tauri_invoke_attempted: false,
        global_tauri_invoke_ok: false,
        global_tauri_invoke_result: null,
        global_tauri_invoke_error: null,
        raw_ipc_fetch_status: null,
        raw_ipc_fetch_body: null,
        raw_ipc_fetch_error: null
      }};

      if (window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === 'function') {{
        res.internals_invoke_attempted = true;
        try {{
          res.internals_invoke_result = await window.__TAURI_INTERNALS__.invoke('secret_backend_ping', {{ caller: label + ':internals' }});
          res.internals_invoke_ok = true;
        }} catch (e) {{
          res.internals_invoke_error = String(e);
        }}
      }}

      if (window.__TAURI__ && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function') {{
        res.global_tauri_invoke_attempted = true;
        try {{
          res.global_tauri_invoke_result = await window.__TAURI__.core.invoke('secret_backend_ping', {{ caller: label + ':global' }});
          res.global_tauri_invoke_ok = true;
        }} catch (e) {{
          res.global_tauri_invoke_error = String(e);
        }}
      }}

      try {{
        const r = await fetch('http://ipc.localhost/secret_backend_ping', {{
          method: 'POST',
          headers: {{ 'Content-Type': 'application/json' }},
          body: JSON.stringify({{ caller: label + ':raw-fetch' }})
        }});
        res.raw_ipc_fetch_status = r.status;
        res.raw_ipc_fetch_body = await r.text();
      }} catch (e) {{
        res.raw_ipc_fetch_error = String(e);
      }}

      await postReport(res);
    }};

    window.spikeTriggerPopupAndNav = async function() {{
      const popupRet = window.open('https://example.com/popup-blocked-test', '_blank');
      const popupBlocked = popupRet === null;
      // Attempt disallowed top-level navigation; if blocked by on_navigation, page stays alive and posts report.
      location.href = 'https://example.com/nav-blocked-test';
      setTimeout(async () => {{
        await postReport({{
          kind: 'nav_popup_after',
          acct: ACCT,
          stillOnFixture: location.hostname === '127.0.0.1',
          href: location.href,
          popupReturnedNull: popupBlocked
        }});
      }}, 400);
    }};

    postReport({{ kind: 'loaded', acct: ACCT, origin: location.origin, nonce: window.__instanceNonce }});
  </script>
</body>
</html>"#,
        acct = acct,
        acct_json = serde_json::to_string(acct).unwrap(),
    )
}

fn start_fixture_server(port: u16, bus: Arc<ReportBus>) {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .unwrap_or_else(|e| panic!("failed to bind fixture HTTP server on 127.0.0.1:{port}: {e}"));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let bus = bus.clone();
            std::thread::spawn(move || {
                handle_http(stream, bus);
            });
        }
    });
}

fn handle_http(mut stream: std::net::TcpStream, bus: Arc<ReportBus>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let mut header_end = None;
    let mut content_length = 0usize;

    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if header_end.is_none() {
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = Some(pos + 4);
                        let headers = String::from_utf8_lossy(&buf[..pos]);
                        for line in headers.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.eq_ignore_ascii_case("content-length") {
                                    content_length = v.trim().parse().unwrap_or(0);
                                }
                            }
                        }
                    }
                }
                if let Some(end) = header_end {
                    if buf.len() >= end + content_length {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }

    let Some(end) = header_end else { return; };
    let header_str = String::from_utf8_lossy(&buf[..end]);
    let first_line = header_str.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");
    let body = &buf[end..buf.len().min(end + content_length)];

    if method == "OPTIONS" {
        let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(resp.as_bytes());
        return;
    }

    if method == "POST" && path == "/report" {
        if let Ok(val) = serde_json::from_slice::<Value>(body) {
            bus.push(val);
        }
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";
        let _ = stream.write_all(resp.as_bytes());
        return;
    }

    if method == "GET" && path.starts_with("/fixture") {
        let acct = path
            .split("acct=")
            .nth(1)
            .map(|s| s.split('&').next().unwrap_or("unknown"))
            .unwrap_or("unknown");
        let html = fixture_html(acct);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );
        let _ = stream.write_all(resp.as_bytes());
        return;
    }

    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found";
    let _ = stream.write_all(resp.as_bytes());
}

fn eval_json(wv: &Webview, js: &str, timeout: Duration) -> Result<Value, String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    wv.eval_with_callback(js, move |res| {
        let _ = tx.send(res);
    })
    .map_err(|e| format!("eval_with_callback error: {e}"))?;
    let raw = rx
        .recv_timeout(timeout)
        .map_err(|_| "eval_with_callback timeout".to_string())?;
    let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
    // If JS returned JSON.stringify(...), unwrap one string layer if valid JSON.
    if let Value::String(s) = &parsed {
        if let Ok(inner) = serde_json::from_str::<Value>(s) {
            return Ok(inner);
        }
    }
    Ok(parsed)
}

fn eval_raw_wry_json(window: &Window, label: &str, js: &str, timeout: Duration) -> Result<Value, String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();
    let label_owned = label.to_string();
    let js_owned = js.to_string();
    window
        .run_on_main_thread(move || {
            RAW_WRY_CHILDREN.with(|cell| {
                let map = cell.borrow();
                if let Some((wv, _)) = map.get(&label_owned) {
                    let tx_cb = tx.clone();
                    if let Err(e) = wv.evaluate_script_with_callback(&js_owned, move |res| {
                        let _ = tx_cb.send(Ok(res));
                    }) {
                        let _ = tx.send(Err(format!("wry evaluate_script_with_callback error: {e}")));
                    }
                } else {
                    let _ = tx.send(Err(format!("raw wry child '{label_owned}' not found")));
                }
            });
        })
        .map_err(|e| format!("run_on_main_thread error: {e}"))?;

    let raw = rx
        .recv_timeout(timeout)
        .map_err(|_| "raw wry eval timeout".to_string())??;
    let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
    if let Value::String(s) = &parsed {
        if let Ok(inner) = serde_json::from_str::<Value>(s) {
            return Ok(inner);
        }
    }
    Ok(parsed)
}

fn create_raw_wry_child(
    window: &Window,
    label: &str,
    url: &str,
    data_dir: PathBuf,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    visible: bool,
    state: Arc<Mutex<Vec<Value>>>,
    new_win_log: Arc<Mutex<Vec<Value>>>,
    allowed_hosts: Vec<String>,
) -> Result<(), String> {
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let win_clone = window.clone();
    let label_owned = label.to_string();
    let url_owned = url.to_string();
    window
        .run_on_main_thread(move || {
            let mut web_context = wry::WebContext::new(Some(data_dir));
            let nav_log = state.clone();
            let nav_label = label_owned.clone();
            let nw_log = new_win_log.clone();
            let nw_label = label_owned.clone();
            let res = wry::WebViewBuilder::new_with_web_context(&mut web_context)
                .with_url(&url_owned)
                .with_visible(visible)
                .with_clipboard(true)
                .with_bounds(wry::Rect {
                    position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(x, y)),
                    size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(w, h)),
                })
                .with_navigation_handler(move |target| {
                    let allowed = Url::parse(&target)
                        .ok()
                        .and_then(|u| u.host_str().map(|h| h.to_string()))
                        .map(|host| {
                            allowed_hosts
                                .iter()
                                .any(|a| host == *a || host.ends_with(&format!(".{a}")))
                        })
                        .unwrap_or(target == "about:blank");
                    nav_log.lock().unwrap().push(json!({
                        "webview": nav_label,
                        "url": target,
                        "allowed": allowed
                    }));
                    allowed
                })
                .with_new_window_req_handler(move |target, _features| {
                    nw_log.lock().unwrap().push(json!({
                        "webview": nw_label,
                        "url": target,
                        "action": "deny_route_os_browser"
                    }));
                    wry::NewWindowResponse::Deny
                })
                .build_as_child(&win_clone);
            match res {
                Ok(wv) => {
                    RAW_WRY_CHILDREN.with(|cell| {
                        cell.borrow_mut().insert(label_owned, (wv, web_context));
                    });
                    let _ = tx.send(Ok(()));
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("raw wry build_as_child failed: {e}")));
                }
            }
        })
        .map_err(|e| format!("run_on_main_thread error: {e}"))?;
    rx.recv_timeout(Duration::from_secs(30))
        .map_err(|_| "timeout creating raw wry child".to_string())?
}

fn measure_process_memory(data_root: &Path, self_pid: u32) -> Value {
    let root_str = data_root.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"
$root = '{root_str}'
$selfPid = {self_pid}
$procs = Get-CimInstance Win32_Process | Where-Object {{
  $_.ProcessId -eq $selfPid -or ($_.Name -eq 'msedgewebview2.exe' -and $_.CommandLine -and $_.CommandLine.IndexOf($root, [System.StringComparison]::OrdinalIgnoreCase) -ge 0)
}}
$list = @()
foreach ($p in $procs) {{
  $procObj = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
  $ws = if ($procObj) {{ $procObj.WorkingSet64 }} else {{ [int64]$p.WorkingSetSize }}
  $priv = if ($procObj) {{ $procObj.PrivateMemorySize64 }} else {{ [int64]$p.PrivatePageCount }}
  $udir = ''
  if ($p.CommandLine -match '--user-data-dir="([^"]+)"') {{ $udir = $Matches[1] }}
  elseif ($p.CommandLine -match '--user-data-dir=([^\s]+)') {{ $udir = $Matches[1] }}
  $ptype = 'browser'
  if ($p.Name -ne 'msedgewebview2.exe') {{ $ptype = 'host' }}
  elseif ($p.CommandLine -match '--type=([^\s]+)') {{ $ptype = $Matches[1] }}
  $list += [pscustomobject]@{{
    pid = $p.ProcessId
    name = $p.Name
    type = $ptype
    userDataDir = $udir
    wsBytes = $ws
    privBytes = $priv
  }}
}}
$wv = @($list | Where-Object {{ $_.name -eq 'msedgewebview2.exe' }})
$hostP = @($list | Where-Object {{ $_.pid -eq $selfPid }})
$summary = [pscustomobject]@{{
  hostWsMb = [math]::Round((($hostP | Measure-Object -Property wsBytes -Sum).Sum) / 1MB, 2)
  hostPrivMb = [math]::Round((($hostP | Measure-Object -Property privBytes -Sum).Sum) / 1MB, 2)
  webview2ProcessCount = $wv.Count
  webview2WsMb = [math]::Round((($wv | Measure-Object -Property wsBytes -Sum).Sum) / 1MB, 2)
  webview2PrivMb = [math]::Round((($wv | Measure-Object -Property privBytes -Sum).Sum) / 1MB, 2)
  totalWsMb = [math]::Round((($list | Measure-Object -Property wsBytes -Sum).Sum) / 1MB, 2)
  totalPrivMb = [math]::Round((($list | Measure-Object -Property privBytes -Sum).Sum) / 1MB, 2)
  byDataDir = @(
    $wv | Group-Object userDataDir | ForEach-Object {{
      [pscustomobject]@{{
        dir = Split-Path -Leaf $_.Name
        procs = $_.Count
        wsMb = [math]::Round((($_.Group | Measure-Object -Property wsBytes -Sum).Sum) / 1MB, 2)
        privMb = [math]::Round((($_.Group | Measure-Object -Property privBytes -Sum).Sum) / 1MB, 2)
      }}
    }}
  )
}}
$summary | ConvertTo-Json -Depth 5 -Compress
"#
    );

    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let txt = String::from_utf8_lossy(&o.stdout);
            serde_json::from_str(txt.trim()).unwrap_or_else(|_| json!({ "raw": txt.trim() }))
        }
        Ok(o) => json!({ "error": String::from_utf8_lossy(&o.stderr).trim() }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

fn stats_ms(samples: &[f64]) -> Value {
    if samples.is_empty() {
        return json!({});
    }
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = s.iter().sum::<f64>() / s.len() as f64;
    let p50 = s[s.len() / 2];
    let p95 = s[((s.len() as f64 * 0.95).ceil() as usize).saturating_sub(1).min(s.len() - 1)];
    json!({
        "n": s.len(),
        "min_ms": (s[0] * 1000.0).round() / 1000.0,
        "median_ms": (p50 * 1000.0).round() / 1000.0,
        "mean_ms": (mean * 1000.0).round() / 1000.0,
        "p95_ms": (p95 * 1000.0).round() / 1000.0,
        "max_ms": (*s.last().unwrap() * 1000.0).round() / 1000.0,
    })
}

fn parse_args() -> (String, PathBuf, u16, PathBuf) {
    let args: Vec<String> = std::env::args().collect();
    let mut mode = "isolation-write".to_string();
    let mut data_root = std::env::temp_dir().join("khoang-web-spike-data");
    let mut port: u16 = 18432;
    let mut report_out = std::env::temp_dir().join("khoang-web-spike-report.json");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" if i + 1 < args.len() => {
                mode = args[i + 1].clone();
                i += 2;
            }
            "--data-root" if i + 1 < args.len() => {
                data_root = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(18432);
                i += 2;
            }
            "--out" if i + 1 < args.len() => {
                report_out = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            _ => i += 1,
        }
    }
    (mode, data_root, port, report_out)
}

fn main() {
    let (mode, data_root, port, report_out) = parse_args();
    std::fs::create_dir_all(&data_root).expect("failed to create data_root");

    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![secret_backend_ping])
        .setup(move |app| {
            let handle = app.handle().clone();
            let window = WindowBuilder::new(app, "main")
                .title(format!("KHOANG Option B Spike ({mode})"))
                .inner_size(1100.0, 720.0)
                .build()?;

            let rail_dir = data_root.join("rail");
            std::fs::create_dir_all(&rail_dir)?;
            let rail = window.add_child(
                WebviewBuilder::new("rail", WebviewUrl::App("index.html".into()))
                    .data_directory(rail_dir),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(220.0, 720.0),
            )?;

            std::thread::spawn(move || {
                let result = match mode.as_str() {
                    "isolation-write" => run_isolation_write(&handle, &window, &rail, &data_root, port),
                    "isolation-verify" => run_isolation_verify(&handle, &window, &rail, &data_root, port),
                    "zalo-live" => run_zalo_live(&handle, &window, &rail, &data_root),
                    "deep-probe" => run_deep_probe(&handle, &window, &data_root, port),
                    other => json!({ "error": format!("unknown mode: {other}") }),
                };

                let pretty = serde_json::to_string_pretty(&result).unwrap();
                let _ = std::fs::write(&report_out, &pretty);
                println!("=== SPIKE_RESULT_BEGIN ===");
                println!("{pretty}");
                println!("=== SPIKE_RESULT_END ===");

                // Gracefully drop raw wry children on main thread & close Tauri webviews so storage flushes cleanly.
                let (tx, rx) = std::sync::mpsc::channel::<()>();
                let _ = window.run_on_main_thread(move || {
                    RAW_WRY_CHILDREN.with(|c| c.borrow_mut().clear());
                    let _ = tx.send(());
                });
                let _ = rx.recv_timeout(Duration::from_secs(5));

                for wv in window.webviews() {
                    let _ = wv.close();
                }
                std::thread::sleep(Duration::from_millis(600));
                handle.exit(0);
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri spike application");
}

fn build_fixture_child(
    window: &Window,
    app: &tauri::AppHandle,
    label: &str,
    url_str: &str,
    data_dir: PathBuf,
    visible: bool,
) -> Result<Webview, String> {
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let app_nav = app.clone();
    let lbl_nav = label.to_string();
    let app_nw = app.clone();
    let lbl_nw = label.to_string();

    let builder = WebviewBuilder::new(label, WebviewUrl::External(Url::parse(url_str).unwrap()))
        .data_directory(data_dir)
        .disable_drag_drop_handler()
        .enable_clipboard_access()
        .on_navigation(move |target| {
            let allowed = target.host_str() == Some("127.0.0.1") || target.as_str() == "about:blank";
            app_nav.state::<AppState>().nav_events.lock().unwrap().push(json!({
                "webview": lbl_nav,
                "url": target.as_str(),
                "allowed": allowed
            }));
            allowed
        })
        .on_new_window(move |target, _features| {
            app_nw.state::<AppState>().new_window_events.lock().unwrap().push(json!({
                "webview": lbl_nw,
                "url": target.as_str(),
                "action": "deny_route_os_browser"
            }));
            NewWindowResponse::Deny
        });

    let wv = window
        .add_child(
            builder,
            LogicalPosition::new(220.0, 0.0),
            LogicalSize::new(880.0, 720.0),
        )
        .map_err(|e| format!("add_child({label}) failed: {e}"))?;

    if !visible {
        wv.hide().map_err(|e| format!("hide({label}) failed: {e}"))?;
    }
    Ok(wv)
}

fn run_isolation_write(
    app: &tauri::AppHandle,
    window: &Window,
    rail: &Webview,
    data_root: &Path,
    port: u16,
) -> Value {
    let bus = ReportBus::new();
    start_fixture_server(port, bus.clone());

    // Wait a moment for local rail to call secret_backend_ping
    std::thread::sleep(Duration::from_millis(1200));
    let rail_report = eval_json(rail, "JSON.stringify(window.__RAIL_REPORT__ || null)", Duration::from_secs(5))
        .unwrap_or_else(|e| json!({ "error": e }));

    let url_a = format!("http://127.0.0.1:{port}/fixture?acct=A");
    let url_b = format!("http://127.0.0.1:{port}/fixture?acct=B");
    let url_wry = format!("http://127.0.0.1:{port}/fixture?acct=WryRaw");

    let wv_a = build_fixture_child(window, app, "acct-a", &url_a, data_root.join("acct-a"), true).unwrap();
    let wv_b = build_fixture_child(window, app, "acct-b", &url_b, data_root.join("acct-b"), false).unwrap();

    let raw_nav_log = Arc::new(Mutex::new(Vec::new()));
    let raw_nw_log = Arc::new(Mutex::new(Vec::new()));
    let raw_wry_created = create_raw_wry_child(
        window,
        "raw-wry-c",
        &url_wry,
        data_root.join("raw-wry-c"),
        220.0,
        0.0,
        880.0,
        720.0,
        false,
        raw_nav_log.clone(),
        raw_nw_log.clone(),
        vec!["127.0.0.1".to_string()],
    );

    let loaded_a = bus.wait_for(Duration::from_secs(20), |v| v["kind"] == "loaded" && v["acct"] == "A");
    let loaded_b = bus.wait_for(Duration::from_secs(20), |v| v["kind"] == "loaded" && v["acct"] == "B");
    let loaded_wry = bus.wait_for(Duration::from_secs(20), |v| v["kind"] == "loaded" && v["acct"] == "WryRaw");

    // Step 1: Write distinct data in A
    let _ = wv_a.eval("window.spikeWrite('ALPHA_SECRET_111')");
    let write_a = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "write" && v["acct"] == "A");

    // Step 2: Verify B does NOT see A's localStorage / cookie / IndexedDB
    let _ = wv_b.eval("window.spikeRead('before_b_write')");
    let read_b_before = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "B" && v["phase"] == "before_b_write");

    // Step 3: Write distinct data in B
    let _ = wv_b.eval("window.spikeWrite('BETA_SECRET_222')");
    let write_b = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "write" && v["acct"] == "B");

    // Step 4: Re-read A to prove B did not overwrite A
    let _ = wv_a.eval("window.spikeRead('after_b_write')");
    let read_a_after = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "A" && v["phase"] == "after_b_write");

    // Step 5: Security boundary probe on Tauri External child (acct-a)
    let _ = wv_a.eval("window.spikeSecurityProbe('tauri-external-acct-a')");
    let sec_tauri_external = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "security" && v["label"] == "tauri-external-acct-a");

    // Step 6: Security boundary probe on Raw Wry child (raw-wry-c)
    let _ = window.run_on_main_thread(|| {
        RAW_WRY_CHILDREN.with(|c| {
            if let Some((wv, _)) = c.borrow().get("raw-wry-c") {
                let _ = wv.evaluate_script("window.spikeSecurityProbe('raw-wry-child')");
            }
        });
    });
    let sec_raw_wry = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "security" && v["label"] == "raw-wry-child");

    // Step 7: Navigation & popup policy probe on acct-a and raw-wry-c
    let _ = wv_a.eval("window.spikeTriggerPopupAndNav()");
    let nav_popup_a = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "nav_popup_after" && v["acct"] == "A");

    let _ = window.run_on_main_thread(|| {
        RAW_WRY_CHILDREN.with(|c| {
            if let Some((wv, _)) = c.borrow().get("raw-wry-c") {
                let _ = wv.evaluate_script("window.spikeTriggerPopupAndNav()");
            }
        });
    });
    let nav_popup_raw = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "nav_popup_after" && v["acct"] == "WryRaw");

    // Flush delay so Chromium commits LevelDB/SQLite cookies & IndexedDB to disk
    std::thread::sleep(Duration::from_millis(800));

    let state = app.state::<AppState>();
    let ping_calls = state.ping_calls.load(Ordering::SeqCst);
    let ping_callers = state.ping_callers.lock().unwrap().clone();
    let nav_events = state.nav_events.lock().unwrap().clone();
    let new_window_events = state.new_window_events.lock().unwrap().clone();

    json!({
        "mode": "isolation-write",
        "port": port,
        "rail_report": rail_report,
        "loaded": { "a": loaded_a, "b": loaded_b, "raw_wry": loaded_wry, "raw_wry_created": raw_wry_created.is_ok() },
        "isolation": {
            "write_a": write_a,
            "read_b_before_write": read_b_before,
            "write_b": write_b,
            "read_a_after_b_write": read_a_after
        },
        "security_boundary": {
            "tauri_external_child": sec_tauri_external,
            "raw_wry_child": sec_raw_wry,
            "backend_ping_calls_total": ping_calls,
            "backend_ping_callers": ping_callers,
            "tauri_nav_popup_result": nav_popup_a,
            "tauri_nav_events": nav_events,
            "tauri_new_window_events": new_window_events,
            "raw_wry_nav_popup_result": nav_popup_raw,
            "raw_wry_nav_events": *raw_nav_log.lock().unwrap(),
            "raw_wry_new_window_events": *raw_nw_log.lock().unwrap()
        }
    })
}

fn run_isolation_verify(
    app: &tauri::AppHandle,
    window: &Window,
    _rail: &Webview,
    data_root: &Path,
    port: u16,
) -> Value {
    let bus = ReportBus::new();
    start_fixture_server(port, bus.clone());

    let url_a = format!("http://127.0.0.1:{port}/fixture?acct=A");
    let url_b = format!("http://127.0.0.1:{port}/fixture?acct=B");

    let wv_a = build_fixture_child(window, app, "acct-a", &url_a, data_root.join("acct-a"), true).unwrap();
    let wv_b = build_fixture_child(window, app, "acct-b", &url_b, data_root.join("acct-b"), false).unwrap();

    let loaded_a = bus.wait_for(Duration::from_secs(20), |v| v["kind"] == "loaded" && v["acct"] == "A");
    let loaded_b = bus.wait_for(Duration::from_secs(20), |v| v["kind"] == "loaded" && v["acct"] == "B");

    // Verify persisted values after full process restart WITHOUT writing anything!
    let _ = wv_a.eval("window.spikeRead('post_restart')");
    let read_a = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "A" && v["phase"] == "post_restart");
    let _ = wv_b.eval("window.spikeRead('post_restart')");
    let read_b = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "B" && v["phase"] == "post_restart");

    let nonce_a_before = read_a.as_ref().and_then(|v| v["nonce"].as_str()).unwrap_or("").to_string();
    let nonce_b_before = read_b.as_ref().and_then(|v| v["nonce"].as_str()).unwrap_or("").to_string();
    let ticks_b_before = read_b.as_ref().and_then(|v| v["ticks"].as_u64()).unwrap_or(0);

    // Wait 500ms while B is hidden to see if B's timers still tick when hidden via hide()
    std::thread::sleep(Duration::from_millis(500));
    let _ = wv_b.eval("window.spikeRead('while_hidden_500ms')");
    let read_b_while_hidden = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "B" && v["phase"] == "while_hidden_500ms");

    // Benchmark 1: hide()/show() switching (20 switches)
    let mut hide_show_samples = Vec::new();
    for i in 0..20 {
        let t0 = Instant::now();
        if i % 2 == 0 {
            let _ = wv_a.hide();
            let _ = wv_b.show();
            let _ = wv_b.set_focus();
        } else {
            let _ = wv_b.hide();
            let _ = wv_a.show();
            let _ = wv_a.set_focus();
        }
        hide_show_samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_millis(25));
    }

    let _ = wv_a.eval("window.spikeRead('after_hide_show')");
    let read_a_after_hs = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "A" && v["phase"] == "after_hide_show");
    let _ = wv_b.eval("window.spikeRead('after_hide_show')");
    let read_b_after_hs = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "B" && v["phase"] == "after_hide_show");

    // Benchmark 2: Offscreen bounds switching (both shown, inactive moved to (-10000, -10000))
    let _ = wv_a.show();
    let _ = wv_b.show();
    let _ = wv_b.set_position(LogicalPosition::new(-10000.0, -10000.0));
    std::thread::sleep(Duration::from_millis(300));
    let _ = wv_b.eval("window.spikeRead('while_offscreen')");
    let read_b_offscreen = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "B" && v["phase"] == "while_offscreen");

    let mut offscreen_samples = Vec::new();
    for i in 0..20 {
        let t0 = Instant::now();
        if i % 2 == 0 {
            let _ = wv_a.set_position(LogicalPosition::new(-10000.0, -10000.0));
            let _ = wv_b.set_position(LogicalPosition::new(220.0, 0.0));
            let _ = wv_b.set_focus();
        } else {
            let _ = wv_b.set_position(LogicalPosition::new(-10000.0, -10000.0));
            let _ = wv_a.set_position(LogicalPosition::new(220.0, 0.0));
            let _ = wv_a.set_focus();
        }
        offscreen_samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_millis(25));
    }

    let _ = wv_a.eval("window.spikeRead('after_offscreen')");
    let read_a_after_off = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "A" && v["phase"] == "after_offscreen");
    let _ = wv_b.eval("window.spikeRead('after_offscreen')");
    let read_b_after_off = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "B" && v["phase"] == "after_offscreen");

    let nonce_a_final = read_a_after_off.as_ref().and_then(|v| v["nonce"].as_str()).unwrap_or("");
    let nonce_b_final = read_b_after_off.as_ref().and_then(|v| v["nonce"].as_str()).unwrap_or("");

    json!({
        "mode": "isolation-verify",
        "loaded": { "a": loaded_a, "b": loaded_b },
        "persistence_after_restart": {
            "a": read_a,
            "b": read_b
        },
        "switch_tests": {
            "ticks_b_before_500ms_hidden": ticks_b_before,
            "read_b_while_hidden_500ms": read_b_while_hidden,
            "hide_show_latency": stats_ms(&hide_show_samples),
            "after_hide_show": { "a": read_a_after_hs, "b": read_b_after_hs },
            "read_b_while_offscreen": read_b_offscreen,
            "offscreen_bounds_latency": stats_ms(&offscreen_samples),
            "after_offscreen": { "a": read_a_after_off, "b": read_b_after_off },
            "no_reload_a": !nonce_a_before.is_empty() && nonce_a_before == nonce_a_final,
            "no_reload_b": !nonce_b_before.is_empty() && nonce_b_before == nonce_b_final
        }
    })
}

fn spawn_zalo_tauri_child(
    window: &Window,
    app: &tauri::AppHandle,
    label: &str,
    data_dir: PathBuf,
    visible: bool,
    with_telemetry_listener: bool,
) -> Result<(Webview, Arc<(Mutex<Vec<String>>, Condvar)>), String> {
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let load_events = Arc::new((Mutex::new(Vec::<String>::new()), Condvar::new()));
    let le_cb = load_events.clone();

    let app_nav = app.clone();
    let lbl_nav = label.to_string();
    let app_nw = app.clone();
    let lbl_nw = label.to_string();
    let app_perm = app.clone();
    let lbl_perm = label.to_string();
    let app_dl = app.clone();
    let lbl_dl = label.to_string();

    let mut builder = WebviewBuilder::new(label, WebviewUrl::External(Url::parse("https://chat.zalo.me").unwrap()))
        .data_directory(data_dir)
        .disable_drag_drop_handler()
        .enable_clipboard_access()
        .on_navigation(move |target| {
            let host = target.host_str().unwrap_or("");
            let allowed = host == "chat.zalo.me"
                || host == "id.zalo.me"
                || host.ends_with(".zalo.me")
                || host.ends_with(".zaloapp.com")
                || target.as_str() == "about:blank";
            app_nav.state::<AppState>().nav_events.lock().unwrap().push(json!({
                "webview": lbl_nav,
                "url": target.as_str(),
                "allowed": allowed
            }));
            allowed
        })
        .on_new_window(move |target, _features| {
            app_nw.state::<AppState>().new_window_events.lock().unwrap().push(json!({
                "webview": lbl_nw,
                "url": target.as_str(),
                "action": "deny_route_os_browser"
            }));
            NewWindowResponse::Deny
        })
        .on_permission_request(move |_wv, kind| {
            app_perm.state::<AppState>().permission_events.lock().unwrap().push(json!({
                "webview": lbl_perm,
                "kind": format!("{kind:?}")
            }));
            PermissionResponse::Default
        })
        .on_download(move |_wv, event| {
            let desc = match &event {
                DownloadEvent::Requested { url, destination } => {
                    format!("Requested({}, {})", url, destination.display())
                }
                DownloadEvent::Finished { url, path, success } => {
                    format!("Finished({}, {:?}, {})", url, path, success)
                }
                _ => "Other".to_string(),
            };
            app_dl.state::<AppState>().download_events.lock().unwrap().push(json!({
                "webview": lbl_dl,
                "event": desc
            }));
            false
        })
        .on_page_load(move |_wv, payload| {
            if matches!(payload.event(), PageLoadEvent::Finished) {
                let (m, cv) = &*le_cb;
                m.lock().unwrap().push(payload.url().as_str().to_string());
                cv.notify_all();
            }
        });

    if with_telemetry_listener {
        builder = builder.initialization_script(
            r#"
            (function() {
              window.__SPIKE_ERRORS__ = [];
              window.__SPIKE_CSP__ = [];
              window.__SPIKE_CONSOLE_ERRORS__ = [];
              window.addEventListener('error', (e) => {
                window.__SPIKE_ERRORS__.push({ msg: e.message, src: e.filename, line: e.lineno });
              });
              window.addEventListener('unhandledrejection', (e) => {
                window.__SPIKE_ERRORS__.push({ msg: 'unhandledrejection: ' + String(e.reason) });
              });
              window.addEventListener('securitypolicyviolation', (e) => {
                window.__SPIKE_CSP__.push({ directive: e.violatedDirective, blockedURI: e.blockedURI });
              });
              const origErr = console.error;
              console.error = function(...args) {
                try {
                  window.__SPIKE_CONSOLE_ERRORS__.push(args.map(a => String(a)).join(' ').slice(0, 300));
                } catch (_) {}
                return origErr.apply(this, args);
              };
            })();
            "#,
        );
    }

    let wv = window
        .add_child(
            builder,
            LogicalPosition::new(220.0, 0.0),
            LogicalSize::new(880.0, 720.0),
        )
        .map_err(|e| format!("add_child({label}) failed: {e}"))?;

    if !visible {
        wv.hide().map_err(|e| format!("hide({label}) failed: {e}"))?;
    }
    Ok((wv, load_events))
}

fn wait_page_finished(ev: &Arc<(Mutex<Vec<String>>, Condvar)>, timeout: Duration) -> Vec<String> {
    let deadline = Instant::now() + timeout;
    let (m, cv) = &**ev;
    let mut g = m.lock().unwrap();
    while g.is_empty() {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let (ng, res) = cv.wait_timeout(g, deadline - now).unwrap();
        g = ng;
        if res.timed_out() {
            break;
        }
    }
    g.clone()
}

fn zalo_inspect_js() -> &'static str {
    r#"
    JSON.stringify((function() {
      const imgs = Array.from(document.querySelectorAll('img')).map(i => ({
        w: i.naturalWidth || i.clientWidth,
        h: i.naturalHeight || i.clientHeight,
        alt: (i.alt || '').slice(0, 60),
        srcPrefix: (i.src || '').slice(0, 60)
      }));
      const canvases = Array.from(document.querySelectorAll('canvas')).map(c => ({
        w: c.width,
        h: c.height,
        cw: c.clientWidth,
        ch: c.clientHeight
      }));
      const text = (document.body && document.body.innerText ? document.body.innerText.trim() : '');
      const hasQrImageOrCanvas = imgs.some(i => i.w >= 100 && i.h >= 100) || canvases.some(c => c.w >= 100 && c.h >= 100);
      return {
        href: location.href,
        origin: location.origin,
        title: document.title,
        readyState: document.readyState,
        userAgent: navigator.userAgent,
        isSecureContext: window.isSecureContext,
        bodyTextLen: text.length,
        bodyTextSample: text.slice(0, 260),
        imgCount: imgs.length,
        largeImgs: imgs.filter(i => i.w >= 80 && i.h >= 80).slice(0, 6),
        canvasCount: canvases.length,
        canvases: canvases.slice(0, 4),
        hasQrImageOrCanvas,
        capabilities: {
          serviceWorker: 'serviceWorker' in navigator,
          notification: 'Notification' in window,
          notificationPermission: ('Notification' in window) ? Notification.permission : null,
          mediaDevices: Boolean(navigator.mediaDevices && navigator.mediaDevices.getUserMedia),
          rtcPeerConnection: 'RTCPeerConnection' in window,
          clipboard: Boolean(navigator.clipboard),
          indexedDB: 'indexedDB' in window,
          webLocks: Boolean(navigator.locks)
        },
        tauriSurface: {
          typeof_TAURI: typeof window.__TAURI__,
          typeof_TAURI_INTERNALS: typeof window.__TAURI_INTERNALS__,
          typeof_window_ipc: typeof window.ipc,
          isTauri: Boolean(window.isTauri)
        },
        errors: window.__SPIKE_ERRORS__ || null,
        cspViolations: window.__SPIKE_CSP__ || null,
        consoleErrors: window.__SPIKE_CONSOLE_ERRORS__ || null
      };
    })())
    "#
}

fn run_zalo_live(
    app: &tauri::AppHandle,
    window: &Window,
    _rail: &Webview,
    data_root: &Path,
) -> Value {
    let self_pid = std::process::id();
    std::thread::sleep(Duration::from_millis(1500));
    let mem_0_children = measure_process_memory(data_root, self_pid);

    // 1st Zalo child (Tauri WebviewBuilder with telemetry listener)
    let t_start_1 = Instant::now();
    let (zalo_1, ev_1) = spawn_zalo_tauri_child(window, app, "zalo-1", data_root.join("zalo-1"), true, true).unwrap();
    let _ = wait_page_finished(&ev_1, Duration::from_secs(20));
    // Wait for SPA redirect (chat.zalo.me -> id.zalo.me) and QR render
    std::thread::sleep(Duration::from_secs(6));
    let zalo_1_load_ms = (t_start_1.elapsed().as_secs_f64() * 1000.0).round();
    let zalo_1_loads = ev_1.0.lock().unwrap().clone();
    let zalo_1_inspect = eval_json(&zalo_1, zalo_inspect_js(), Duration::from_secs(10))
        .unwrap_or_else(|e| json!({ "error": e }));
    // Plant an in-memory marker to verify zero reload on hide/show
    let _ = zalo_1.eval("window.__ZALO_SPIKE_MARKER__ = 'ALIVE_ZALO_1';");
    // Test window.open interception on real Zalo WebView
    let _ = zalo_1.eval("window.open('https://example.com/zalo-popup-check', '_blank');");
    std::thread::sleep(Duration::from_millis(400));
    let mem_1_child = measure_process_memory(data_root, self_pid);

    // 2nd Zalo child (Tauri WebviewBuilder, NO custom init script)
    let t_start_2 = Instant::now();
    let (zalo_2, ev_2) = spawn_zalo_tauri_child(window, app, "zalo-2", data_root.join("zalo-2"), true, false).unwrap();
    let _ = zalo_1.hide();
    let _ = wait_page_finished(&ev_2, Duration::from_secs(20));
    std::thread::sleep(Duration::from_secs(6));
    let zalo_2_load_ms = (t_start_2.elapsed().as_secs_f64() * 1000.0).round();
    let zalo_2_inspect = eval_json(&zalo_2, zalo_inspect_js(), Duration::from_secs(10))
        .unwrap_or_else(|e| json!({ "error": e }));
    let _ = zalo_2.eval("window.__ZALO_SPIKE_MARKER__ = 'ALIVE_ZALO_2';");
    let mem_2_children = measure_process_memory(data_root, self_pid);

    // Switch latency between real Zalo child 1 & child 2 (10 switches) + verify markers preserved!
    let mut zalo_switch_samples = Vec::new();
    for i in 0..10 {
        let t0 = Instant::now();
        if i % 2 == 0 {
            let _ = zalo_2.hide();
            let _ = zalo_1.show();
            let _ = zalo_1.set_focus();
        } else {
            let _ = zalo_1.hide();
            let _ = zalo_2.show();
            let _ = zalo_2.set_focus();
        }
        zalo_switch_samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_millis(60));
    }
    let marker_1 = eval_json(&zalo_1, "JSON.stringify(window.__ZALO_SPIKE_MARKER__ || null)", Duration::from_secs(5))
        .unwrap_or(Value::Null);
    let marker_2 = eval_json(&zalo_2, "JSON.stringify(window.__ZALO_SPIKE_MARKER__ || null)", Duration::from_secs(5))
        .unwrap_or(Value::Null);

    // 3rd Zalo child: Raw Wry child (ZERO Tauri IPC/init scripts!)
    let raw_nav_log = Arc::new(Mutex::new(Vec::new()));
    let raw_nw_log = Arc::new(Mutex::new(Vec::new()));
    let _ = zalo_2.hide();
    let _ = zalo_1.hide();
    let t_start_3 = Instant::now();
    let raw_zalo_3_created = create_raw_wry_child(
        window,
        "zalo-3-raw-wry",
        "https://chat.zalo.me",
        data_root.join("zalo-3-raw-wry"),
        220.0,
        0.0,
        880.0,
        720.0,
        true,
        raw_nav_log.clone(),
        raw_nw_log.clone(),
        vec!["zalo.me".to_string(), "zaloapp.com".to_string()],
    );
    std::thread::sleep(Duration::from_secs(8));
    let zalo_3_load_ms = (t_start_3.elapsed().as_secs_f64() * 1000.0).round();
    let zalo_3_inspect = eval_raw_wry_json(window, "zalo-3-raw-wry", zalo_inspect_js(), Duration::from_secs(10))
        .unwrap_or_else(|e| json!({ "error": e }));
    let mem_3_children = measure_process_memory(data_root, self_pid);

    let state = app.state::<AppState>();
    json!({
        "mode": "zalo-live",
        "zalo_1_tauri_child": {
            "load_and_hydrate_ms": zalo_1_load_ms,
            "page_load_finished_urls": zalo_1_loads,
            "inspect": zalo_1_inspect,
            "marker_after_switches": marker_1
        },
        "zalo_2_tauri_child": {
            "load_and_hydrate_ms": zalo_2_load_ms,
            "inspect": zalo_2_inspect,
            "marker_after_switches": marker_2
        },
        "zalo_3_raw_wry_child": {
            "created": raw_zalo_3_created.is_ok(),
            "load_and_hydrate_ms": zalo_3_load_ms,
            "inspect": zalo_3_inspect,
            "nav_events": *raw_nav_log.lock().unwrap(),
            "new_window_events": *raw_nw_log.lock().unwrap()
        },
        "zalo_hide_show_switch_latency": stats_ms(&zalo_switch_samples),
        "events": {
            "nav_events": *state.nav_events.lock().unwrap(),
            "new_window_events": *state.new_window_events.lock().unwrap(),
            "permission_events": *state.permission_events.lock().unwrap(),
            "download_events": *state.download_events.lock().unwrap()
        },
        "memory_overhead": {
            "0_children_rail_only": mem_0_children,
            "1_zalo_child": mem_1_child,
            "2_zalo_children": mem_2_children,
            "3_zalo_children": mem_3_children
        }
    })
}

fn create_raw_wry_profile_child(
    window: &Window,
    label: &str,
    url: &str,
    shared_data_dir: PathBuf,
    profile_name: &str,
    visible: bool,
) -> Result<(), String> {
    #[cfg(windows)]
    use wry::WebViewBuilderExtWindows;

    std::fs::create_dir_all(&shared_data_dir).map_err(|e| e.to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let win_clone = window.clone();
    let label_owned = label.to_string();
    let url_owned = url.to_string();
    let prof_owned = profile_name.to_string();
    window
        .run_on_main_thread(move || {
            let mut web_context = wry::WebContext::new(Some(shared_data_dir));
            let builder = wry::WebViewBuilder::new_with_web_context(&mut web_context)
                .with_url(&url_owned)
                .with_visible(visible)
                .with_clipboard(true)
                .with_bounds(wry::Rect {
                    position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(220.0, 0.0)),
                    size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(880.0, 720.0)),
                });
            #[cfg(windows)]
            let builder = builder.with_profile_name(prof_owned);
            match builder.build_as_child(&win_clone) {
                Ok(wv) => {
                    RAW_WRY_CHILDREN.with(|cell| {
                        cell.borrow_mut().insert(label_owned, (wv, web_context));
                    });
                    let _ = tx.send(Ok(()));
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("build_as_child(profile) failed: {e}")));
                }
            }
        })
        .map_err(|e| format!("run_on_main_thread error: {e}"))?;
    rx.recv_timeout(Duration::from_secs(30))
        .map_err(|_| "timeout creating profile child".to_string())?
}

fn sync_switch_benchmark(window: &Window, a: &str, b: &str) -> Value {
    let mut hide_show_ms = Vec::new();
    let mut offscreen_ms = Vec::new();

    for i in 0..20 {
        let a_s = a.to_string();
        let b_s = b.to_string();
        let (tx, rx) = std::sync::mpsc::channel::<f64>();
        let _ = window.run_on_main_thread(move || {
            RAW_WRY_CHILDREN.with(|cell| {
                let map = cell.borrow();
                if let (Some((wa, _)), Some((wb, _))) = (map.get(&a_s), map.get(&b_s)) {
                    let t0 = Instant::now();
                    if i % 2 == 0 {
                        let _ = wa.set_visible(false);
                        let _ = wb.set_visible(true);
                        let _ = wb.focus();
                    } else {
                        let _ = wb.set_visible(false);
                        let _ = wa.set_visible(true);
                        let _ = wa.focus();
                    }
                    let _ = tx.send(t0.elapsed().as_secs_f64() * 1000.0);
                }
            });
        });
        if let Ok(ms) = rx.recv_timeout(Duration::from_secs(5)) {
            hide_show_ms.push(ms);
        }
        std::thread::sleep(Duration::from_millis(30));
    }

    // Put both visible for offscreen bounds benchmark
    let a_s = a.to_string();
    let b_s = b.to_string();
    let (tx0, rx0) = std::sync::mpsc::channel::<()>();
    let _ = window.run_on_main_thread(move || {
        RAW_WRY_CHILDREN.with(|cell| {
            let map = cell.borrow();
            if let (Some((wa, _)), Some((wb, _))) = (map.get(&a_s), map.get(&b_s)) {
                let _ = wa.set_visible(true);
                let _ = wb.set_visible(true);
            }
            let _ = tx0.send(());
        });
    });
    let _ = rx0.recv_timeout(Duration::from_secs(5));

    let active_rect = wry::Rect {
        position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(220.0, 0.0)),
        size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(880.0, 720.0)),
    };
    let hidden_rect = wry::Rect {
        position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(-10000.0, -10000.0)),
        size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(880.0, 720.0)),
    };

    for i in 0..20 {
        let a_s = a.to_string();
        let b_s = b.to_string();
        let (tx, rx) = std::sync::mpsc::channel::<f64>();
        let _ = window.run_on_main_thread(move || {
            RAW_WRY_CHILDREN.with(|cell| {
                let map = cell.borrow();
                if let (Some((wa, _)), Some((wb, _))) = (map.get(&a_s), map.get(&b_s)) {
                    let t0 = Instant::now();
                    if i % 2 == 0 {
                        let _ = wa.set_bounds(hidden_rect);
                        let _ = wb.set_bounds(active_rect);
                        let _ = wb.focus();
                    } else {
                        let _ = wb.set_bounds(hidden_rect);
                        let _ = wa.set_bounds(active_rect);
                        let _ = wa.focus();
                    }
                    let _ = tx.send(t0.elapsed().as_secs_f64() * 1000.0);
                }
            });
        });
        if let Ok(ms) = rx.recv_timeout(Duration::from_secs(5)) {
            offscreen_ms.push(ms);
        }
        std::thread::sleep(Duration::from_millis(30));
    }

    json!({
        "sync_com_hide_show_ms": stats_ms(&hide_show_ms),
        "sync_com_offscreen_bounds_ms": stats_ms(&offscreen_ms)
    })
}

fn run_deep_probe(
    _app: &tauri::AppHandle,
    window: &Window,
    data_root: &Path,
    port: u16,
) -> Value {
    let self_pid = std::process::id();
    let shared_dir = data_root.join("shared-webview2-env");

    // 1. Test shared-environment WebView2 with_profile_name isolation on fixture
    let bus = ReportBus::new();
    start_fixture_server(port, bus.clone());
    let url_pa = format!("http://127.0.0.1:{port}/fixture?acct=ProfA");
    let url_pb = format!("http://127.0.0.1:{port}/fixture?acct=ProfB");
    let p_a = create_raw_wry_profile_child(window, "prof-a", &url_pa, shared_dir.clone(), "acct_prof_a", true);
    let p_b = create_raw_wry_profile_child(window, "prof-b", &url_pb, shared_dir.clone(), "acct_prof_b", false);
    let _ = bus.wait_for(Duration::from_secs(20), |v| v["kind"] == "loaded" && v["acct"] == "ProfA");
    let _ = bus.wait_for(Duration::from_secs(20), |v| v["kind"] == "loaded" && v["acct"] == "ProfB");

    let _ = window.run_on_main_thread(|| {
        RAW_WRY_CHILDREN.with(|c| {
            if let Some((w, _)) = c.borrow().get("prof-a") {
                let _ = w.evaluate_script("window.spikeWrite('SHARED_ENV_PROF_A')");
            }
        });
    });
    let write_pa = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "write" && v["acct"] == "ProfA");
    let _ = window.run_on_main_thread(|| {
        RAW_WRY_CHILDREN.with(|c| {
            if let Some((w, _)) = c.borrow().get("prof-b") {
                let _ = w.evaluate_script("window.spikeRead('shared_env_isolation')");
            }
        });
    });
    let read_pb = bus.wait_for(Duration::from_secs(10), |v| v["kind"] == "read" && v["acct"] == "ProfB");

    // 2. Measure true synchronous main-thread COM switch latency (hide/show vs offscreen bounds)
    let sync_switch = sync_switch_benchmark(window, "prof-a", "prof-b");

    // Clear fixture children before Zalo shared-env RAM & QR DOM test
    let (txc, rxc) = std::sync::mpsc::channel::<()>();
    let _ = window.run_on_main_thread(move || {
        RAW_WRY_CHILDREN.with(|c| c.borrow_mut().clear());
        let _ = txc.send(());
    });
    let _ = rxc.recv_timeout(Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(800));

    // 3. Shared-env WebView2 profiles loading live https://chat.zalo.me (1, 2, 3 profiles) + deep QR DOM inspection
    let shared_zalo_dir = data_root.join("shared-zalo-env");
    let r1 = create_raw_wry_profile_child(window, "sz-1", "https://chat.zalo.me", shared_zalo_dir.clone(), "zalo_p1", true);
    std::thread::sleep(Duration::from_secs(8));
    let qr_dom_probe = eval_raw_wry_json(
        window,
        "sz-1",
        r#"JSON.stringify((function(){
            const imgs = Array.from(document.querySelectorAll('img')).map(i => {
                const r = i.getBoundingClientRect();
                return {
                    alt: (i.alt || '').slice(0, 60),
                    complete: i.complete,
                    naturalWidth: i.naturalWidth,
                    naturalHeight: i.naturalHeight,
                    rectW: Math.round(r.width),
                    rectH: Math.round(r.height),
                    srcPrefix: (i.src || '').slice(0, 80),
                    srcLen: (i.src || '').length,
                    cls: (i.className || '').slice(0, 60)
                };
            });
            const svgs = Array.from(document.querySelectorAll('svg')).map(s => {
                const r = s.getBoundingClientRect();
                return { rectW: Math.round(r.width), rectH: Math.round(r.height), cls: String(s.className?.baseVal || '').slice(0, 60), children: s.childElementCount };
            });
            const qrNodes = Array.from(document.querySelectorAll('[class*="qr" i], [id*="qr" i]')).slice(0, 10).map(n => {
                const r = n.getBoundingClientRect();
                return { tag: n.tagName, cls: String(n.className || '').slice(0, 80), rectW: Math.round(r.width), rectH: Math.round(r.height), bgImg: getComputedStyle(n).backgroundImage.slice(0, 60) };
            });
            const iframes = Array.from(document.querySelectorAll('iframe')).map(f => ({ src: (f.src || '').slice(0, 80) }));
            const failedResources = performance.getEntriesByType('resource')
                .filter(e => e.responseStatus >= 400)
                .map(e => ({ name: e.name.slice(0, 80), status: e.responseStatus }));
            return {
                href: location.href,
                title: document.title,
                imgs,
                svgs,
                qrNodes,
                iframes,
                failedResources
            };
        })())"#,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|e| json!({ "error": e }));
    let mem_shared_1 = measure_process_memory(data_root, self_pid);

    let r2 = create_raw_wry_profile_child(window, "sz-2", "https://chat.zalo.me", shared_zalo_dir.clone(), "zalo_p2", false);
    std::thread::sleep(Duration::from_secs(7));
    let mem_shared_2 = measure_process_memory(data_root, self_pid);

    let r3 = create_raw_wry_profile_child(window, "sz-3", "https://chat.zalo.me", shared_zalo_dir.clone(), "zalo_p3", false);
    std::thread::sleep(Duration::from_secs(7));
    let mem_shared_3 = measure_process_memory(data_root, self_pid);

    json!({
        "mode": "deep-probe",
        "shared_env_profile_isolation": {
            "created_a": p_a.is_ok(),
            "created_b": p_b.is_ok(),
            "write_a": write_pa,
            "read_b_before_write": read_pb
        },
        "sync_com_switch_latency": sync_switch,
        "zalo_qr_dom_probe": qr_dom_probe,
        "shared_env_zalo_memory": {
            "created": [r1.is_ok(), r2.is_ok(), r3.is_ok()],
            "1_shared_profile": mem_shared_1,
            "2_shared_profiles": mem_shared_2,
            "3_shared_profiles": mem_shared_3
        }
    })
}
