#!/usr/bin/env node
/**
 * Real Windows Native Tauri (`dim-desktop.exe`) End-to-End Verification (§11).
 *
 * Launches the compiled `dim-desktop.exe` Win32 + WebView2 binary against an isolated
 * temporary `DIM_DATA_DIR` and `DIM_NODE_FIXTURE_SCRIPT` (`xf-fixture.mjs`), attaches
 * Chrome DevTools Protocol (CDP) directly to the live WebView2 window, asserts
 * `Boolean(window.__TAURI_INTERNALS__) === true` (zero browser mocks!), and exercises
 * all 12 mandatory §11 desktop lifecycle, persistence, and error-handling steps.
 */

import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const dimRoot = path.resolve(__dirname, '..');
const releaseBin = path.join(
  dimRoot,
  'apps',
  'desktop',
  'src-tauri',
  'target',
  'release',
  process.platform === 'win32' ? 'dim-desktop.exe' : 'dim-desktop'
);
const debugBin = path.join(
  dimRoot,
  'apps',
  'desktop',
  'src-tauri',
  'target',
  'debug',
  process.platform === 'win32' ? 'dim-desktop.exe' : 'dim-desktop'
);
const tauriBin = fs.existsSync(releaseBin) ? releaseBin : debugBin;
const fixtureScript = path.join(dimRoot, 'fixtures', 'cross-platform', 'xf-fixture.mjs');
const evidenceOutPath = path.join(dimRoot, 'evidence', 'tauri-native-e2e-result.json');

if (process.platform !== 'win32') {
  console.error('SKIPPED: Windows native WebView2 E2E requires win32 host.');
  process.exit(0);
}
if (!fs.existsSync(tauriBin)) {
  console.error(`MISSING BINARY: Expected compiled dim-desktop.exe at ${releaseBin}`);
  process.exit(1);
}
if (!fs.existsSync(fixtureScript)) {
  console.error(`MISSING FIXTURE: Expected xf-fixture.mjs at ${fixtureScript}`);
  process.exit(1);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function allocateFreePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      const addr = srv.address();
      const port = typeof addr === 'object' && addr ? addr.port : 0;
      srv.close((err) => (err ? reject(err) : resolve(port)));
    });
    srv.on('error', reject);
  });
}

function fetchJson(url) {
  return new Promise((resolve, reject) => {
    const req = http.get(url, { timeout: 1500 }, (res) => {
      if (res.statusCode !== 200) {
        res.resume();
        reject(new Error(`HTTP ${res.statusCode}`));
        return;
      }
      const chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => {
        try {
          resolve(JSON.parse(Buffer.concat(chunks).toString('utf8')));
        } catch (err) {
          reject(err);
        }
      });
    });
    req.on('error', reject);
    req.on('timeout', () => {
      req.destroy();
      reject(new Error('timeout'));
    });
  });
}

function isPidAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return err && err.code === 'EPERM';
  }
}

async function waitForWebViewTarget(port, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const targets = await fetchJson(`http://127.0.0.1:${port}/json`);
      if (Array.isArray(targets)) {
        const page = targets.find(
          (t) =>
            t.type === 'page' &&
            t.webSocketDebuggerUrl &&
            (String(t.url || '').includes('tauri.localhost') ||
              String(t.title || '').includes('Multi-Zalo'))
        );
        if (page) return page;
      }
    } catch {
      // Poll until WebView2 remote debugging endpoint starts.
    }
    await sleep(120);
  }
  throw new Error(`Timed out waiting for dim-desktop.exe WebView2 CDP target on port ${port}`);
}

