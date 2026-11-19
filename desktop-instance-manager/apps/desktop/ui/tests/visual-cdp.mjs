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

const SESSION_TOKEN = 'test-token-123';

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8'
};

function createMockState() {
  return {
    nextProfileSeq: 101,
    delayNextLaunchMs: 0,
    delayNextCreateMs: 0,
    delayNextStatusMs: 0,
    failNextAction: null,
    failNextCreate: null,
    statusOkFalse: null,
    disconnectStatus: false,
    statusDelayQueue: [],
    status: {
      app_version: '0.1.0',
      platform: 'windows',
      data_dir: 'C:\\Users\\Demo\\AppData\\Local\\MultiZaloDesktop',
      installation: { valid: true, mode: 'standard', version: '24.10.1', authenticode_status: 'Valid' },
      primary_zalo: { is_running: true, locking_pids: [4120], process_count: 1, total_working_set_bytes: 268435456 },
      profiles: [
        {
          id: 'prof-1',
          name: 'Đức — Công việc & Đối tác miền Nam',
          directory_name: 'profile-duc-work',
          created_at: '2026-01-10T08:00:00Z',
          last_launched_at: '2026-02-01T09:15:00Z',
          zalo_user_id: '100204819',
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
          source_profile_name: 'Đức — Công việc & Đối tác miền Nam',
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

function buildSyntheticProfiles(count) {
  const catalog = [
    ['Đức — Công việc & Đối tác miền Nam', 'Tài khoản công ty', true],
    ['Cá nhân', '', false],
    ['Bán hàng Online', 'Đơn hàng và tư vấn', true],
    ['Hỗ trợ khách hàng', 'Ca sáng', true],
    ['Kế toán nội bộ', 'Hóa đơn và chứng từ', false],
    ['Nhân sự & Tuyển dụng', '', false],
    ['Đối tác miền Bắc', 'Khu vực Hà Nội', true],
    ['Kho vận TP.HCM', 'Điều phối giao nhận', false],
    ['Marketing & Sự kiện', 'Chiến dịch quý 2', true],
    ['Dự án Sản phẩm mới', '', false],
    ['Chăm sóc khách hàng thân thiết', 'Nhóm ưu tiên', false],
    ['Quản trị Cộng đồng', '', true],
    ['Chi nhánh Đà Nẵng', 'Văn phòng miền Trung', false],
    ['Đối tác Vận chuyển', '', false],
    ['Đào tạo nội bộ', 'Lớp hội nhập', false],
    ['Mua hàng & Nhà cung cấp', '', false],
    ['Kênh Đại lý', 'Hệ thống phân phối', true],
    ['Tài chính — Thu chi', '', false],
    ['Pháp chế & Hợp đồng', '', false],
    ['Ban điều hành', 'Họp giao ban tuần', false]
  ];
  const out = [];
  for (let i = 0; i < count; i += 1) {
    const [name, notes, running] = catalog[i % catalog.length];
    const id = `prof-${i + 1}`;
    out.push({
      id,
      name: i < catalog.length ? name : `${name} (${i + 1})`,
      directory_name: `profile-${i + 1}`,
      created_at: '2026-01-10T08:00:00Z',
      last_launched_at: running ? '2026-02-01T09:15:00Z' : null,
      zalo_user_id: i === 0 ? '100204819' : null,
      notes,
      running,
      main_pid: running ? 8820 + i : null,
      process_count: running ? 2 : 0,
      window_title: running ? 'Zalo' : null,
      total_working_set_bytes: running ? 180 * 1024 * 1024 : 0,
      data_size_bytes: 120 * 1024 * 1024,
      stale_lockfile_detected: false
    });
  }
  return out;
}

let mockData = createMockState();
const recordedRequests = [];

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
  const token = req.headers['x-dim-session-token'] || null;
  const json = (code, payload) => {
    if (res.writableEnded || res.destroyed) return;
    res.writeHead(code, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(payload));
  };

  if (mockData.disconnectStatus && urlPath.startsWith('/api/')) {
    recordedRequests.push({ method: req.method, urlPath, body: null, token });
    res.destroy();
    return;
  }

  if (urlPath === '/api/status') {
    recordedRequests.push({ method: req.method, urlPath, body: null, token });
    if (mockData.delayNextStatusMs > 0) {
      const delay = mockData.delayNextStatusMs;
      mockData.delayNextStatusMs = 0;
      await new Promise((r) => setTimeout(r, delay));
    }
    if (mockData.statusOkFalse) {
      const err = mockData.statusOkFalse;
      mockData.statusOkFalse = null;
      return json(typeof err === 'object' && err.httpStatus ? err.httpStatus : 200, typeof err === 'object' ? err : { ok: false, error_code: 'HOST_EXEC_FAILED', message: 'Status error' });
    }
    if (Array.isArray(mockData.statusDelayQueue) && mockData.statusDelayQueue.length > 0) {
      const next = mockData.statusDelayQueue.shift();
      await new Promise((r) => setTimeout(r, next.delayMs));
      return json(200, next.payload);
    }
    return json(200, mockData.status);
  }

  if (urlPath === '/api/backups') {
    recordedRequests.push({ method: req.method, urlPath, body: null, token });
    return json(200, mockData.backups);
  }
  if (urlPath === '/api/diagnostics') {
    recordedRequests.push({ method: req.method, urlPath, body: null, token });
    return json(200, mockData.diagnostics);
  }
  if (urlPath === '/api/verify-isolation') {
    recordedRequests.push({ method: req.method, urlPath, body: null, token });
    return json(200, mockData.isolation);
  }

  if (urlPath === '/api/profiles' && req.method === 'POST') {
    const body = await readJsonBody(req);
    recordedRequests.push({ method: req.method, urlPath, body, token });
    if (mockData.delayNextCreateMs > 0) {
      const delay = mockData.delayNextCreateMs;
      mockData.delayNextCreateMs = 0;
      await new Promise((r) => setTimeout(r, delay));
    }
    if (mockData.failNextCreate) {
      const err = mockData.failNextCreate;
      mockData.failNextCreate = null;
      return json(400, err);
    }
    const seq = mockData.nextProfileSeq++;
    const created = {
      id: `prof-${seq}`,
      name: body.name || 'Tài khoản mới',
      directory_name: `profile-${seq}`,
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
    recordedRequests.push({ method: req.method, urlPath, body, token });
    const seq = mockData.nextProfileSeq++;
    const restored = {
      id: `prof-${seq}`,
      name: body.name || `Bản khôi phục ${seq}`,
      directory_name: `profile-${seq}`,
      created_at: new Date().toISOString(),
      last_launched_at: null,
      zalo_user_id: null,
      notes: '',
      running: false,
      main_pid: null,
      process_count: 0,
      window_title: null,
      total_working_set_bytes: 0,
      data_size_bytes: 104857600,
      stale_lockfile_detected: false
    };
    mockData.status.profiles.push(restored);
    return json(200, { ok: true, message: `Restored ${restored.name}`, data: restored });
  }

  const profMatch = urlPath.match(/^\/api\/profiles\/([^/]+)(?:\/(launch|focus|stop|backup|rename))?$/);
  if (profMatch) {
    const id = decodeURIComponent(profMatch[1]);
    const action = profMatch[2];
    const prof = mockData.status.profiles.find((p) => p.id === id);
    const body = req.method === 'GET' ? {} : await readJsonBody(req);
    recordedRequests.push({ method: req.method, urlPath, body, token });

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
  if (rel === 'index.html') {
    const html = fs.readFileSync(fullPath, 'utf8').replace(
      '</head>',
      `  <meta name="dim-session-token" content="${SESSION_TOKEN}">\n</head>`
    );
    res.end(html);
    return;
  }
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
  const eventWaiters = new Map();
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
    if (msg.method && eventWaiters.has(msg.method)) {
      const list = eventWaiters.get(msg.method);
      eventWaiters.delete(msg.method);
      for (const cb of list) cb(msg.params);
    }
  });

  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });

  const waitForEvent = (method, timeoutMs = 5000) => new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      eventWaiters.delete(method);
      reject(new Error(`Timed out waiting for CDP event ${method}`));
    }, timeoutMs);
    const list = eventWaiters.get(method) || [];
    list.push((params) => {
      clearTimeout(timer);
      resolve(params);
    });
    eventWaiters.set(method, list);
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
    const outPath = path.join(screenshotsDir, filename);
    const buf = Buffer.from(data, 'base64');
    for (let attempt = 0; attempt < 5; attempt++) {
      try {
        fs.writeFileSync(outPath, buf);
        return;
      } catch (err) {
        if (attempt === 4) throw err;
        await new Promise((r) => setTimeout(r, 60 * (attempt + 1)));
      }
    }
  };

  const pressKey = async (key, code, windowsVirtualKeyCode) => {
    await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key, code, windowsVirtualKeyCode });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode });
    await new Promise((r) => setTimeout(r, 40));
  };

  await new Promise((r) => setTimeout(r, 600));

  // (a) Verify X-DIM-Session-Token sent on all initial bridge API requests
  const metaToken = await evalJs(`document.querySelector('meta[name="dim-session-token"]')?.content || ''`);
  assert.equal(metaToken, SESSION_TOKEN);
  assert.ok(recordedRequests.length >= 4, 'Expected initial API calls on load');
  for (const r of recordedRequests) {
    assert.equal(r.token, SESSION_TOKEN, `Missing X-DIM-Session-Token on ${r.method} ${r.urlPath}`);
  }

  // 1. Matrix verification + screenshots across 16 viewport x locale x theme combinations
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
          dataTheme: document.documentElement.getAttribute('data-theme'),
          themeMeta: document.querySelector('meta[name="color-scheme"]').content,
          accountCount: document.querySelectorAll('#profiles-list .account-card').length,
          primaryVisible: !document.getElementById('primary-zalo-section').hidden
        }))()`);
        assert.ok(metrics.scrollWidth <= metrics.innerWidth, `Overflow at ${width}x${height} (${locale}/${theme})`);
        assert.equal(metrics.accountCount, 2);
        assert.equal(metrics.primaryVisible, true);
        assert.equal(metrics.dataTheme, theme);
        assert.equal(metrics.themeMeta, theme);
        assert.equal(metrics.lang, locale === 'vi-VN' ? 'vi' : 'en');
        if (width === 1160 || (locale === 'vi-VN' && theme === 'light')) {
          await snap(`account-dock-${locale}-${theme}-${width}x${height}.png`);
        }
      }
    }
  }

  // (b) Theme reload persistence: select dark, reload, verify immediately after reload, then reset to vi-VN / light
  await setViewport(1160, 780);
  await evalJs(`(() => {
    const r = document.querySelector('input[name="ui-theme"][value="dark"]');
    r.checked = true;
    r.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  assert.equal(await evalJs(`localStorage.getItem('multi-zalo.theme')`), 'dark');

  recordedRequests.length = 0;
  const loadDone = waitForEvent('Page.loadEventFired');
  await send('Page.reload');
  await loadDone;

  const immediateTheme = await evalJs(`({
    dataTheme: document.documentElement.getAttribute('data-theme'),
    themeMeta: document.querySelector('meta[name="color-scheme"]')?.getAttribute('content')
  })`);
  assert.equal(immediateTheme.dataTheme, 'dark', 'data-theme must be dark immediately after reload');
  assert.equal(immediateTheme.themeMeta, 'dark', 'meta[name="color-scheme"] must be dark immediately after reload');

  await new Promise((r) => setTimeout(r, 400));
  assert.equal(await evalJs(`document.querySelector('input[name="ui-theme"][value="dark"]')?.checked`), true);
  for (const r of recordedRequests) {
    assert.equal(r.token, SESSION_TOKEN, `Missing X-DIM-Session-Token after reload on ${r.method} ${r.urlPath}`);
  }

  // Reset cleanly to vi-VN / light / 1160x780
  await evalJs(`(() => {
    const l = document.querySelector('input[name="ui-locale"][value="vi-VN"]'); l.checked = true; l.dispatchEvent(new Event('change', { bubbles: true }));
    const t = document.querySelector('input[name="ui-theme"][value="light"]'); t.checked = true; t.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  assert.equal(await evalJs(`document.documentElement.getAttribute('data-theme')`), 'light');
  assert.equal(await evalJs(`document.documentElement.lang`), 'vi');

  // (c) & (d): Scale 0 -> Add Account Step 1 (double-submit, pending, inline error DUPLICATE_NAME, edit, Step 2, Launch PROCESS_ERROR -> card remains on Home) -> Scale 1
  const savedProfiles = JSON.parse(JSON.stringify(mockData.status.profiles));
  mockData.status.profiles = [];
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  const emptyCheck = await evalJs(`({
    emptyVisible: !document.getElementById('profiles-empty').hidden,
    searchHidden: document.getElementById('search-wrap').hidden,
    summaryHidden: document.getElementById('accounts-count-summary').hidden,
    noOverflow: document.documentElement.scrollWidth <= window.innerWidth
  })`);
  assert.deepEqual(emptyCheck, { emptyVisible: true, searchHidden: true, summaryHidden: true, noOverflow: true });
  await snap('account-dock-empty-vi-light.png');

  await evalJs(`document.getElementById('btn-empty-create').click()`);
  await snap('account-dock-add-step1-vi-light.png');

  mockData.delayNextCreateMs = 250;
  mockData.failNextCreate = { ok: false, error_code: 'DUPLICATE_NAME', message: 'Profile name already exists' };
  const beforeCreatePosts = recordedRequests.filter((r) => r.method === 'POST' && r.urlPath === '/api/profiles').length;

  await evalJs(`(() => {
    document.getElementById('create-profile-name').value = 'Công việc';
    document.getElementById('create-profile-notes').value = 'Nhóm dự án nội bộ';
    const color = document.querySelector('input[name="create-account-color"][value="teal"]');
    if (color) color.checked = true;
    document.getElementById('btn-submit-create').click();
    document.getElementById('create-profile-form').requestSubmit();
  })()`);

  await new Promise((r) => setTimeout(r, 60));
  const pendingCreate = await evalJs(`(() => {
    const btn = document.getElementById('btn-submit-create');
    return {
      disabled: btn.disabled,
      ariaBusy: btn.getAttribute('aria-busy'),
      text: btn.textContent
    };
  })()`);
  assert.equal(pendingCreate.disabled, true);
  assert.equal(pendingCreate.ariaBusy, 'true');
  assert.equal(pendingCreate.text, 'Đang tạo...');

  await new Promise((r) => setTimeout(r, 280));
  const afterCreatePosts = recordedRequests.filter((r) => r.method === 'POST' && r.urlPath === '/api/profiles').length;
  assert.equal(afterCreatePosts - beforeCreatePosts, 1, 'Double-submit must send only 1 POST /api/profiles');

  const step1ErrState = await evalJs(`(() => {
    const dialog = document.getElementById('create-profile-dialog');
    const nameEl = document.getElementById('create-profile-name');
    const notesEl = document.getElementById('create-profile-notes');
    const errEl = document.getElementById('create-step1-error');
    return {
      dialogOpen: dialog.open,
      step1Visible: !document.getElementById('create-step-1').hidden,
      step2Hidden: document.getElementById('create-step-2').hidden,
      nameVal: nameEl.value,
      notesVal: notesEl.value,
      errHidden: errEl.hidden,
      errText: errEl.textContent,
      ariaInvalid: nameEl.getAttribute('aria-invalid'),
      ariaDescribedBy: nameEl.getAttribute('aria-describedby')
    };
  })()`);
  assert.equal(step1ErrState.dialogOpen, true);
  assert.equal(step1ErrState.step1Visible, true);
  assert.equal(step1ErrState.step2Hidden, true);
  assert.equal(step1ErrState.nameVal, 'Công việc');
  assert.equal(step1ErrState.notesVal, 'Nhóm dự án nội bộ');
  assert.equal(step1ErrState.errHidden, false);
  assert.match(step1ErrState.errText, /Tên tài khoản này đã tồn tại/);
  assert.equal(step1ErrState.ariaInvalid, 'true');
  assert.equal(step1ErrState.ariaDescribedBy, 'create-step1-error');

  // Edit name clears inline error and succeeds to Step 2 with monotonic prof-101 ID
  await evalJs(`(() => {
    const nameEl = document.getElementById('create-profile-name');
    nameEl.value = 'Bán hàng';
    nameEl.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  assert.equal(await evalJs(`document.getElementById('create-step1-error').hidden`), true);
  assert.equal(await evalJs(`document.getElementById('create-profile-name').hasAttribute('aria-invalid')`), false);

  await evalJs(`document.getElementById('create-profile-form').requestSubmit()`);
  await new Promise((r) => setTimeout(r, 200));
  const step2State = await evalJs(`({
    step2Visible: !document.getElementById('create-step-2').hidden,
    step1Hidden: document.getElementById('create-step-1').hidden,
    title: document.getElementById('create-step2-title').textContent
  })`);
  assert.equal(step2State.step2Visible, true);
  assert.equal(step2State.step1Hidden, true);
  assert.equal(mockData.status.profiles[0]?.id, 'prof-101', 'New profile ID must use monotonic seq >= 101');
  await snap('account-dock-add-step2-vi-light.png');

  // Step 2 Launch failure (PROCESS_ERROR): dialog closes, toast from create does not block inline error, remediation card & inline card error allow retry
  mockData.failNextAction = { ok: false, error_code: 'PROCESS_ERROR', message: 'Failed to launch Zalo executable' };
  await evalJs(`document.getElementById('btn-create-launch').click()`);
  await new Promise((r) => setTimeout(r, 180));
  const afterStep2Fail = await evalJs(`(() => {
    const remCard = document.getElementById('desktop-remediation-card');
    const card = document.querySelector('#profiles-list .account-card[data-profile-id="prof-101"]');
    const inlineErr = card?.querySelector('[data-profile-error]');
    return {
      dialogOpen: document.getElementById('create-profile-dialog').open,
      stageVisible: !document.getElementById('stage-desktop-session').hidden,
      remVisible: !remCard.hidden,
      remBody: document.getElementById('desktop-remediation-body').textContent,
      inlineErrCode: inlineErr?.dataset.profileError || null,
      cardVisible: Boolean(card),
      launchEnabled: card ? !card.querySelector('.btn-primary').disabled : false,
      totalCards: document.querySelectorAll('#profiles-list .account-card').length
    };
  })()`);
  assert.equal(afterStep2Fail.dialogOpen, false);
  assert.equal(afterStep2Fail.stageVisible, true);
  assert.equal(afterStep2Fail.remVisible, true);
  assert.match(afterStep2Fail.remBody, /Không thể mở Zalo/);
  assert.equal(afterStep2Fail.inlineErrCode, 'PROCESS_ERROR');
  assert.equal(afterStep2Fail.cardVisible, true);
  assert.equal(afterStep2Fail.launchEnabled, true);
  assert.equal(afterStep2Fail.totalCards, 1);

  // Back button on Desktop Stage returns cleanly to Home and shows inline error + 1-account scale
  await evalJs(`document.getElementById('btn-desktop-back').click()`);
  await new Promise((r) => setTimeout(r, 80));
  const homeAfterBack = await evalJs(`({
    profilesVisible: !document.getElementById('view-profiles').hidden,
    searchHidden: document.getElementById('search-wrap').hidden,
    summaryHidden: document.getElementById('accounts-count-summary').hidden,
    noOverflow: document.documentElement.scrollWidth <= window.innerWidth
  })`);
  assert.deepEqual(homeAfterBack, { profilesVisible: true, searchHidden: true, summaryHidden: true, noOverflow: true });
  await snap('account-dock-1account-vi-light.png');

  // Retry launch from Home card succeeds and clears inline error
  await evalJs(`document.querySelector('#profiles-list .account-card[data-profile-id="prof-101"] .btn-primary').click()`);
  await new Promise((r) => setTimeout(r, 150));
  assert.equal(await evalJs(`Boolean(document.querySelector('#profiles-list .account-card[data-profile-id="prof-101"] [data-profile-error]'))`), false);
  assert.equal(await evalJs(`!document.getElementById('stage-desktop-session').hidden`), true);
  await snap('account-dock-desktop-stage-vi-light.png');
  await evalJs(`document.querySelector('.top-nav-btn[data-view="profiles"]').click()`);

  // Restore 3 accounts (2 running) for Journey B, Search + Escape, Menu, Help, Settings, Diagnostics, Backups
  mockData.status.profiles = [
    ...savedProfiles,
    {
      id: 'prof-3',
      name: 'Bán hàng',
      directory_name: 'ban-hang',
      created_at: '2026-02-10T09:00:00Z',
      last_launched_at: '2026-02-11T10:00:00Z',
      zalo_user_id: null,
      notes: 'Nhóm khách hàng',
      running: true,
      main_pid: 4105,
      process_count: 2,
      window_title: 'Zalo',
      total_working_set_bytes: 180 * 1024 * 1024,
      data_size_bytes: 64 * 1024 * 1024,
      stale_lockfile_detected: false
    }
  ];
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  const threeScale = await evalJs(`({
    cards: document.querySelectorAll('#profiles-list .account-card').length,
    searchHidden: document.getElementById('search-wrap').hidden,
    summaryHidden: document.getElementById('accounts-count-summary').hidden,
    noOverflow: document.documentElement.scrollWidth <= window.innerWidth
  })`);
  assert.deepEqual(threeScale, { cards: 3, searchHidden: false, summaryHidden: true, noOverflow: true });
  await snap('account-dock-3accounts-vi-light.png');

  // Search filter + summary visible while searching + Escape clears search
  await evalJs(`(() => {
    const inp = document.getElementById('profile-search');
    inp.value = 'Bán hàng';
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    inp.focus();
  })()`);
  const searchActive = await evalJs(`({
    cards: document.querySelectorAll('#profiles-list .account-card').length,
    summaryHidden: document.getElementById('accounts-count-summary').hidden,
    summaryText: document.getElementById('accounts-count-summary').textContent
  })`);
  assert.equal(searchActive.cards, 1);
  assert.equal(searchActive.summaryHidden, false);
  assert.equal(searchActive.summaryText, 'Hiển thị 1 / 3 tài khoản');
  await pressKey('Escape', 'Escape', 27);
  const searchCleared = await evalJs(`({
    val: document.getElementById('profile-search').value,
    cards: document.querySelectorAll('#profiles-list .account-card').length,
    summaryHidden: document.getElementById('accounts-count-summary').hidden
  })`);
  assert.deepEqual(searchCleared, { val: '', cards: 3, summaryHidden: true });

  // (j) WAI-ARIA Menu Button keyboard navigation + disabled menu item hints (Đóng Zalo trước)
  await evalJs(`document.querySelector('#profiles-list .account-card:first-child button[aria-haspopup="menu"]').focus()`);
  await pressKey('Enter', 'Enter', 13);
  const menuOpened = await evalJs(`(() => {
    const card = document.querySelector('#profiles-list .account-card:first-child');
    const btn = card.querySelector('button[aria-haspopup="menu"]');
    const items = Array.from(card.querySelectorAll('[role="menuitem"]'));
    return {
      expanded: btn.getAttribute('aria-expanded'),
      activeRole: document.activeElement?.getAttribute('role'),
      backupDisabled: items[1]?.disabled,
      backupHint: items[1]?.querySelector('.menu-item-hint')?.textContent,
      deleteDisabled: items[3]?.disabled,
      deleteHint: items[3]?.querySelector('.menu-item-hint')?.textContent
    };
  })()`);
  assert.equal(menuOpened.expanded, 'true');
  assert.equal(menuOpened.activeRole, 'menuitem');
  assert.equal(menuOpened.backupDisabled, true);
  assert.equal(menuOpened.backupHint, 'Đóng Zalo trước');
  assert.equal(menuOpened.deleteDisabled, true);
  assert.equal(menuOpened.deleteHint, 'Đóng Zalo trước');
  await snap('account-dock-menu-vi-light.png');
  await pressKey('ArrowDown', 'ArrowDown', 40);
  await pressKey('Escape', 'Escape', 27);
  const menuClosed = await evalJs(`(() => {
    const btn = document.querySelector('#profiles-list .account-card:first-child button[aria-haspopup="menu"]');
    return { expanded: btn.getAttribute('aria-expanded'), focusedBack: document.activeElement === btn };
  })()`);
  assert.equal(menuClosed.expanded, 'false');
  assert.equal(menuClosed.focusedBack, true);

  // (i) Help view (#btn-nav-help -> #view-help, 4 guide cards, #btn-help-diagnostics -> #view-diagnostics)
  await evalJs(`document.getElementById('btn-nav-help').click()`);
  const helpState = await evalJs(`({
    helpVisible: !document.getElementById('view-help').hidden,
    profilesHidden: document.getElementById('view-profiles').hidden,
    navActive: document.getElementById('btn-nav-help').classList.contains('is-active'),
    ariaCurrent: document.getElementById('btn-nav-help').getAttribute('aria-current'),
    cardCount: document.querySelectorAll('#view-help .help-grid .panel').length,
    title: document.title
  })`);
  assert.deepEqual(helpState, {
    helpVisible: true,
    profilesHidden: true,
    navActive: true,
    ariaCurrent: 'page',
    cardCount: 4,
    title: 'KHOANG — Hướng dẫn sử dụng'
  });
  await snap('account-dock-help-vi-light.png');

  await evalJs(`document.getElementById('btn-help-diagnostics').click()`);
  assert.equal(await evalJs(`!document.getElementById('view-diagnostics').hidden`), true);
  assert.equal(await evalJs(`document.getElementById('view-help').hidden`), true);
  assert.equal(await evalJs(`document.getElementById('btn-diag-back').getAttribute('data-back-to')`), 'help');
  await evalJs(`document.getElementById('btn-diag-back').click()`);
  assert.equal(await evalJs(`!document.getElementById('view-help').hidden`), true);
  assert.equal(await evalJs(`document.activeElement?.id`), 'btn-help-diagnostics');

  // 3. Native Radio Group keyboard navigation in Settings + Advanced Diagnostics
  await evalJs(`document.querySelector('.top-nav-btn[data-view="settings"]').click()`);
  await snap('account-dock-settings-vi-light.png');
  await evalJs(`document.querySelector('input[name="ui-locale"][value="vi-VN"]').focus()`);
  await pressKey('ArrowRight', 'ArrowRight', 39);
  assert.equal(await evalJs(`document.documentElement.lang`), 'en');
  await pressKey('ArrowLeft', 'ArrowLeft', 37);
  assert.equal(await evalJs(`document.documentElement.lang`), 'vi');

  await evalJs(`document.getElementById('btn-goto-diagnostics').click()`);
  await snap('account-dock-diagnostics-vi-light.png');
  const diagText = await evalJs(`document.getElementById('diagnostics-tbody').textContent`);
  assert.match(diagText, /NOT_TESTED/);
  assert.match(diagText, /BLOCKED/);
  const diagSummaryText = await evalJs(`document.getElementById('diag-health-list').textContent`);
  assert.match(diagSummaryText, /chưa thể kết luận an toàn toàn bộ/);
  assert.doesNotMatch(diagSummaryText, /An toàn —/);

  // 4. Backups view
  await evalJs(`document.querySelector('.top-nav-btn[data-view="backups"]').click()`);
  await snap('account-dock-backups-vi-light.png');

  // 5. Async per-account pending isolation + Error Banner + Details + 200% Zoom
  await evalJs(`document.querySelector('.top-nav-btn[data-view="profiles"]').click()`);
  mockData.delayNextLaunchMs = 250;
  await evalJs(`document.querySelector('#profiles-list .account-card:nth-child(2) .btn-primary').click()`);
  await new Promise((r) => setTimeout(r, 60));
  const pendingCheck = await evalJs(`(() => ({
    prof2Disabled: document.querySelector('#profiles-list .account-card:nth-child(2) .btn-primary').disabled,
    prof1Enabled: !document.querySelector('#profiles-list .account-card:nth-child(1) .btn-primary').disabled
  }))()`);
  assert.equal(pendingCheck.prof2Disabled, true);
  assert.equal(pendingCheck.prof1Enabled, true);
  await new Promise((r) => setTimeout(r, 300));
  await evalJs(`document.querySelector('.top-nav-btn[data-view="profiles"]').click()`);
  await new Promise((r) => setTimeout(r, 80));

  // Primary protection guard must not blame secondary card or show Retry / assertive alert
  mockData.failNextAction = { ok: false, error_code: 'PROTECTED_PRIMARY_PROCESS', message: 'Target PID 4120 is primary Zalo' };
  await evalJs(`document.querySelector('#profiles-list .account-card:first-child .btn-subtle-danger').click()`);
  await new Promise((r) => setTimeout(r, 150));
  const guardCheck = await evalJs(`(() => {
    const banner = document.getElementById('status-banner');
    const card1Inline = document.querySelector('#profiles-list .account-card:first-child [data-profile-error]');
    const btns = Array.from(banner?.querySelectorAll('button') || []).map((b) => b.textContent.trim());
    return {
      bannerVisible: !banner.hidden,
      bannerRole: banner.getAttribute('role'),
      bannerLive: banner.getAttribute('aria-live'),
      bannerIsInfo: banner.classList.contains('banner-info'),
      hasRetryBtn: btns.some((txt) => /Thử lại|Retry/i.test(txt)),
      card1HasError: Boolean(card1Inline)
    };
  })()`);
  assert.equal(guardCheck.bannerVisible, true);
  assert.equal(guardCheck.bannerRole, 'status');
  assert.equal(guardCheck.bannerLive, 'polite');
  assert.equal(guardCheck.bannerIsInfo, true);
  assert.equal(guardCheck.hasRetryBtn, false);
  assert.equal(guardCheck.card1HasError, false);
  await evalJs(`document.getElementById('btn-dismiss-banner')?.click()`);

  // Recoverable per-account error renders inline on affected card with Retry + focus restoration
  mockData.failNextAction = { ok: false, error_code: 'PROCESS_ERROR', message: 'Failed to stop child process 9901' };
  await evalJs(`document.querySelector('#profiles-list .account-card:first-child .btn-subtle-danger').click()`);
  await new Promise((r) => setTimeout(r, 150));
  const errCheck = await evalJs(`(() => {
    const inlineEl = document.querySelector('#profiles-list .account-card:first-child [data-profile-error]');
    const btns = Array.from(inlineEl?.querySelectorAll('button') || []).map((b) => b.textContent.trim());
    return {
      code: inlineEl?.dataset.profileError || null,
      role: inlineEl?.getAttribute('role') || null,
      hasRetryBtn: btns.some((txt) => /Thử lại|Retry/i.test(txt)),
      text: inlineEl?.querySelector('.account-inline-error-text')?.textContent || '',
      globalBannerHidden: document.getElementById('status-banner').hidden,
      siblingDisabled: document.querySelector('#profiles-list .account-card:nth-child(2) .btn-primary')?.disabled || false
    };
  })()`);
  assert.equal(errCheck.code, 'PROCESS_ERROR');
  assert.equal(errCheck.role, 'alert');
  assert.equal(errCheck.hasRetryBtn, true);
  assert.equal(errCheck.siblingDisabled, false);
  assert.doesNotMatch(errCheck.text, /9901/);
  assert.equal(errCheck.globalBannerHidden, true);
  await snap('account-dock-error-vi-light.png');

  // Dismiss inline error and verify focus restores to card action button
  await evalJs(`document.querySelector('#profiles-list .account-card:first-child [data-profile-error] .btn-link').click()`);
  const dismissFocus = await evalJs(`({
    errGone: !document.querySelector('#profiles-list .account-card:first-child [data-profile-error]'),
    focusInsideCard: Boolean(document.activeElement?.closest('#profiles-list .account-card:first-child'))
  })`);
  assert.equal(dismissFocus.errGone, true);
  assert.equal(dismissFocus.focusInsideCard, true);

  await evalJs(`document.querySelector('#profiles-list .account-card:first-child [role="menuitem"]:nth-child(3)').click()`);
  const detailsCheck = await evalJs(`({
    basic: document.getElementById('profile-details-list').textContent,
    advanced: document.getElementById('profile-details-advanced-list').textContent
  })`);
  assert.match(detailsCheck.basic, /Màu nhận diện/);
  assert.match(detailsCheck.advanced, /100204819/);
  await snap('account-dock-details-vi-light.png');
  await evalJs(`document.getElementById('profile-details-dialog').close('close')`);
  await evalJs(`document.getElementById('btn-dismiss-banner')?.click()`);

  // Zoom 200% simulation (580x390 CSS px)
  await setViewport(580, 390, 2);
  const zoomMetrics = await evalJs(`({ scrollWidth: document.documentElement.scrollWidth, innerWidth: window.innerWidth })`);
  assert.ok(zoomMetrics.scrollWidth <= zoomMetrics.innerWidth, 'Overflow at 200% zoom');
  await snap('account-dock-zoom-200-vi-light.png');

  // 6. Full API operations + double-click dedup + (e) Color persistence across rename, delete removes color, restore preserves source color
  await setViewport(1160, 780, 1);
  await evalJs(`document.querySelector('#profiles-list .account-card:nth-child(2) .btn-subtle-danger')?.click()`);
  await new Promise((r) => setTimeout(r, 150));
  recordedRequests.length = 0;

  mockData.delayNextLaunchMs = 200;
  await evalJs(`(() => {
    const btn = document.querySelector('#profiles-list .account-card:nth-child(2) .btn-primary');
    btn.click();
    btn.click();
  })()`);
  await new Promise((r) => setTimeout(r, 320));
  assert.equal(recordedRequests.filter((r) => r.urlPath === '/api/profiles/prof-2/launch').length, 1, 'Double-click must send only 1 launch request');

  await evalJs(`document.querySelector('#profiles-list .account-card:nth-child(1) .btn-primary').click()`);
  await new Promise((r) => setTimeout(r, 120));
  await evalJs(`document.querySelector('#profiles-list .account-card:nth-child(2) .btn-subtle-danger').click()`);
  await new Promise((r) => setTimeout(r, 120));
  await evalJs(`document.querySelector('#profiles-list .account-card:nth-child(2) [role="menuitem"]:nth-child(2)').click()`);
  await new Promise((r) => setTimeout(r, 120));

  // (e.1) Rename prof-1 changing color to 'plum', then rename prof-1 again preserving 'plum'
  await evalJs(`(() => {
    document.querySelector('#profiles-list .account-card[data-profile-id="prof-1"] [role="menuitem"]:nth-child(1)').click();
    const plumRadio = document.querySelector('input[name="rename-account-color"][value="plum"]');
    plumRadio.checked = true;
    document.getElementById('rename-profile-name').value = 'Đức — Công việc (Đổi màu)';
    document.querySelector('#rename-profile-dialog button[value="rename"]').click();
  })()`);
  await new Promise((r) => setTimeout(r, 150));
  const prof1AfterColorChange = await evalJs(`({
    avatarVar: document.querySelector('#profiles-list .account-card[data-profile-id="prof-1"] .account-avatar').style.getPropertyValue('--avatar-color'),
    storedColor: JSON.parse(localStorage.getItem('multi-zalo.accountColors') || '{}')['prof-1']
  })`);
  assert.equal(prof1AfterColorChange.avatarVar, 'var(--id-plum)');
  assert.equal(prof1AfterColorChange.storedColor, 'plum');

  await evalJs(`(() => {
    document.querySelector('#profiles-list .account-card[data-profile-id="prof-1"] [role="menuitem"]:nth-child(1)').click();
  })()`);
  assert.equal(await evalJs(`document.querySelector('input[name="rename-account-color"][value="plum"]')?.checked`), true);
  await evalJs(`(() => {
    document.getElementById('rename-profile-name').value = 'Đức — Công việc & Đối tác miền Nam';
    document.querySelector('#rename-profile-dialog button[value="rename"]').click();
  })()`);
  await new Promise((r) => setTimeout(r, 150));
  const prof1AfterSecondRename = await evalJs(`({
    avatarVar: document.querySelector('#profiles-list .account-card[data-profile-id="prof-1"] .account-avatar').style.getPropertyValue('--avatar-color'),
    storedColor: JSON.parse(localStorage.getItem('multi-zalo.accountColors') || '{}')['prof-1']
  })`);
  assert.equal(prof1AfterSecondRename.avatarVar, 'var(--id-plum)');
  assert.equal(prof1AfterSecondRename.storedColor, 'plum');

  // (e.2) Rename prof-3 with coral, stop prof-3, delete prof-3 -> removes color from localStorage
  await evalJs(`(() => {
    document.querySelector('#profiles-list .account-card[data-profile-id="prof-3"] [role="menuitem"]:nth-child(1)').click();
    document.querySelector('input[name="rename-account-color"][value="coral"]').checked = true;
    document.getElementById('rename-profile-name').value = 'Bán hàng (Cập nhật)';
    document.querySelector('#rename-profile-dialog button[value="rename"]').click();
  })()`);
  await new Promise((r) => setTimeout(r, 150));
  assert.equal(await evalJs(`JSON.parse(localStorage.getItem('multi-zalo.accountColors') || '{}')['prof-3']`), 'coral');

  await evalJs(`document.querySelector('#profiles-list .account-card[data-profile-id="prof-3"] .btn-subtle-danger').click()`);
  await new Promise((r) => setTimeout(r, 150));
  await evalJs(`(() => {
    document.querySelector('#profiles-list .account-card[data-profile-id="prof-3"] [role="menuitem"]:nth-child(4)').click();
    document.querySelector('#delete-profile-dialog button[value="delete"]').click();
  })()`);
  await new Promise((r) => setTimeout(r, 150));
  const prof3ColorRemoved = await evalJs(`Object.hasOwn(JSON.parse(localStorage.getItem('multi-zalo.accountColors') || '{}'), 'prof-3')`);
  assert.equal(prof3ColorRemoved, false, 'Deleting profile must remove its saved color from localStorage');

  // (e.3) Restore backup bk-1 (source_profile_id = 'prof-1') -> preserves 'plum' on newly restored profile
  await evalJs(`(() => {
    document.querySelector('.top-nav-btn[data-view="backups"]').click();
    document.querySelector('#backups-tbody .btn-primary').click();
    document.getElementById('restore-profile-name').value = 'Khôi phục từ sao lưu';
    document.querySelector('#restore-backup-dialog button[value="restore"]').click();
  })()`);
  await new Promise((r) => setTimeout(r, 200));
  await evalJs(`document.querySelector('.top-nav-btn[data-view="profiles"]').click()`);
  const restoredProfile = mockData.status.profiles.find((p) => p.name === 'Khôi phục từ sao lưu');
  assert.ok(restoredProfile, 'Restored profile should exist in mockData');
  const restoredColorState = await evalJs(`({
    avatarVar: document.querySelector('#profiles-list .account-card[data-profile-id="${restoredProfile.id}"] .account-avatar')?.style.getPropertyValue('--avatar-color'),
    storedColor: JSON.parse(localStorage.getItem('multi-zalo.accountColors') || '{}')['${restoredProfile.id}']
  })`);
  assert.equal(restoredColorState.avatarVar, 'var(--id-plum)', 'Restored account must inherit source_profile_id color');
  assert.equal(restoredColorState.storedColor, 'plum');

  const seenRoutes = new Set(recordedRequests.map((r) => `${r.method} ${r.urlPath}`));
  for (const expected of [
    'POST /api/profiles/prof-2/launch',
    'POST /api/profiles/prof-1/focus',
    'POST /api/profiles/prof-2/stop',
    'POST /api/profiles/prof-2/backup',
    'POST /api/profiles/prof-1/rename',
    'POST /api/profiles/prof-3/rename',
    'DELETE /api/profiles/prof-3',
    'POST /api/backups/restore'
  ]) {
    assert.ok(seenRoutes.has(expected), `Missing recorded route: ${expected}`);
  }
  for (const r of recordedRequests) {
    assert.equal(r.token, SESSION_TOKEN, `Missing X-DIM-Session-Token on ${r.method} ${r.urlPath}`);
  }

  // (f) Primary Zalo honest states: true, false, null, disconnectStatus = true, and statusOkFalse
  await evalJs(`document.getElementById('btn-dismiss-banner')?.click()`);
  const readPrimary = () => evalJs(`({
    classes: document.getElementById('primary-zalo-state-row').className,
    desc: document.getElementById('primary-zalo-desc').textContent,
    buttons: document.querySelectorAll('#primary-zalo-section button').length,
    connHidden: document.getElementById('connection-pill').hidden,
    bannerVisible: !document.getElementById('status-banner').hidden
  })`);

  let primaryState = await readPrimary();
  assert.match(primaryState.classes, /\bis-running\b/);
  assert.match(primaryState.desc, /Cửa sổ đang mở/);
  assert.equal(primaryState.buttons, 0);

  mockData.status.primary_zalo.is_running = false;
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  primaryState = await readPrimary();
  assert.match(primaryState.classes, /\bis-stopped\b/);
  assert.match(primaryState.desc, /Đã đóng/);
  assert.equal(primaryState.buttons, 0);

  mockData.status.primary_zalo.is_running = null;
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  primaryState = await readPrimary();
  assert.match(primaryState.classes, /\bis-unknown\b/);
  assert.match(primaryState.desc, /Chưa xác định/);
  assert.equal(primaryState.buttons, 0);

  // Cache running:true then disconnect bridge -> must become unknown, never fake running
  mockData.status.primary_zalo.is_running = true;
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  mockData.disconnectStatus = true;
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 200));
  primaryState = await readPrimary();
  assert.match(primaryState.classes, /\bis-unknown\b/);
  assert.match(primaryState.desc, /Chưa xác định/);
  assert.doesNotMatch(primaryState.desc, /Cửa sổ đang mở/);
  assert.equal(primaryState.buttons, 0);
  assert.equal(primaryState.connHidden, false);

  mockData.disconnectStatus = false;
  mockData.statusOkFalse = { ok: false, error_code: 'HOST_EXEC_FAILED', message: 'Status failure' };
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  primaryState = await readPrimary();
  assert.match(primaryState.classes, /\bis-unknown\b/);
  assert.match(primaryState.desc, /Chưa xác định/);
  assert.equal(primaryState.bannerVisible, true);

  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  await evalJs(`document.getElementById('btn-dismiss-banner')?.click()`);
  assert.equal(await evalJs(`document.getElementById('connection-pill').hidden`), true);

  // (g) Network timeout (window.__MZ_API_TIMEOUT_MS = 120 + delayed /api/status) -> HOST_TIMEOUT banner
  mockData.delayNextStatusMs = 350;
  await evalJs(`window.__MZ_API_TIMEOUT_MS = 120; document.getElementById('btn-refresh').click();`);
  await new Promise((r) => setTimeout(r, 220));
  const timeoutBanner = await evalJs(`({
    visible: !document.getElementById('status-banner').hidden,
    text: document.getElementById('status-banner-text').textContent,
    raw: document.getElementById('status-banner-raw').textContent,
    connHidden: document.getElementById('connection-pill').hidden
  })`);
  assert.equal(timeoutBanner.visible, true);
  assert.match(timeoutBanner.text, /Thao tác phản hồi quá lâu/);
  assert.match(timeoutBanner.raw, /HOST_TIMEOUT/);
  assert.equal(timeoutBanner.connHidden, false);

  await new Promise((r) => setTimeout(r, 200));
  await evalJs(`delete window.__MZ_API_TIMEOUT_MS; document.getElementById('btn-refresh').click();`);
  await new Promise((r) => setTimeout(r, 150));
  await evalJs(`document.getElementById('btn-dismiss-banner')?.click()`);

  // (h) Mock window.__TAURI_INTERNALS__.invoke in-page for delete_profile ({ id, confirm: true }) and get_status
  const tauriTmpProfile = {
    id: 'prof-tauri-tmp',
    name: 'Tài khoản tạm Tauri',
    directory_name: 'profile-tauri-tmp',
    created_at: '2026-02-12T08:00:00Z',
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
  };
  mockData.status.profiles.push(tauriTmpProfile);
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));

  const tauriSnapshot = JSON.stringify(mockData.status);
  const tauriDiag = JSON.stringify(mockData.diagnostics);
  await evalJs(`(() => {
    const statusObj = ${tauriSnapshot};
    const diagObj = ${tauriDiag};
    window.__TAURI_CALLS__ = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd, args) => {
        window.__TAURI_CALLS__.push({ cmd, args });
        if (cmd === 'delete_profile') {
          statusObj.profiles = statusObj.profiles.filter((p) => p.id !== args?.id);
          return { ok: true, message: 'Deleted' };
        }
        if (cmd === 'get_status') return statusObj;
        if (cmd === 'list_backups') return { backups: [] };
        if (cmd === 'get_diagnostics') return diagObj;
        if (cmd === 'verify_isolation') return { ok: true, checks: [] };
        return { ok: true };
      }
    };
  })()`);

  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 120));
  await evalJs(`(() => {
    document.querySelector('#profiles-list .account-card[data-profile-id="prof-tauri-tmp"] [role="menuitem"]:nth-child(4)').click();
    document.querySelector('#delete-profile-dialog button[value="delete"]').click();
  })()`);
  await new Promise((r) => setTimeout(r, 150));

  const tauriCalls = await evalJs(`(() => {
    const calls = window.__TAURI_CALLS__;
    delete window.__TAURI_INTERNALS__;
    delete window.__TAURI_CALLS__;
    return calls;
  })()`);
  const getStatusCall = tauriCalls.find((c) => c.cmd === 'get_status');
  const deleteProfileCall = tauriCalls.find((c) => c.cmd === 'delete_profile');
  assert.ok(getStatusCall, 'Expected Tauri invoke(get_status)');
  assert.ok(deleteProfileCall, 'Expected Tauri invoke(delete_profile)');
  assert.deepEqual(deleteProfileCall.args, { id: 'prof-tauri-tmp', confirm: true });

  mockData.status.profiles = mockData.status.profiles.filter((p) => p.id !== 'prof-tauri-tmp');
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  await evalJs(`document.getElementById('btn-dismiss-banner')?.click()`);

  // (j) Out-of-order /api/status responses + Overflow menu stays open across refresh
  const expectedCount = mockData.status.profiles.length;
  const staleStatus = JSON.parse(JSON.stringify(mockData.status));
  staleStatus.profiles = [staleStatus.profiles[0]];
  const freshStatus = JSON.parse(JSON.stringify(mockData.status));
  mockData.statusDelayQueue = [
    { delayMs: 220, payload: staleStatus },
    { delayMs: 20, payload: freshStatus }
  ];
  await evalJs(`document.getElementById('btn-run-isolation').click()`);
  await new Promise((r) => setTimeout(r, 30));
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 320));
  assert.equal(await evalJs(`document.querySelectorAll('#profiles-list .account-card').length`), expectedCount);

  await evalJs(`document.querySelector('#profiles-list .account-card:first-child button[aria-haspopup="menu"]').click()`);
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  assert.equal(await evalJs(`document.querySelector('#profiles-list .account-card:first-child button[aria-haspopup="menu"]').getAttribute('aria-expanded')`), 'true');
  await pressKey('Escape', 'Escape', 27);

  // (c) Scale 10 & 20 accounts + search at scale + account-dock-20accounts-vi-light.png
  await evalJs(`document.getElementById('btn-dismiss-banner')?.click()`);
  mockData.status.profiles = buildSyntheticProfiles(10);
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  const scale10 = await evalJs(`({
    cards: document.querySelectorAll('#profiles-list .account-card').length,
    searchHidden: document.getElementById('search-wrap').hidden,
    summaryHidden: document.getElementById('accounts-count-summary').hidden,
    summaryText: document.getElementById('accounts-count-summary').textContent,
    noOverflow: document.documentElement.scrollWidth <= window.innerWidth
  })`);
  assert.deepEqual(scale10, {
    cards: 10,
    searchHidden: false,
    summaryHidden: false,
    summaryText: 'Hiển thị 10 / 10 tài khoản',
    noOverflow: true
  });

  mockData.status.profiles = buildSyntheticProfiles(20);
  await evalJs(`document.getElementById('btn-refresh').click()`);
  await new Promise((r) => setTimeout(r, 150));
  const scale20 = await evalJs(`({
    cards: document.querySelectorAll('#profiles-list .account-card').length,
    searchHidden: document.getElementById('search-wrap').hidden,
    summaryHidden: document.getElementById('accounts-count-summary').hidden,
    summaryText: document.getElementById('accounts-count-summary').textContent,
    noOverflow: document.documentElement.scrollWidth <= window.innerWidth
  })`);
  assert.deepEqual(scale20, {
    cards: 20,
    searchHidden: false,
    summaryHidden: false,
    summaryText: 'Hiển thị 20 / 20 tài khoản',
    noOverflow: true
  });
  await snap('account-dock-20accounts-vi-light.png');

  await evalJs(`(() => {
    const inp = document.getElementById('profile-search');
    inp.value = 'Đà Nẵng';
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    inp.focus();
  })()`);
  assert.equal(await evalJs(`document.querySelectorAll('#profiles-list .account-card').length`), 1);
  assert.equal(await evalJs(`document.getElementById('accounts-count-summary').textContent`), 'Hiển thị 1 / 20 tài khoản');
  await pressKey('Escape', 'Escape', 27);
  assert.equal(await evalJs(`document.querySelectorAll('#profiles-list .account-card').length`), 20);
  assert.equal(await evalJs(`document.getElementById('accounts-count-summary').textContent`), 'Hiển thị 20 / 20 tài khoản');

  // Quick Switcher (Ctrl+K) + Account Rail context menu + Font telemetry check
  await evalJs(`(() => {
    const railBtn = document.querySelector('#rail-accounts .rail-account-btn');
    railBtn?.focus();
    railBtn?.dispatchEvent(new KeyboardEvent('keydown', { key: 'F10', shiftKey: true, bubbles: true }));
  })()`);
  await new Promise((r) => setTimeout(r, 60));
  const railCtx = await evalJs(`({
    menuVisible: !document.getElementById('account-context-menu').hidden,
    items: document.querySelectorAll('#account-context-menu [role="menuitem"]').length
  })`);
  assert.equal(railCtx.menuVisible, true);
  assert.ok(railCtx.items >= 2);
  await pressKey('Escape', 'Escape', 27);
  assert.equal(await evalJs(`document.getElementById('account-context-menu').hidden`), true);

  // P0 Feedback & P1 Navigation / Rail / Typography verification
  // 1. Toast dedupe, consecutive toasts, clipboard rejection, view switch & dialog coexistence
  await evalJs(`(() => {
    document.querySelectorAll('#toast-region .toast-close').forEach((btn) => btn.click());
    window.__origClipboard = navigator.clipboard;
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: async () => Promise.resolve() }
    });
  })()`);
  await evalJs(`document.querySelector('.top-nav-btn[data-view="settings"]').click()`);
  await evalJs(`document.getElementById('btn-goto-diagnostics').click()`);
  assert.equal(await evalJs(`document.getElementById('btn-diag-back').getAttribute('data-back-to')`), 'settings');
  await evalJs(`document.getElementById('btn-copy-diagnostics').click()`);
  await new Promise((r) => setTimeout(r, 40));
  await evalJs(`document.getElementById('btn-copy-diagnostics').click()`); // duplicate should dedupe
  await new Promise((r) => setTimeout(r, 40));
  const toastDedupe = await evalJs(`document.querySelectorAll('#toast-region .toast').length`);
  assert.equal(toastDedupe, 1, 'Duplicate copy diagnostics toast must deduplicate');

  // Clipboard rejection -> error toast (role/kind error, not success)
  await evalJs(`(() => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: async () => Promise.reject(new Error('Clipboard denied')) }
    });
  })()`);
  await evalJs(`document.getElementById('btn-copy-diagnostics').click()`);
  await new Promise((r) => setTimeout(r, 50));
  const toastTwo = await evalJs(`({
    count: document.querySelectorAll('#toast-region .toast').length,
    hasErrorToast: Boolean(document.querySelector('#toast-region .toast.toast-error'))
  })`);
  assert.equal(toastTwo.count, 2, 'Two distinct consecutive toasts must coexist');
  assert.equal(toastTwo.hasErrorToast, true, 'Clipboard rejection must emit an error toast');

  // View switch during active toast keeps #toast-region populated
  await evalJs(`document.getElementById('btn-diag-back').click()`);
  assert.equal(await evalJs(`!document.getElementById('view-settings').hidden`), true);
  assert.equal(await evalJs(`document.querySelectorAll('#toast-region .toast').length`), 2);

  // Critical global error banner coexists with toast without being wiped + repeated error dedupe
  mockData.delayNextStatusMs = 300;
  await evalJs(`window.__MZ_API_TIMEOUT_MS = 100; document.getElementById('btn-refresh').click();`);
  await new Promise((r) => setTimeout(r, 180));
  mockData.delayNextStatusMs = 300;
  await evalJs(`document.getElementById('btn-refresh').click();`);
  await new Promise((r) => setTimeout(r, 180));
  await evalJs(`window.__MZ_API_TIMEOUT_MS = 5000;`);
  const bannerAndToast = await evalJs(`({
    bannerVisible: !document.getElementById('status-banner').hidden,
    bannerRole: document.getElementById('status-banner').getAttribute('role'),
    toastCount: document.querySelectorAll('#toast-region .toast').length
  })`);
  assert.equal(bannerAndToast.bannerVisible, true);
  assert.equal(bannerAndToast.bannerRole, 'alert');
  assert.ok(bannerAndToast.toastCount >= 1);
  await evalJs(`document.getElementById('btn-dismiss-banner')?.click()`);

  // P1 Back Navigation: Desktop Stage -> Diagnostics -> Back -> Desktop Stage; direct rail reset
  await evalJs(`document.querySelector('#rail-accounts .rail-account-btn').click()`);
  await new Promise((r) => setTimeout(r, 100));
  await evalJs(`(() => {
    const stageBtn = document.querySelector('#desktop-remediation-card button, #btn-desktop-back');
    if (stageBtn && stageBtn.id === 'btn-desktop-back') stageBtn.click();
  })()`);
  await evalJs(`document.getElementById('btn-nav-help').click()`);
  await evalJs(`document.getElementById('btn-help-diagnostics').click()`);
  assert.equal(await evalJs(`document.getElementById('btn-diag-back').getAttribute('data-back-to')`), 'help');
  await evalJs(`document.getElementById('btn-diag-back').click()`);
  assert.equal(await evalJs(`!document.getElementById('view-help').hidden`), true);

  // Outside click closes account context menu
  await evalJs(`document.querySelector('.top-nav-btn[data-view="profiles"]').click()`);
  await evalJs(`(() => {
    const railBtn = document.querySelector('#rail-accounts .rail-account-btn');
    railBtn?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 60, clientY: 220 }));
  })()`);
  await new Promise((r) => setTimeout(r, 50));
  assert.equal(await evalJs(`!document.getElementById('account-context-menu').hidden`), true);
  await evalJs(`document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))`);
  await new Promise((r) => setTimeout(r, 50));
  assert.equal(await evalJs(`document.getElementById('account-context-menu').hidden`), true);

  const fontInfo = await evalJs(`({
    computedBodyFont: getComputedStyle(document.body).fontFamily,
    bodySize: getComputedStyle(document.body).fontSize,
    fontsStatus: document.fonts?.status || 'unknown',
    interLoaded: document.fonts ? document.fonts.check('14px Inter') : false,
    segoeLoaded: document.fonts ? document.fonts.check('14px "Segoe UI"') : false,
    toastRegionPresent: Boolean(document.getElementById('toast-region'))
  })`);
  assert.equal(fontInfo.bodySize, '14px');
  assert.equal(fontInfo.fontsStatus, 'loaded');
  assert.match(fontInfo.computedBodyFont, /Segoe UI|Inter|Be Vietnam Pro/i);
  assert.equal(fontInfo.toastRegionPresent, true);
  console.log('FONT_METRICS', JSON.stringify(fontInfo));

  await evalJs(`document.getElementById('btn-quick-switcher').click()`);
  await new Promise((r) => setTimeout(r, 80));
  const qsOpen = await evalJs(`({
    open: document.getElementById('quick-switcher-dialog').open,
    options: document.querySelectorAll('#quick-switcher-list [role="option"]').length
  })`);
  assert.equal(qsOpen.open, true);
  assert.ok(qsOpen.options >= 21);
  await snap('account-dock-quick-switcher-vi-light.png');
  await evalJs(`document.getElementById('quick-switcher-dialog').close('close')`);

  assert.deepEqual(runtimeErrors, [], `Runtime errors: ${runtimeErrors.join('; ')}`);
  console.log('PASS (Account Dock: 16 matrix combinations, session token, theme reload, scale 0/1/3/10/20, Step1/Step2 resilience, color persistence/restore, primary Zalo honest states, timeout, Tauri invoke, Help view, WAI-ARIA menu, out-of-order status, 200% zoom, 0 runtime errors)');
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
