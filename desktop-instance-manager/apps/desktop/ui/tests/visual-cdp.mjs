import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const uiDir = path.resolve(__dirname, '..');

const EDGE_PATH = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const CHROME_PATH = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const browserBin = fs.existsSync(EDGE_PATH) ? EDGE_PATH : (fs.existsSync(CHROME_PATH) ? CHROME_PATH : null);

if (!browserBin) {
  console.log('SKIPPED/NOT_TESTED: No Edge or Chrome binary found.');
  process.exit(0);
}

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8'
};

const mockData = {
  status: {
    app_version: '0.1.0',
    platform: 'windows',
    data_dir: 'C:\\Users\\Demo\\AppData\\Local\\MultiZaloDesktop',
    installation: { valid: true, mode: 'standard', version: '24.10.1', authenticode_status: 'Valid' },
    primary_zalo: { is_running: true, locking_pids: [4120], process_count: 1, total_working_set_bytes: 268435456 },
    profiles: [
      {
        id: 'prof-1',
        name: 'Đức - Công việc',
        directory_name: 'profile-duc-work',
        created_at: '2026-01-10T08:00:00Z',
        last_launched_at: '2026-02-01T09:15:00Z',
        zalo_user_id: '84901234567',
        notes: 'Tài khoản công ty',
        running: true,
        main_pid: 8820,
        process_count: 4,
        window_title: 'Zalo',
        total_working_set_bytes: 314572800,
        data_size_bytes: 524288000,
        stale_lockfile_detected: false
      },
      {
        id: 'prof-2',
        name: 'Cá nhân',
        directory_name: 'profile-personal',
        created_at: '2026-01-12T10:00:00Z',
        last_launched_at: null,
        zalo_user_id: null,
        notes: '',
        running: false,
        main_pid: null,
        process_count: 0,
        window_title: null,
        total_working_set_bytes: 0,
        data_size_bytes: null,
        stale_lockfile_detected: false
      }
    ]
  },
  backups: {
    backups: [
      {
        backup_id: 'bk-1',
        backup_name: 'duc-work-20260201.zip',
        backup_dir: 'C:\\Backups\\bk-1',
        source_profile_id: 'prof-1',
        source_profile_name: 'Đức - Công việc',
        created_at: '2026-02-01T12:00:00Z',
        file_count: 142,
        total_bytes: 104857600
      }
    ]
  },
  diagnostics: {
    is_elevated: false,
    smart_app_control_state: 1,
    disk_free_bytes: 53687091200,
    disk_free_ok: true,
    folder_redirection: { redirected: false, warning: null },
    platform_support_matrix: [
      { platform: 'Windows 10/11', status: 'SUPPORTED_AND_TESTED', strategy: 'USERPROFILE/APPDATA sandbox', limits: 'PipeZCallRecv global' },
      { platform: 'macOS', status: 'PARTIALLY_SUPPORTED', strategy: 'HOME isolation', limits: 'Untested on Tahoe' },
      { platform: 'Ubuntu LTS', status: 'NOT_TESTED', strategy: 'XDG sandbox', limits: 'Requires verification' }
    ],
    sanitized_logs: ['[INFO] Host bridge started on 127.0.0.1']
  },
  isolation: {
    ok: true,
    all_passed: false,
    checks: [
      { check_name: 'path_boundary', passed: true, detail: 'Inside sandbox root' },
      { check_name: 'ntfs_acl', status: 'NOT_TESTED', detail: 'Skipped in mock mode' }
    ]
  }
};

const server = http.createServer((req, res) => {
  const urlPath = (req.url || '/').split('?')[0];
  if (urlPath === '/api/status') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(mockData.status));
    return;
  }
  if (urlPath === '/api/backups') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(mockData.backups));
    return;
  }
  if (urlPath === '/api/diagnostics') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(mockData.diagnostics));
    return;
  }
  if (urlPath === '/api/verify-isolation') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(mockData.isolation));
    return;
  }

  const rel = urlPath === '/' ? 'index.html' : urlPath.replace(/^\/+/, '');
  const fullPath = path.join(uiDir, rel);
  if (!fullPath.startsWith(uiDir) || !fs.existsSync(fullPath)) {
    res.writeHead(404);
    res.end('Not found');
    return;
  }
  const ext = path.extname(fullPath);
  res.writeHead(200, {
    'Content-Type': MIME[ext] || 'application/octet-stream',
    'Content-Security-Policy': "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:;"
  });
  res.end(fs.readFileSync(fullPath));
});