async function connectCdpSession(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });

  let nextId = 1;
  const pending = new Map();
  const runtimeErrors = [];

  ws.addEventListener('message', (ev) => {
    const msg = JSON.parse(String(ev.data));
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(JSON.stringify(msg.error)));
      else resolve(msg.result);
      return;
    }
    if (msg.method === 'Runtime.exceptionThrown') {
      const text =
        msg.params?.exceptionDetails?.exception?.description ||
        msg.params?.exceptionDetails?.text ||
        'Unknown runtime exception';
      runtimeErrors.push(text);
    } else if (msg.method === 'Runtime.consoleAPICalled' && msg.params?.type === 'error') {
      const rendered = (msg.params.args || [])
        .map((a) => a.value ?? a.description ?? '')
        .join(' ');
      runtimeErrors.push(`console.error: ${rendered}`);
    }
  });

  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });

  await send('Runtime.enable');
  await send('Page.enable');

  const evalJs = async (expression) => {
    const res = await send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true
    });
    if (res.exceptionDetails) {
      throw new Error(
        `WebView2 eval exception: ${res.exceptionDetails.exception?.description || res.exceptionDetails.text}`
      );
    }
    return res.result?.value;
  };

  return { ws, send, evalJs, runtimeErrors };
}

async function launchTauriInstance({ dataDir, wv2ProfileDir }) {
  const debugPort = await allocateFreePort();
  const proc = spawn(tauriBin, [], {
    cwd: dimRoot,
    env: {
      ...process.env,
      DIM_DATA_DIR: dataDir,
      DIM_NODE_FIXTURE_SCRIPT: fixtureScript,
      WEBVIEW2_USER_DATA_FOLDER: wv2ProfileDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort} --remote-allow-origins=*`
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: false
  });

  let stderrBuf = '';
  proc.stderr.on('data', (chunk) => {
    stderrBuf += chunk.toString('utf8');
  });

  const target = await waitForWebViewTarget(debugPort, 25_000);
  const cdp = await connectCdpSession(target.webSocketDebuggerUrl);

  // Wait for Account Dock DOM + Tauri IPC bridge initialization
  const readyDeadline = Date.now() + 15_000;
  while (Date.now() < readyDeadline) {
    const ready = await cdp.evalJs(`Boolean(
      window.__TAURI_INTERNALS__ &&
      typeof window.__TAURI_INTERNALS__.invoke === 'function' &&
      document.getElementById('btn-open-create') &&
      document.getElementById('topbar-open-summary')
    )`);
    if (ready) break;
    await sleep(100);
  }

  return { proc, debugPort, target, cdp, getStderr: () => stderrBuf };
}

async function terminateProcessCleanly(proc) {
  if (!proc || proc.exitCode !== null) return;
  try {
    proc.kill();
  } catch {
    // Ignore if already exited.
  }
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline && proc.exitCode === null) {
    await sleep(80);
  }
}

function sanitizePaths(obj, replacements) {
  const raw = JSON.stringify(obj, null, 2);
  let out = raw;
  for (const [needle, token] of replacements) {
    if (!needle) continue;
    const fwd = needle.replace(/\\/g, '/');
    const bwd = needle.replace(/\//g, '\\');
    const escapedBwd = bwd.replace(/\\/g, '\\\\');
    out = out.split(escapedBwd).join(token);
    out = out.split(fwd).join(token);
    out = out.split(bwd).join(token);
  }
  try {
    const user = os.userInfo().username;
    if (user && user.length >= 2) {
      const re = new RegExp(`([\\\\/]Users[\\\\/])${user}(?=[\\\\/\"'])`, 'gi');
      out = out.replace(re, '$1<USER>');
    }
  } catch {
    // Ignore userInfo lookup issues.
  }
  return JSON.parse(out);
}

const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'dim-tauri-native-e2e-'));
const dataDir = path.join(tempRoot, 'dim-data');
const wv2Dir = path.join(tempRoot, 'webview2-profile');
fs.mkdirSync(dataDir, { recursive: true });
fs.mkdirSync(wv2Dir, { recursive: true });

let activeSession = null;
const spawnedFixturePids = new Set();
const steps = [];
const recordStep = (stepNumber, name, details) => {
  steps.push({ step: stepNumber, name, status: 'PASS', details });
};

