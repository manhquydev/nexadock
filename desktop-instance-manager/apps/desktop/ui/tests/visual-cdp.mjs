import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const uiDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(uiDir, '..', '..', '..');
const screenshotsDir = path.join(repoRoot, 'docs', 'design', 'screenshots');
fs.mkdirSync(screenshotsDir, { recursive: true });

const EDGE_PATH = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const CHROME_PATH = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const browserBin = fs.existsSync(EDGE_PATH) ? EDGE_PATH : (fs.existsSync(CHROME_PATH) ? CHROME_PATH : null);

if (!browserBin) {
  console.error('BLOCKED: No Edge or Chrome binary found for headless browser verification.');
  process.exit(1);
}

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8'
};

function createMockState() {
  return {
    delayNextLaunchMs: 0,
    failNextAction: null,
    disconnectStatus: false,
    status: {
      app_version: '0.1.0',
      platform: 'windows',
      data_dir: 'C:\\Users\\Demo\\AppData\\Local\\MultiZaloDesktop',
      installation: { valid: true, mode: 'standard', version: '24.10.1', authenticode_status: 'Valid' },
      primary_zalo: { is_running: true, locking_pids: [4120], process_count: 1, total_working_set_bytes: 268435456 },
      profiles: [
        {
          id: 'prof-1',
          name: 'Đức - Công việc (Hồ sơ tên dài kiểm tra xuống dòng)',
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
        { platform: 'macOS', status: 'PartiallySupported', strategy: 'HOME isolation', limits: 'Untested on Tahoe' },
        { platform: 'Ubuntu LTS', status: 'NOT_TESTED', strategy: 'XDG sandbox', limits: 'Requires verification' }
      ],
      sanitized_logs: ['[INFO] Host bridge started on 127.0.0.1']
    },
    isolation: {
      ok: true,
      all_passed: false,
      checks: [
        { check_name: 'path_boundary', passed: true, detail: 'Inside sandbox root' },
        { check_name: 'ntfs_acl', status: 'NOT_TESTED', detail: 'Skipped in mock mode' },
        { check_name: 'call_pipe', status: 'BLOCKED', detail: 'Global PipeZCallRecv' }
      ]
    }
  };
}

let mockData = createMockState();

function readJsonBody(req) {
  return new Promise((resolve) => {
    let raw = '';
    req.on('data', (chunk) => { raw += chunk; });
    req.on('end', () => {
      try {
        resolve(raw ? JSON.parse(raw) : {});
      } catch {
        resolve({});
      }
    });
  });
}

const server = http.createServer(async (req, res) => {
  const urlPath = (req.url || '/').split('?')[0];
  const json = (code, payload) => {
    res.writeHead(code, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(payload));
  };

  if (urlPath === '/api/status') {
    if (mockData.disconnectStatus) {
      res.destroy();
      return;
    }
    return json(200, mockData.status);
  }
  if (urlPath === '/api/backups') return json(200, mockData.backups);
  if (urlPath === '/api/diagnostics') return json(200, mockData.diagnostics);
  if (urlPath === '/api/verify-isolation') return json(200, mockData.isolation);

  if (urlPath === '/api/profiles' && req.method === 'POST') {
    const body = await readJsonBody(req);
    const created = {
      id: `prof-${mockData.status.profiles.length + 1}`,
      name: body.name || 'New Profile',
      directory_name: `profile-${mockData.status.profiles.length + 1}`,
      created_at: new Date().toISOString(),
      last_launched_at: null,
      zalo_user_id: null,
      notes: body.notes || '',
      running: false,
      main_pid: null,
      process_count: 0,
      window_title: null,
      total_working_set_bytes: 0,
      data_size_bytes: null,
      stale_lockfile_detected: false
    };
    mockData.status.profiles.push(created);
    return json(200, { ok: true, message: 'Profile created', data: created });
  }

  if (urlPath === '/api/backups/restore' && req.method === 'POST') {
    const body = await readJsonBody(req);
    return json(200, { ok: true, message: `Restored ${body.name || 'auto'}` });
  }

  const profMatch = urlPath.match(/^\/api\/profiles\/([^/]+)(?:\/(launch|focus|stop|backup|rename))?$/);
  if (profMatch) {
    const id = decodeURIComponent(profMatch[1]);
    const action = profMatch[2];
    const prof = mockData.status.profiles.find((p) => p.id === id);

    if (mockData.failNextAction) {
      const err = mockData.failNextAction;
      mockData.failNextAction = null;
      return json(400, err);
    }

    if (action === 'launch') {
      if (mockData.delayNextLaunchMs > 0) {
        const delay = mockData.delayNextLaunchMs;
        mockData.delayNextLaunchMs = 0;
        await new Promise((r) => setTimeout(r, delay));
      }
      if (prof) {
        prof.running = true;
        prof.main_pid = 9901;
        prof.process_count = 2;
      }
      return json(200, { ok: true, message: 'Launched' });
    }
    if (action === 'stop') {
      if (prof) {
        prof.running = false;
        prof.main_pid = null;
        prof.process_count = 0;
      }
      return json(200, { ok: true, message: 'Stopped' });
    }
    if (action === 'focus' || action === 'backup') {
      return json(200, { ok: true, message: `${action} ok` });
    }
    if (action === 'rename') {
      const body = await readJsonBody(req);
      if (prof && body.newName) prof.name = body.newName;
      return json(200, { ok: true, message: 'Renamed' });
    }
    if (!action && req.method === 'DELETE') {
      mockData.status.profiles = mockData.status.profiles.filter((p) => p.id !== id);
      return json(200, { ok: true, message: 'Deleted' });
    }
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

  const evalJs = async (expression) => {
    const res = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (res.exceptionDetails) throw new Error(res.exceptionDetails.text);
    return res.result.value;
  };

  const setViewport = async (width, height, scale = 1) => {
    await send('Emulation.setDeviceMetricsOverride', {
      width,
      height,
      deviceScaleFactor: scale,
      mobile: width <= 480
    });
    await new Promise((r) => setTimeout(r, 80));
  };

  const snap = async (filename) => {
    const { data } = await send('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync(path.join(screenshotsDir, filename), Buffer.from(data, 'base64'));
  };

  const pressKey = async (key, code, windowsVirtualKeyCode) => {
    await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key, code, windowsVirtualKeyCode });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode });
    await new Promise((r) => setTimeout(r, 40));
  };

  await new Promise((r) => setTimeout(r, 600));

  // 1. Matrix verification + screenshots across viewports x locales x themes
  const viewports = [
    [1160, 780],
    [900, 650],
    [768, 650],
    [420, 540]
  ];
  for (const locale of ['vi-VN', 'en-US']) {
    await evalJs(`(() => { const r = document.querySelector('input[name="ui-locale"][value="${locale}"]'); r.checked = true; r.dispatchEvent(new Event('change', { bubbles: true })); })()`);
    for (const theme of ['light', 'dark']) {
      await evalJs(`(() => { const r = document.querySelector('input[name="ui-theme"][value="${theme}"]'); r.checked = true; r.dispatchEvent(new Event('change', { bubbles: true })); })()`);
      for (const [width, height] of viewports) {
        await setViewport(width, height);
        const metrics = await evalJs(`(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          innerWidth: window.innerWidth,
          lang: document.documentElement.lang,
          themeMeta: document.querySelector('meta[name="color-scheme"]').content,
          profileCount: document.querySelectorAll('#profiles-list .profile-row').length
        }))()`);
        assert.ok(metrics.scrollWidth <= metrics.innerWidth, `Overflow at ${width}x${height} (${locale}/${theme})`);
        assert.equal(metrics.profileCount, 2);
        assert.equal(metrics.themeMeta, theme);
        assert.equal(metrics.lang, locale === 'vi-VN' ? 'vi' : 'en');
        if (width === 1160 || (locale === 'vi-VN' && theme === 'light')) {
          await snap(`profiles-${locale}-${theme}-${width}x${height}.png`);
        }
      }
    }
  }

  // Reset to vi-VN / light / 1160x780 for interaction & accessibility tests
  await evalJs(`(() => {
    const l = document.querySelector('input[name="ui-locale"][value="vi-VN"]'); l.checked = true; l.dispatchEvent(new Event('change', { bubbles: true }));
    const t = document.querySelector('input[name="ui-theme"][value="light"]'); t.checked = true; t.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await setViewport(1160, 780);

  // 2. WAI-ARIA Menu Button keyboard navigation test
  await evalJs(`document.querySelector('#profiles-list .profile-row:first-child button[aria-haspopup="menu"]').focus()`);
  await pressKey('Enter', 'Enter', 13);
  const menuOpened = await evalJs(`(() => {
    const btn = document.querySelector('#profiles-list .profile-row:first-child button[aria-haspopup="menu"]');
    const activeRole = document.activeElement?.getAttribute('role');
    return { expanded: btn.getAttribute('aria-expanded'), activeRole };
  })()`);
  assert.equal(menuOpened.expanded, 'true');
  assert.equal(menuOpened.activeRole, 'menuitem');
  await snap('overflow-menu-vi-light.png');
  await pressKey('ArrowDown', 'ArrowDown', 40);
  await pressKey('Escape', 'Escape', 27);
  const menuClosed = await evalJs(`(() => {
    const btn = document.querySelector('#profiles-list .profile-row:first-child button[aria-haspopup="menu"]');
    return { expanded: btn.getAttribute('aria-expanded'), focusedBack: document.activeElement === btn };
  })()`);
  assert.equal(menuClosed.expanded, 'false');
  assert.equal(menuClosed.focusedBack, true);

  // 3. Native Radio Group keyboard navigation in Settings
  await evalJs(`document.querySelector('.nav-item[data-view="settings"]').click()`);
  await snap('settings-vi-light-1160x780.png');
  await evalJs(`document.querySelector('input[name="ui-locale"][value="vi-VN"]').focus()`);
  await pressKey('ArrowRight', 'ArrowRight', 39);
  const langAfterArrow = await evalJs(`document.documentElement.lang`);
  assert.equal(langAfterArrow, 'en');
  await pressKey('ArrowLeft', 'ArrowLeft', 37);
  assert.equal(await evalJs(`document.documentElement.lang`), 'vi');

  // 4. Backups & Diagnostics views + screenshots
  await evalJs(`document.querySelector('.nav-item[data-view="backups"]').click()`);
  await snap('backups-vi-light-1160x780.png');
  await evalJs(`document.querySelector('.nav-item[data-view="diagnostics"]').click()`);
  await snap('diagnostics-vi-light-1160x780.png');
  const diagText = await evalJs(`document.getElementById('diagnostics-tbody').textContent`);
  assert.match(diagText, /NOT_TESTED/);
  assert.match(diagText, /BLOCKED/);

  // 5. Async per-profile pending isolation + Dialogs + Error Banner i18n
  await evalJs(`document.querySelector('.nav-item[data-view="profiles"]').click()`);
  mockData.delayNextLaunchMs = 250;
  await evalJs(`document.querySelector('#profiles-list .profile-row:nth-child(2) .btn-primary').click()`);
  await new Promise((r) => setTimeout(r, 60));
  const pendingCheck = await evalJs(`(() => ({
    prof2Disabled: document.querySelector('#profiles-list .profile-row:nth-child(2) .btn-primary').disabled,
    prof1Enabled: !document.querySelector('#profiles-list .profile-row:nth-child(1) .btn-primary').disabled
  }))()`);
  assert.equal(pendingCheck.prof2Disabled, true);
  assert.equal(pendingCheck.prof1Enabled, true);
  await new Promise((r) => setTimeout(r, 300));

  // Error banner: raw English message must NOT appear in Vietnamese banner text, only in expandable details
  mockData.failNextAction = { ok: false, error_code: 'PROTECTED_PRIMARY_PROCESS', message: 'Target PID 4120 is primary Zalo' };
  await evalJs(`document.querySelector('#profiles-list .profile-row:first-child .btn-danger').click()`);
  await new Promise((r) => setTimeout(r, 150));
  const errCheck = await evalJs(`(() => ({
    bannerText: document.getElementById('status-banner-text').textContent,
    rawText: document.getElementById('status-banner-raw').textContent,
    detailsHidden: document.getElementById('status-banner-details').hidden
  }))()`);
  assert.doesNotMatch(errCheck.bannerText, /Target PID 4120/);
  assert.match(errCheck.rawText, /PROTECTED_PRIMARY_PROCESS/);
  assert.equal(errCheck.detailsHidden, false);
  await snap('state-error-banner-vi-light.png');

  // Dialogs: Create, Details, Delete (verify autofocus on Cancel)
  await evalJs(`document.querySelector('button[commandfor="create-profile-dialog"]').click()`);
  await snap('dialog-create-vi-light.png');
  await evalJs(`document.getElementById('create-profile-dialog').close('cancel')`);

  await evalJs(`document.querySelector('#profiles-list .profile-row:first-child [role="menuitem"]:nth-child(3)').click()`);
  await snap('dialog-details-vi-light.png');
  await evalJs(`document.getElementById('profile-details-dialog').close('close')`);

  await evalJs(`document.querySelector('#profiles-list .profile-row:nth-child(2) [role="menuitem"]:nth-child(4)').click()`);
  await snap('dialog-delete-vi-light.png');
  await evalJs(`document.getElementById('delete-profile-dialog').close('cancel')`);

  // Zoom 200% simulation (580x390 CSS px)
  await setViewport(580, 390, 2);
  const zoomMetrics = await evalJs(`({ scrollWidth: document.documentElement.scrollWidth, innerWidth: window.innerWidth })`);
  assert.ok(zoomMetrics.scrollWidth <= zoomMetrics.innerWidth, 'Overflow at 200% zoom');
  await snap('zoom-200-vi-light.png');

  assert.deepEqual(runtimeErrors, [], `Runtime errors: ${runtimeErrors.join('; ')}`);
  console.log('PASS (16 matrix combinations, WAI-ARIA menu, native radios, async pending, error banner, dialogs, 200% zoom, 0 runtime errors)');
  ws.close();
} finally {
  proc.kill();
  server.close();
  try {
    fs.rmSync(userDataDir, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
  } catch {
    // Ignore Windows temp lock
  }
}