await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const port = server.address().port;
const baseUrl = `http://127.0.0.1:${port}/`;

const cdpPort = 9222 + Math.floor(Math.random() * 500);
const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'mz-edge-'));
const proc = spawn(browserBin, [
  '--headless=new',
  '--disable-gpu',
  `--remote-debugging-port=${cdpPort}`,
  `--user-data-dir=${userDataDir}`,
  baseUrl
], { stdio: 'ignore' });

async function getWsUrl() {
  for (let i = 0; i < 30; i += 1) {
    try {
      const resp = await fetch(`http://127.0.0.1:${cdpPort}/json/list`);
      const list = await resp.json();
      const page = list.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // Wait for browser startup
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error('Timed out waiting for CDP WebSocket');
}

try {
  const wsUrl = await getWsUrl();
  const ws = new WebSocket(wsUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });

  let nextId = 1;
  const pending = new Map();
  const runtimeErrors = [];

  ws.addEventListener('message', (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(msg.error.message));
      else resolve(msg.result);
    } else if (msg.method === 'Runtime.exceptionThrown') {
      runtimeErrors.push(msg.params?.exceptionDetails?.text || 'Runtime exception');
    }
  });

  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });

  await send('Page.enable');
  await send('Runtime.enable');
  await send('Emulation.setDefaultBackgroundColorOverride', { color: { r: 0, g: 0, b: 0, a: 0 } });

  const evalJs = async (expression) => {
    const res = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (res.exceptionDetails) throw new Error(res.exceptionDetails.text);
    return res.result.value;
  };

  // Wait for profiles list to render
  await new Promise((r) => setTimeout(r, 600));

  const viewports = [
    [1160, 780],
    [900, 650],
    [768, 650],
    [420, 540]
  ];
  const locales = ['vi-VN', 'en-US'];
  const themes = ['light', 'dark'];

  const results = [];
  for (const locale of locales) {
    await evalJs(`document.querySelector('[data-locale="${locale}"]').click()`);
    for (const theme of themes) {
      await evalJs(`document.querySelector('[data-theme-choice="${theme}"]').click()`);
      for (const [width, height] of viewports) {
        await send('Emulation.setDeviceMetricsOverride', {
          width,
          height,
          deviceScaleFactor: 1,
          mobile: width <= 480
        });
        await new Promise((r) => setTimeout(r, 60));
        const metrics = await evalJs(`(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          innerWidth: window.innerWidth,
          lang: document.documentElement.lang,
          themeMeta: document.querySelector('meta[name="color-scheme"]').content,
          profileCount: document.querySelectorAll('#profiles-list .profile-row').length,
          statusText: document.getElementById('profiles-status-bar').textContent
        }))()`);
        assert.ok(metrics.scrollWidth <= metrics.innerWidth, `Horizontal overflow at ${width}x${height} (${locale}/${theme}): ${metrics.scrollWidth} > ${metrics.innerWidth}`);
        assert.equal(metrics.profileCount, 2);
        assert.equal(metrics.themeMeta, theme);
        assert.equal(metrics.lang, locale === 'vi-VN' ? 'vi' : 'en');
        results.push(`${locale}/${theme}@${width}x${height}: ok`);
      }
    }
  }

  assert.deepEqual(runtimeErrors, [], `Runtime errors: ${runtimeErrors.join('; ')}`);
  console.log(`PASS (${results.length} matrix combinations verified, 0 overflow, 0 runtime errors)`);
  ws.close();
} finally {
  proc.kill();
  server.close();
  try {
    fs.rmSync(userDataDir, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
  } catch {
    // Ignore Windows temp lock after browser exit
  }
}