try {
  // =========================================================================
  // Step 1: Launch real compiled `dim-desktop.exe` (Win32 + WebView2)
  // =========================================================================
  activeSession = await launchTauriInstance({ dataDir, wv2ProfileDir: wv2Dir });
  const firstAppPid = activeSession.proc.pid;
  assert.ok(isPidAlive(firstAppPid), 'dim-desktop.exe Win32 process must be alive');

  const webviewEnv = await activeSession.cdp.evalJs(`({
    hasTauriInternals: Boolean(window.__TAURI_INTERNALS__),
    invokeType: typeof window.__TAURI_INTERNALS__?.invoke,
    origin: window.location.origin,
    title: document.title,
    lang: document.documentElement.lang
  })`);
  assert.equal(webviewEnv.hasTauriInternals, true, 'window.__TAURI_INTERNALS__ must be present');
  assert.equal(webviewEnv.invokeType, 'function', 'window.__TAURI_INTERNALS__.invoke must be a function');
  assert.match(webviewEnv.origin, /tauri\.localhost/i, `Expected real Tauri WebView2 origin, got ${webviewEnv.origin}`);

  recordStep(1, 'Launch compiled dim-desktop.exe Win32 + WebView2 process', {
    binary_kind: tauriBin === releaseBin ? 'release' : 'debug',
    app_pid_alive: true,
    webview_origin: webviewEnv.origin,
    has_tauri_internals: webviewEnv.hasTauriInternals,
    default_lang: webviewEnv.lang
  });

  // =========================================================================
  // Step 2: Verify real Rust Tauri IPC (`get_status`, `get_diagnostics`, `verify_isolation`)
  // =========================================================================
  const ipcCheck = await activeSession.cdp.evalJs(`(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const status = await invoke('get_status', { withSize: true });
    const diagnostics = await invoke('get_diagnostics', {});
    const isolation = await invoke('verify_isolation', {});
    return { status, diagnostics, isolation };
  })()`);

  assert.equal(ipcCheck.status.ok, true);
  assert.equal(ipcCheck.status.platform, 'windows');
  assert.equal(ipcCheck.status.installation.valid, true);
  assert.equal(ipcCheck.status.installation.mode, 'node-fixture');
  assert.equal(ipcCheck.diagnostics.ok, true);
  assert.equal(Array.isArray(ipcCheck.diagnostics.platform_support_matrix), true);
  assert.equal(ipcCheck.isolation.ok, true);
  assert.equal(ipcCheck.isolation.all_passed, true);

  recordStep(2, 'Verify real Tauri IPC (get_status, get_diagnostics, verify_isolation)', {
    platform: ipcCheck.status.platform,
    installation_mode: ipcCheck.status.installation.mode,
    primary_zalo_running: ipcCheck.status.primary_zalo.is_running,
    primary_protected_pid_count: (ipcCheck.status.primary_zalo.locking_pids || []).length,
    isolation_checks_passed: ipcCheck.isolation.checks.map((c) => `${c.check_name}:${c.status}`),
    diagnostics_matrix_rows: ipcCheck.diagnostics.platform_support_matrix.length
  });

  // =========================================================================
  // Step 3: Create profile via Account Dock UI + Tauri IPC
  // =========================================================================
  await activeSession.cdp.evalJs(`(async () => {
    document.getElementById('btn-open-create').click();
    const nameInput = document.getElementById('create-profile-name');
    const notesInput = document.getElementById('create-profile-notes');
    nameInput.value = 'Đức — E2E Tauri Thật';
    nameInput.dispatchEvent(new Event('input', { bubbles: true }));
    notesInput.value = 'Tài khoản kiểm thử Tauri thật';
    notesInput.dispatchEvent(new Event('input', { bubbles: true }));
    const coralRadio = document.querySelector('input[name="create-account-color"][value="coral"]');
    if (coralRadio) {
      coralRadio.checked = true;
      coralRadio.dispatchEvent(new Event('change', { bubbles: true }));
    }
    document.querySelector('#create-profile-form button[type="submit"]').click();
  })()`);

  let createdProfile = null;
  const createDeadline = Date.now() + 10_000;
  while (Date.now() < createDeadline) {
    const check = await activeSession.cdp.evalJs(`(async () => {
      const st = await window.__TAURI_INTERNALS__.invoke('get_status', {});
      const p = st?.profiles?.[0] || null;
      const cardReady = p
        ? Boolean(document.querySelector(\`.account-card[data-profile-id="\${p.id}"] button[aria-haspopup="menu"]:not(:disabled)\`))
        : false;
      const step2Visible = !document.getElementById('create-step-2')?.hidden;
      return { p, cardReady, step2Visible };
    })()`);
    if (check?.p && check.cardReady && check.step2Visible) {
      createdProfile = check.p;
      break;
    }
    await sleep(100);
  }
  assert.ok(createdProfile, 'Profile must be created and rendered in Account Dock via real Tauri IPC');
  assert.equal(createdProfile.name, 'Đức — E2E Tauri Thật');
  assert.match(createdProfile.directory_name, /^duc-e2e-tauri-that_[0-9a-f]{8}$/);
  assert.equal(fs.existsSync(path.join(dataDir, 'profiles.json')), true);

  // Close Step-2 onboarding dialog via the "Later" button so Home cards are interactive
  await activeSession.cdp.evalJs(`(() => {
    const laterBtn = document.getElementById('btn-create-later');
    if (laterBtn) laterBtn.click();
    const dlg = document.getElementById('create-profile-dialog');
    if (dlg && dlg.open) dlg.close();
  })()`);
  await sleep(100);

  recordStep(3, 'Create isolated profile via Account Dock UI', {
    profile_id: createdProfile.id,
    directory_name: createdProfile.directory_name,
    dom_card_rendered: await activeSession.cdp.evalJs(
      `Boolean(document.querySelector('.account-card[data-profile-id="${createdProfile.id}"]'))`
    )
  });

  // =========================================================================
  // Step 4: Rename profile via Account Dock UI & verify directory + color preserved
  // =========================================================================
  await activeSession.cdp.evalJs(`(() => {
    const card = document.querySelector('.account-card[data-profile-id="${createdProfile.id}"]');
    card.querySelector('button[aria-haspopup="menu"]').click();
    card.querySelectorAll('[role="menuitem"]')[0].click();
    const inp = document.getElementById('rename-profile-name');
    inp.value = 'Đức — Tauri Đã Đổi Tên';
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    document.querySelector('#rename-profile-form button[value="rename"]').click();
  })()`);

  let renamedProfile = null;
  const renameDeadline = Date.now() + 10_000;
  while (Date.now() < renameDeadline) {
    const check = await activeSession.cdp.evalJs(`(async () => {
      const st = await window.__TAURI_INTERNALS__.invoke('get_status', {});
      const p = st?.profiles?.[0] || null;
      const launchBtnEnabled = p
        ? Boolean(document.querySelector(\`.account-card[data-profile-id="\${p.id}"] .account-actions-primary button:not(:disabled)\`))
        : false;
      return { p, launchBtnEnabled };
    })()`);
    if (check?.p?.name === 'Đức — Tauri Đã Đổi Tên' && check.launchBtnEnabled) {
      renamedProfile = check.p;
      break;
    }
    await sleep(100);
  }
  assert.ok(renamedProfile, 'Profile rename must persist in profiles.json and re-enable card actions');
  assert.equal(renamedProfile.directory_name, createdProfile.directory_name, 'Rename must preserve directory_name');

  recordStep(4, 'Rename profile and preserve directory_name + identity color', {
    profile_id: renamedProfile.id,
    new_name: renamedProfile.name,
    preserved_directory_name: renamedProfile.directory_name
  });

  // =========================================================================
  // Step 5: Launch isolated fixture instance via Account Dock card button
  // =========================================================================
  await activeSession.cdp.evalJs(`(() => {
    const card = document.querySelector('.account-card[data-profile-id="${createdProfile.id}"]');
    card.querySelector('.account-actions-primary button').click();
  })()`);

  const profileZaloData = path.join(
    dataDir,
    'Profiles',
    createdProfile.directory_name,
    'AppData',
    'Roaming',
    'ZaloData'
  );
  const fixtureStatePath = path.join(profileZaloData, 'fixture-state.json');

  let lastBannerText = '';
  let runningProfile = null;
  const launchDeadline = Date.now() + 14_000;
  while (Date.now() < launchDeadline) {
    const check = await activeSession.cdp.evalJs(`(async () => {
      const st = await window.__TAURI_INTERNALS__.invoke('get_status', {});
      const cardReady = Boolean(
        document.querySelector('.account-card.is-running[data-profile-id="${createdProfile.id}"] .account-actions-primary button:not(:disabled)')
      );
      const bannerText = document.getElementById('status-banner-raw')?.textContent
        || document.getElementById('status-banner-text')?.textContent
        || '';
      return { st, cardReady, bannerText };
    })()`);
    lastBannerText = check?.bannerText || '';
    const p = check?.st?.profiles?.[0];
    if (p?.running && p?.main_pid && check?.cardReady && fs.existsSync(fixtureStatePath)) {
      runningProfile = p;
      break;
    }
    await sleep(120);
  }
  assert.ok(runningProfile, `Fixture instance must be running after launch (banner: ${lastBannerText})`);
  spawnedFixturePids.add(runningProfile.main_pid);
  assert.equal(runningProfile.lifecycle_state, 'running');
  assert.equal(isPidAlive(runningProfile.main_pid), true);

  recordStep(5, 'Launch isolated fixture instance via Tauri coordinator', {
    profile_id: runningProfile.id,
    running: runningProfile.running,
    lifecycle_state: runningProfile.lifecycle_state,
    fixture_pid_alive: true,
    detected_zalo_user_id: runningProfile.zalo_user_id
  });

  // =========================================================================
  // Step 6: Focus running instance via Account Dock card button
  // =========================================================================
  await activeSession.cdp.evalJs(`(() => {
    const card = document.querySelector('.account-card[data-profile-id="${createdProfile.id}"]');
    card.querySelector('.account-actions-primary button:not(:disabled)').click();
  })()`);

  let focusCount = 0;
  const focusDeadline = Date.now() + 8_000;
  while (Date.now() < focusDeadline) {
    const focusReady = await activeSession.cdp.evalJs(
      `Boolean(document.querySelector('.account-card.is-running[data-profile-id="${createdProfile.id}"] .account-actions-secondary button.btn-subtle-danger:not(:disabled)'))`
    );
    if (focusReady && fs.existsSync(fixtureStatePath)) {
      try {
        const fstate = JSON.parse(fs.readFileSync(fixtureStatePath, 'utf8'));
        if (fstate.focus_count >= 1) {
          focusCount = fstate.focus_count;
          break;
        }
      } catch {
        // Retry if mid-atomic-write.
      }
    }
    await sleep(100);
  }
  assert.ok(focusCount >= 1, `Expected focus_count >= 1 in fixture-state.json, got ${focusCount}`);

  recordStep(6, 'Focus running instance and verify fixture focus handoff', {
    profile_id: createdProfile.id,
    focus_count: focusCount
  });

  // =========================================================================
  // Step 7: Stop running instance cleanly via Account Dock Stop button
  // =========================================================================
  await activeSession.cdp.evalJs(`(() => {
    const card = document.querySelector('.account-card[data-profile-id="${createdProfile.id}"]');
    card.querySelector('.account-actions-secondary button.btn-subtle-danger').click();
  })()`);

  let stoppedCleanly = false;
  const stopDeadline = Date.now() + 10_000;
  while (Date.now() < stopDeadline) {
    const st = await activeSession.cdp.evalJs(`window.__TAURI_INTERNALS__.invoke('get_status', {})`);
    const p = st?.profiles?.[0];
    if (p && !p.running && p.lifecycle_state === 'stopped' && !isPidAlive(runningProfile.main_pid)) {
      stoppedCleanly = true;
      break;
    }
    await sleep(120);
  }
  assert.equal(stoppedCleanly, true, 'Fixture process must stop cleanly');
  const postStopFixtureState = JSON.parse(fs.readFileSync(fixtureStatePath, 'utf8'));
  assert.equal(postStopFixtureState.clean_shutdown, true);
  assert.equal(
    fs.existsSync(path.join(dataDir, 'Profiles', createdProfile.directory_name, '.runtime-lease.json')),
    false,
    '.runtime-lease.json must be removed on clean stop'
  );

  recordStep(7, 'Stop running instance cleanly and verify zero orphan PIDs or leases', {
    profile_id: createdProfile.id,
    clean_shutdown: postStopFixtureState.clean_shutdown,
    pid_terminated: !isPidAlive(runningProfile.main_pid),
    runtime_lease_removed: true
  });

  // =========================================================================
  // Step 8: Create offline atomic SHA-256 backup
  // =========================================================================
  const backupRes = await activeSession.cdp.evalJs(`(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const bk = await invoke('backup_profile', { id: '${createdProfile.id}' });
    const list = await invoke('list_backups', {});
    return { bk, list };
  })()`);
  assert.equal(backupRes.bk.ok, true, `backup_profile failed: ${backupRes.bk.message}`);
  assert.equal(backupRes.list.backups.length, 1);
  const backupItem = backupRes.list.backups[0];
  assert.equal(fs.existsSync(path.join(backupItem.backup_dir, 'backup.json')), true);

  recordStep(8, 'Create offline atomic SHA-256 backup via Tauri IPC', {
    backup_id: backupItem.backup_id,
    backup_name: backupItem.backup_name,
    file_count: backupItem.file_count,
    total_bytes: backupItem.total_bytes
  });

  // =========================================================================
  // Step 9: Restore backup as a brand-new isolated profile
  // =========================================================================
  const restoreRes = await activeSession.cdp.evalJs(`window.__TAURI_INTERNALS__.invoke('restore_backup', {
    backupDir: ${JSON.stringify(backupItem.backup_name)},
    name: 'Đức — Khôi Phục E2E'
  })`);
  assert.equal(restoreRes.ok, true, `restore_backup failed: ${restoreRes.message}`);
  const restoredProfile = restoreRes.data;
  assert.notEqual(restoredProfile.id, createdProfile.id);
  assert.equal(restoredProfile.name, 'Đức — Khôi Phục E2E');

  const statusWithTwo = await activeSession.cdp.evalJs(`window.__TAURI_INTERNALS__.invoke('get_status', {})`);
  assert.equal(statusWithTwo.profiles.length, 2);

  recordStep(9, 'Restore verified backup into a new isolated profile', {
    source_profile_id: createdProfile.id,
    restored_profile_id: restoredProfile.id,
    restored_directory_name: restoredProfile.directory_name,
    total_profiles: statusWithTwo.profiles.length
  });

  // =========================================================================
  // Step 10: Delete restored profile via Account Dock confirmation dialog
  // =========================================================================
  await activeSession.cdp.evalJs(`(() => {
    document.getElementById('btn-refresh').click();
  })()`);
  let restoredCardReady = false;
  const restoredCardDeadline = Date.now() + 8_000;
  while (Date.now() < restoredCardDeadline) {
    restoredCardReady = await activeSession.cdp.evalJs(
      `Boolean(document.querySelector('.account-card[data-profile-id="${restoredProfile.id}"] button[aria-haspopup="menu"]:not(:disabled)'))`
    );
    if (restoredCardReady) break;
    await sleep(100);
  }
  assert.equal(restoredCardReady, true, 'Restored profile card must render before delete');
  await activeSession.cdp.evalJs(`(() => {
    const card = document.querySelector('.account-card[data-profile-id="${restoredProfile.id}"]');
    card.querySelector('button[aria-haspopup="menu"]').click();
    card.querySelectorAll('[role="menuitem"]')[3].click();
    document.querySelector('#delete-profile-dialog button[value="delete"]').click();
  })()`);

  const restoredDirOnDisk = path.join(dataDir, 'Profiles', restoredProfile.directory_name);
  let deletedCleanly = false;
  const delDeadline = Date.now() + 8_000;
  while (Date.now() < delDeadline) {
    const st = await activeSession.cdp.evalJs(`window.__TAURI_INTERNALS__.invoke('get_status', {})`);
    if (st?.profiles?.length === 1 && !fs.existsSync(restoredDirOnDisk)) {
      deletedCleanly = true;
      break;
    }
    await sleep(100);
  }
  assert.equal(deletedCleanly, true, 'Deleted profile and its directory must be removed cleanly');
  assert.equal(
    fs.existsSync(path.join(dataDir, 'Profiles', createdProfile.directory_name)),
    true,
    'Primary survivor profile directory must remain intact'
  );

  recordStep(10, 'Delete profile with confirmation and verify directory removal + survivor intact', {
    deleted_profile_id: restoredProfile.id,
    deleted_dir_removed: !fs.existsSync(restoredDirOnDisk),
    survivor_profile_id: createdProfile.id
  });

  // =========================================================================
  // Step 11: Set dark theme + en-US locale, restart `dim-desktop.exe`, verify persistence
  // =========================================================================
  await activeSession.cdp.evalJs(`(() => {
    document.querySelector('.top-nav-btn[data-view="settings"]').click();
    const darkRadio = document.querySelector('input[name="ui-theme"][value="dark"]');
    darkRadio.checked = true;
    darkRadio.dispatchEvent(new Event('change', { bubbles: true }));
    const enRadio = document.querySelector('input[name="ui-locale"][value="en-US"]');
    enRadio.checked = true;
    enRadio.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await sleep(200);

  assert.deepEqual(
    activeSession.cdp.runtimeErrors,
    [],
    `Unexpected WebView2 runtime errors before restart: ${activeSession.cdp.runtimeErrors.join('; ')}`
  );
  activeSession.cdp.ws.close();
  await terminateProcessCleanly(activeSession.proc);
  activeSession = null;

  // Launch fresh 2nd `dim-desktop.exe` process on the SAME `DIM_DATA_DIR` & `WEBVIEW2_USER_DATA_FOLDER`
  activeSession = await launchTauriInstance({ dataDir, wv2ProfileDir: wv2Dir });
  const secondAppPid = activeSession.proc.pid;
  assert.notEqual(secondAppPid, firstAppPid, 'Restarted dim-desktop.exe must be a new OS process');
  const restartDomDeadline = Date.now() + 8_000;
  while (Date.now() < restartDomDeadline) {
    const hasCard = await activeSession.cdp.evalJs(
      `Boolean(document.querySelector('.account-card[data-profile-id="${createdProfile.id}"]'))`
    );
    if (hasCard) break;
    await sleep(100);
  }

  const persistedAfterRestart = await activeSession.cdp.evalJs(`(async () => {
    const st = await window.__TAURI_INTERNALS__.invoke('get_status', {});
    const bks = await window.__TAURI_INTERNALS__.invoke('list_backups', {});
    const card = document.querySelector('.account-card[data-profile-id="${createdProfile.id}"]');
    const avatarVar = card?.querySelector('.account-avatar')?.style.getPropertyValue('--avatar-color') || '';
    return {
      themeAttr: document.documentElement.getAttribute('data-theme'),
      langAttr: document.documentElement.lang,
      storedColors: localStorage.getItem('multi-zalo.accountColors'),
      avatarVar,
      profileCount: st.profiles.length,
      profileName: st.profiles[0]?.name,
      zaloUserId: st.profiles[0]?.zalo_user_id,
      backupCount: bks.backups.length
    };
  })()`);

  assert.equal(persistedAfterRestart.themeAttr, 'dark');
  assert.equal(persistedAfterRestart.langAttr, 'en');
  assert.equal(persistedAfterRestart.profileCount, 1);
  assert.equal(persistedAfterRestart.profileName, 'Đức — Tauri Đã Đổi Tên');
  assert.match(persistedAfterRestart.avatarVar, /--id-coral/);
  assert.equal(persistedAfterRestart.backupCount, 1);

  recordStep(11, 'Restart dim-desktop.exe and verify profile, UID, color, theme, and locale persistence', {
    restarted_new_process: secondAppPid !== firstAppPid,
    persisted_theme: persistedAfterRestart.themeAttr,
    persisted_lang: persistedAfterRestart.langAttr,
    persisted_avatar_token: persistedAfterRestart.avatarVar,
    persisted_profile_name: persistedAfterRestart.profileName,
    persisted_zalo_user_id: persistedAfterRestart.zaloUserId,
    persisted_backup_count: persistedAfterRestart.backupCount
  });

  // =========================================================================
  // Step 12: Verify error handling (`DUPLICATE_NAME`, `INVALID_NAME`, `PROFILE_RUNNING` on delete)
  // =========================================================================
  const errorChecks = await activeSession.cdp.evalJs(`(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const dupRes = await invoke('create_profile', { name: 'đức — tauri đã đổi tên', notes: '' });
    const invalidEmptyRes = await invoke('create_profile', { name: '   ', notes: '' });
    const invalidTooLongRes = await invoke('create_profile', { name: 'A'.repeat(70), notes: '' });
    const launchRes = await invoke('launch_profile', { id: '${createdProfile.id}' });
    const delWhileRunningRes = await invoke('delete_profile', { id: '${createdProfile.id}', confirm: true });
    const delUnconfirmedRes = await invoke('delete_profile', { id: '${createdProfile.id}', confirm: false });
    const stopAllRes = await invoke('stop_all_profiles', {});
    return {
      dupCode: dupRes.error_code,
      invalidEmptyCode: invalidEmptyRes.error_code,
      invalidTooLongCode: invalidTooLongRes.error_code,
      launchedPid: launchRes.data?.main_pid,
      delWhileRunningCode: delWhileRunningRes.error_code,
      delUnconfirmedCode: delUnconfirmedRes.error_code,
      stopOk: stopAllRes.ok,
      stoppedProfilesCount: stopAllRes.data?.stopped_profiles_count
    };
  })()`);

  if (errorChecks.launchedPid) spawnedFixturePids.add(errorChecks.launchedPid);
  assert.equal(errorChecks.dupCode, 'DUPLICATE_NAME');
  assert.equal(errorChecks.invalidEmptyCode, 'INVALID_NAME');
  assert.equal(errorChecks.invalidTooLongCode, 'INVALID_NAME');
  assert.equal(errorChecks.delWhileRunningCode, 'PROFILE_RUNNING');
  assert.equal(errorChecks.delUnconfirmedCode, 'CONFIRM_REQUIRED');
  assert.equal(errorChecks.stopOk, true);
  assert.equal(errorChecks.stoppedProfilesCount, 1);
  assert.equal(
    fs.existsSync(path.join(dataDir, 'Profiles', createdProfile.directory_name)),
    true,
    'Blocked running-profile delete must leave profile directory untouched'
  );
  assert.deepEqual(
    activeSession.cdp.runtimeErrors,
    [],
    `Unexpected WebView2 runtime errors: ${activeSession.cdp.runtimeErrors.join('; ')}`
  );

  recordStep(12, 'Verify Tauri IPC error guards (DUPLICATE_NAME, INVALID_NAME, PROFILE_RUNNING, CONFIRM_REQUIRED, stop_all_profiles)', {
    duplicate_unicode_name_code: errorChecks.dupCode,
    invalid_empty_name_code: errorChecks.invalidEmptyCode,
    invalid_too_long_name_code: errorChecks.invalidTooLongCode,
    delete_while_running_code: errorChecks.delWhileRunningCode,
    delete_without_confirm_code: errorChecks.delUnconfirmedCode,
    stop_all_profiles_ok: errorChecks.stopOk,
    stopped_profiles_count: errorChecks.stoppedProfilesCount,
    webview_runtime_errors: 0
  });

  const rawReport = {
    ok: true,
    executed_at_utc: new Date().toISOString(),
    platform: process.platform,
    node_version: process.version,
    tauri_binary: path.relative(dimRoot, tauriBin).replace(/\\/g, '/'),
    fixture_script: path.relative(dimRoot, fixtureScript).replace(/\\/g, '/'),
    isolated_data_dir: '<TEMP_E2E_ROOT>/dim-data',
    real_tauri_ipc_verified: true,
    total_steps: 12,
    passed_steps: steps.length,
    failed_steps: 0,
    steps
  };

  const sanitizedReport = sanitizePaths(rawReport, [
    [tempRoot, '<TEMP_E2E_ROOT>'],
    [dimRoot, '<DIM_ROOT>'],
    [process.env.USERPROFILE || '', '<USERPROFILE>']
  ]);

  fs.mkdirSync(path.dirname(evidenceOutPath), { recursive: true });
  fs.writeFileSync(evidenceOutPath, `${JSON.stringify(sanitizedReport, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify(sanitizedReport, null, 2));
} finally {
  if (activeSession) {
    try {
      activeSession.cdp.ws.close();
    } catch {
      // Ignore.
    }
    await terminateProcessCleanly(activeSession.proc);
  }
  for (const pid of spawnedFixturePids) {
    if (isPidAlive(pid)) {
      try {
        process.kill(pid);
      } catch {
        // Ignore.
      }
    }
  }
  await sleep(250);
  try {
    fs.rmSync(tempRoot, { recursive: true, force: true, maxRetries: 5, retryDelay: 150 });
  } catch {
    // Ignore transient WebView2 log handle release delay on Windows temp dir.
  }
}
