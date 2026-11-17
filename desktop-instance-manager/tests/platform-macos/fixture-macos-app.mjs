#!/usr/bin/env node
/**
 * macOS-compatible multi-instance desktop test fixture (`fixture-macos-app.mjs`).
 *
 * Simulates Zalo.app / Electron / Chromium runtime behavior under `CFFIXED_USER_HOME`, `HOME`,
 * `TMPDIR`, `MAC_CHROMIUM_TMPDIR`, and `--user-data-dir` with ZERO real Zalo data or Keychain access:
 *   - Writes synthetic profile-tagged files across all non-Keychain macOS isolation surfaces:
 *     Application Support, ZaloData, Session Storage, Chromium Caches, Local Storage, Cookies,
 *     Preferences plist, HTTPStorages, Crashpad/Logs, Downloads, and SUN_LEN-safe TMPDIR socket
 *   - Supports child renderer helper (`--spawn-helper`) AND `ppid == 1` orphan helper (`--orphan-helper`)
 *   - Supports slow startup pre-lock (`--slow-startup-pre-lock-ms`), cooperative delayed exit
 *     (`--delayed-exit-ms`), hung process ignoring quit/SIGTERM (`--hang-on-quit`), and `--crash`
 */

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const MACOS_SOCKET_MAX_BYTES = 103; // Darwin sockaddr_un.sun_path[104] minus NUL

function parseArgs(argv) {
  const opts = {
    helperType: null,
    spawnHelper: false,
    orphanHelper: false,
    crash: false,
    crashDelayMs: 120,
    delayedExitMs: 0,
    hangOnQuit: false,
    slowStartupPreLockMs: 0,
    stayMs: null,
    userDataDir: null,
    diskCacheDir: null,
    userId: process.env.FIXTURE_ZALO_USER_ID || null,
    accountLabel: process.env.FIXTURE_ACCOUNT_LABEL || null,
    enforceSunLen: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg.startsWith('--type=')) {
      opts.helperType = arg.slice('--type='.length);
    } else if (arg === '--spawn-helper') {
      opts.spawnHelper = true;
    } else if (arg === '--orphan-helper') {
      opts.orphanHelper = true;
    } else if (arg === '--enforce-sun-len') {
      opts.enforceSunLen = true;
    } else if (arg === '--crash') {
      opts.crash = true;
    } else if (arg === '--hang-on-quit') {
      opts.hangOnQuit = true;
    } else if (arg === '--crash-delay-ms' && i + 1 < argv.length) {
      opts.crashDelayMs = Math.max(10, Number.parseInt(argv[++i], 10) || 120);
    } else if (arg.startsWith('--crash-delay-ms=')) {
      opts.crashDelayMs = Math.max(10, Number.parseInt(arg.slice('--crash-delay-ms='.length), 10) || 120);
    } else if (arg === '--delayed-exit-ms' && i + 1 < argv.length) {
      opts.delayedExitMs = Math.max(0, Number.parseInt(argv[++i], 10) || 0);
    } else if (arg.startsWith('--delayed-exit-ms=')) {
      opts.delayedExitMs = Math.max(0, Number.parseInt(arg.slice('--delayed-exit-ms='.length), 10) || 0);
    } else if (arg === '--slow-startup-pre-lock-ms' && i + 1 < argv.length) {
      opts.slowStartupPreLockMs = Math.max(0, Number.parseInt(argv[++i], 10) || 0);
    } else if (arg.startsWith('--slow-startup-pre-lock-ms=')) {
      opts.slowStartupPreLockMs = Math.max(
        0,
        Number.parseInt(arg.slice('--slow-startup-pre-lock-ms='.length), 10) || 0,
      );
    } else if (arg === '--stay-ms' && i + 1 < argv.length) {
      const n = Number.parseInt(argv[++i], 10);
      if (Number.isFinite(n) && n > 0) opts.stayMs = n;
    } else if (arg.startsWith('--stay-ms=')) {
      const n = Number.parseInt(arg.slice('--stay-ms='.length), 10);
      if (Number.isFinite(n) && n > 0) opts.stayMs = n;
    } else if (arg === '--user-data-dir' && i + 1 < argv.length) {
      opts.userDataDir = argv[++i];
    } else if (arg.startsWith('--user-data-dir=')) {
      opts.userDataDir = arg.slice('--user-data-dir='.length);
    } else if (arg === '--disk-cache-dir' && i + 1 < argv.length) {
      opts.diskCacheDir = argv[++i];
    } else if (arg.startsWith('--disk-cache-dir=')) {
      opts.diskCacheDir = arg.slice('--disk-cache-dir='.length);
    } else if (arg === '--user-id' && i + 1 < argv.length) {
      opts.userId = argv[++i];
    } else if (arg.startsWith('--user-id=')) {
      opts.userId = arg.slice('--user-id='.length);
    } else if (arg === '--account-label' && i + 1 < argv.length) {
      opts.accountLabel = argv[++i];
    } else if (arg.startsWith('--account-label=')) {
      opts.accountLabel = arg.slice('--account-label='.length);
    }
  }
  return opts;
}

function sleepMs(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function resolveProfileHome() {
  if (process.env.CFFIXED_USER_HOME && process.env.CFFIXED_USER_HOME.trim().length > 0) {
    return process.env.CFFIXED_USER_HOME.trim();
  }
  if (process.env.HOME && process.env.HOME.trim().length > 0) {
    return process.env.HOME.trim();
  }
  return os.homedir();
}

function resolveEffectiveTmpDir(profileHome) {
  if (process.env.MAC_CHROMIUM_TMPDIR && process.env.MAC_CHROMIUM_TMPDIR.trim().length > 0) {
    return process.env.MAC_CHROMIUM_TMPDIR.trim();
  }
  if (process.env.TMPDIR && process.env.TMPDIR.trim().length > 0) {
    return process.env.TMPDIR.trim();
  }
  return path.join(profileHome, 'tmp');
}

function isPidAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return Boolean(err && err.code === 'EPERM');
  }
}

function readJsonSafe(filePath, fallback) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch {
    return fallback;
  }
}

function writeJsonAtomic(filePath, value) {
  const tmpPath = `${filePath}.tmp.${process.pid}.${Date.now()}`;
  fs.writeFileSync(tmpPath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
  fs.renameSync(tmpPath, filePath);
}

function tryCreateSymlink(target, linkPath) {
  try {
    try {
      if (fs.lstatSync(linkPath)) fs.unlinkSync(linkPath);
    } catch {
      // Does not exist yet.
    }
    fs.symlinkSync(target, linkPath);
    return true;
  } catch {
    try {
      fs.writeFileSync(linkPath, `${target}\n`, 'utf8');
    } catch {
      // Ignore fallback error.
    }
    return false;
  }
}

function removeNoFollow(filePath) {
  try {
    const st = fs.lstatSync(filePath);
    if (st.isDirectory() && !st.isSymbolicLink()) return;
    fs.unlinkSync(filePath);
  } catch {
    // Absent.
  }
}

function deriveNumericUserId(profileId, explicitUserId) {
  if (explicitUserId && /^\d{6,}$/.test(explicitUserId)) {
    return explicitUserId;
  }
  if (profileId && /^[0-9a-z_-]{4,}$/i.test(profileId)) {
    let hash = 0x811c9dc5;
    const norm = profileId.trim().toLowerCase();
    for (let i = 0; i < norm.length; i += 1) {
      hash ^= norm.charCodeAt(i);
      hash = Math.imul(hash, 0x01000193) >>> 0;
    }
    const suffix = String(hash % 10_000_000).padStart(7, '0');
    return `84${suffix}`;
  }
  // Never fall back to a constant `'100001'` that would cause cross-profile account collisions (`DEFECT-02`).
  return null;
}

function runRendererHelper(helperType) {
  const isOrphanMode = helperType === 'orphan-renderer';
  const parentPid = process.ppid;
  // Safety cap so an uncleaned orphan in a broken test can never linger forever.
  const safetyCap = setTimeout(() => process.exit(0), 12_000);
  safetyCap.unref?.();

  let timer = null;
  if (!isOrphanMode) {
    timer = setInterval(() => {
      if (parentPid > 1 && !isPidAlive(parentPid)) {
        clearInterval(timer);
        process.exit(0);
      }
    }, 100);
  } else {
    // Keep event loop open until SIGTERM arrives (simulating `ppid == 1` orphaned helper).
    timer = setInterval(() => {}, 250);
  }

  process.on('SIGTERM', () => {
    if (timer) clearInterval(timer);
    process.exit(0);
  });
  process.on('SIGINT', () => {
    if (timer) clearInterval(timer);
    process.exit(0);
  });
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.helperType) {
    runRendererHelper(opts.helperType);
    return;
  }

  const profileHome = resolveProfileHome();
  const libraryDir = path.join(profileHome, 'Library');
  const appSupportDir = path.join(libraryDir, 'Application Support');
  const zaloAppMetaDir = path.join(appSupportDir, 'Zalo');
  const zaloDataDir = opts.userDataDir || path.join(appSupportDir, 'ZaloData');
  const sessionStorageDir = path.join(zaloDataDir, 'Session Storage');
  const localStorageDir = path.join(zaloDataDir, 'Local Storage', 'leveldb');
  const networkCookiesDir = path.join(zaloDataDir, 'Network');
  const crashpadDir = path.join(zaloDataDir, 'Crashpad');
  const prefsDir = path.join(libraryDir, 'Preferences');
  const httpStoragesDir = path.join(libraryDir, 'HTTPStorages', 'com.vng.zalo');
  const logsDiagDir = path.join(libraryDir, 'Logs', 'DiagnosticReports');
  const cachesDir = opts.diskCacheDir || path.join(libraryDir, 'Caches', 'com.vng.zalo');
  const downloadsDir = path.join(profileHome, 'Downloads');
  const effectiveTmpDir = resolveEffectiveTmpDir(profileHome);

  for (const dir of [
    zaloAppMetaDir,
    zaloDataDir,
    sessionStorageDir,
    localStorageDir,
    networkCookiesDir,
    crashpadDir,
    prefsDir,
    httpStoragesDir,
    logsDiagDir,
    cachesDir,
    effectiveTmpDir,
  ]) {
    fs.mkdirSync(dir, { recursive: true });
  }

  // Optional slow pre-lock startup delay (`DEFECT-03` / `LC-07`).
  if (opts.slowStartupPreLockMs > 0) {
    await sleepMs(opts.slowStartupPreLockMs);
  }

  const suffixTag = String(process.pid).padStart(6, '0').slice(-6);
  const chromiumSocketDir = path.join(effectiveTmpDir, `.org.chromium.Chromium.${suffixTag}`);
  const chromiumSocketPath = path.join(chromiumSocketDir, 'SingletonSocket');
  const tmpLeaf = path.basename(effectiveTmpDir);
  const parentLeaf = path.basename(path.dirname(effectiveTmpDir));
  let darwinEquivalentSocketPath = chromiumSocketPath.replace(/\\/g, '/');
  if (process.platform === 'win32') {
    if (/^dz-\d+$/i.test(parentLeaf) && /^[0-9a-z_-]+$/i.test(tmpLeaf)) {
      darwinEquivalentSocketPath = `/tmp/${parentLeaf}/${tmpLeaf}/.org.chromium.Chromium.${suffixTag}/SingletonSocket`;
    } else if (/^dim-zalo-\d+-[0-9a-z-]+$/i.test(tmpLeaf)) {
      darwinEquivalentSocketPath = `/tmp/${tmpLeaf}/.org.chromium.Chromium.${suffixTag}/SingletonSocket`;
    }
  }
  const socketUtf8Bytes = Buffer.byteLength(darwinEquivalentSocketPath, 'utf8');

  if (opts.enforceSunLen && socketUtf8Bytes > MACOS_SOCKET_MAX_BYTES) {
    process.stderr.write(
      `SUN_LEN_OVERFLOW: SingletonSocket path (${socketUtf8Bytes} bytes) > ${MACOS_SOCKET_MAX_BYTES}: ${darwinEquivalentSocketPath}\n`,
    );
    process.exit(3);
  }

  fs.mkdirSync(chromiumSocketDir, { recursive: true });
  fs.writeFileSync(chromiumSocketPath, `socket-pid=${process.pid}\n`, 'utf8');

  const lockfilePath = path.join(zaloDataDir, 'lockfile');
  const singletonLockPath = path.join(zaloDataDir, 'SingletonLock');
  const singletonSocketLink = path.join(zaloDataDir, 'SingletonSocket');
  const singletonCookieLink = path.join(zaloDataDir, 'SingletonCookie');
  const focusSignalPath = path.join(zaloDataDir, '.focus-signal');
  const quitSignalPath = path.join(zaloDataDir, '.quit-signal');
  const windowStatePath = path.join(zaloDataDir, '.window-state.json');
  const helperPidsPath = path.join(zaloDataDir, '.helper-pids.json');
  const launchStatePath = path.join(zaloDataDir, 'launch-state.json');

  if (fs.existsSync(lockfilePath)) {
    const existing = readJsonSafe(lockfilePath, null);
    if (existing && Number.isInteger(existing.pid) && existing.pid !== process.pid && isPidAlive(existing.pid)) {
      writeJsonAtomic(focusSignalPath, {
        focused_at: new Date().toISOString(),
        requester_pid: process.pid,
      });
      process.exit(2);
    }
    removeNoFollow(lockfilePath);
  }

  const profileId = process.env.DIM_PROFILE_ID || null;
  const userId = deriveNumericUserId(profileId, opts.userId);
  const tag = profileId || userId || `pid-${process.pid}`;

  writeJsonAtomic(lockfilePath, {
    pid: process.pid,
    started_at: new Date().toISOString(),
    app: 'dim-macos-fixture',
    profile_id: profileId,
    effective_tmpdir: effectiveTmpDir,
    socket_path_bytes: socketUtf8Bytes,
  });

  removeNoFollow(focusSignalPath);
  removeNoFollow(quitSignalPath);

  // Write synthetic profile-specific runtime markers across all isolated storage surfaces (`P0-2`).
  writeJsonAtomic(path.join(zaloAppMetaDir, 'zmeta-runtime.json'), {
    profile_id: profileId,
    pid: process.pid,
    surface: 'application_support',
  });
  writeJsonAtomic(path.join(sessionStorageDir, 'session-marker.json'), {
    profile_id: profileId,
    zalo_user_id: userId,
    session_token_synthetic: `sess-${tag}-${process.pid}`,
  });
  writeJsonAtomic(path.join(localStorageDir, 'ls-marker.json'), {
    profile_id: profileId,
    origin: 'app://zalo',
    key_prefix: `ls-${tag}`,
  });
  writeJsonAtomic(path.join(networkCookiesDir, 'cookies-synthetic.json'), {
    profile_id: profileId,
    cookie_jar: `jar-${tag}`,
  });
  writeJsonAtomic(path.join(crashpadDir, 'crashpad-meta.json'), {
    profile_id: profileId,
    pid: process.pid,
  });
  writeJsonAtomic(path.join(logsDiagDir, 'diag-runtime.json'), {
    profile_id: profileId,
    pid: process.pid,
  });
  writeJsonAtomic(path.join(httpStoragesDir, 'http-storage.json'), {
    profile_id: profileId,
    hsts_partition: `http-${tag}`,
  });
  writeJsonAtomic(path.join(effectiveTmpDir, 'profile-tmp-marker.json'), {
    profile_id: profileId,
    pid: process.pid,
    tmpdir: effectiveTmpDir,
  });

  const prefPlistPath = path.join(prefsDir, 'com.vng.zalo.plist');
  fs.writeFileSync(
    prefPlistPath,
    `<?xml version="1.0" encoding="UTF-8"?>\n<plist version="1.0"><dict><key>ProfileId</key><string>${profileId || 'unmanaged'}</string><key>ZaloUserId</key><string>${userId || 'none'}</string></dict></plist>\n`,
    'utf8',
  );
  writeJsonAtomic(path.join(cachesDir, 'cache-meta.json'), {
    profile_id: profileId,
    zalo_user_id: userId,
    pid: process.pid,
    tmpdir: effectiveTmpDir,
    socket_path: chromiumSocketPath,
    socket_path_bytes: socketUtf8Bytes,
  });

  try {
    if (!fs.existsSync(downloadsDir)) {
      fs.mkdirSync(downloadsDir, { recursive: true });
    }
    const dlStat = fs.lstatSync(downloadsDir);
    if (dlStat.isDirectory() && !dlStat.isSymbolicLink()) {
      fs.writeFileSync(
        path.join(downloadsDir, `received-${tag}.txt`),
        `synthetic-download-owner=${tag}\n`,
        'utf8',
      );
    }
  } catch {
    // Ignore if simulated read-only test.
  }

  let dbWalPath = null;
  if (userId) {
    const prodUserDir = path.join(zaloDataDir, 'Database', '_production', userId);
    fs.mkdirSync(prodUserDir, { recursive: true });
    dbWalPath = path.join(prodUserDir, `ftsm_${userId}.db-wal`);
    fs.writeFileSync(dbWalPath, `active-wal-pid-${process.pid}\n`, 'utf8');
    writeJsonAtomic(path.join(prodUserDir, 'session.json'), {
      zalo_user_id: userId,
      profile_id: profileId,
      pid: process.pid,
      account_label: opts.accountLabel,
    });
  }

  const helperRecords = [];
  let helperChild = null;
  if (opts.spawnHelper) {
    helperChild = spawn(
      process.execPath,
      [__filename, '--type=renderer', `--user-data-dir=${zaloDataDir}`],
      {
        stdio: 'ignore',
        env: process.env,
        windowsHide: true,
      },
    );
    if (helperChild.pid) {
      helperRecords.push({ pid: helperChild.pid, ppid: process.pid, mode: 'child' });
    }
  }

  let orphanChild = null;
  if (opts.orphanHelper) {
    orphanChild = spawn(
      process.execPath,
      [__filename, '--type=orphan-renderer', `--user-data-dir=${zaloDataDir}`],
      {
        detached: true,
        stdio: 'ignore',
        env: process.env,
        windowsHide: true,
      },
    );
    orphanChild.unref();
    if (orphanChild.pid) {
      helperRecords.push({ pid: orphanChild.pid, ppid: 1, mode: 'orphan' });
    }
  }

  if (helperRecords.length > 0) {
    writeJsonAtomic(helperPidsPath, helperRecords);
  }

  const windowId = 70_000 + (process.pid % 20_000);
  const windowTitle = opts.accountLabel
    ? `Zalo — ${opts.accountLabel}`
    : `Zalo (${profileId || userId || process.pid})`;
  writeJsonAtomic(windowStatePath, {
    window_id: windowId,
    owner_pid: process.pid,
    helper_pid: helperChild ? helperChild.pid : null,
    orphan_helper_pid: orphanChild ? orphanChild.pid : null,
    title: windowTitle,
    visible: true,
    focus_count: 0,
  });

  const prev = readJsonSafe(launchStatePath, {});
  const launchState = {
    launch_count: (Number.isInteger(prev.launch_count) ? prev.launch_count : 0) + 1,
    focus_count: Number.isInteger(prev.focus_count) ? prev.focus_count : 0,
    last_pid: process.pid,
    helper_pid: helperChild ? helperChild.pid : null,
    orphan_helper_pid: orphanChild ? orphanChild.pid : null,
    symlink_singleton_lock: false,
    effective_tmpdir: effectiveTmpDir,
    socket_path: chromiumSocketPath,
    socket_path_bytes: socketUtf8Bytes,
    zalo_user_id: userId,
    window_id: windowId,
    window_title: windowTitle,
    clean_shutdown: false,
    exit_reason: null,
    crashed: false,
  };
  const hostTarget = `${os.hostname()}-${process.pid}`;
  tryCreateSymlink(chromiumSocketPath, singletonSocketLink);
  const symlinkCookie = tryCreateSymlink(`cookie-${process.pid}`, singletonCookieLink);
  launchState.symlink_singleton_lock = symlinkCookie;
  writeJsonAtomic(launchStatePath, launchState);
  tryCreateSymlink(hostTarget, singletonLockPath);

  let shuttingDown = false;
  let delayedExitScheduled = false;
  let pollTimer = null;

  function cleanShutdown(code = 0, reason = 'clean') {
    if (shuttingDown) return;
    shuttingDown = true;
    if (pollTimer) clearInterval(pollTimer);

    // Note: `orphanChild` (`--orphan-helper`) is intentionally NOT killed by parent exit so
    // `stop_isolated_profile_tree` must reap the `ppid == 1` orphan via `SIGTERM`!
    if (helperChild && helperChild.pid) {
      try {
        helperChild.kill('SIGTERM');
      } catch {
        // Already exited.
      }
    }

    if (dbWalPath) {
      removeNoFollow(dbWalPath);
    }

    const latest = readJsonSafe(launchStatePath, launchState);
    latest.clean_shutdown = code === 0;
    latest.exit_reason = reason;
    latest.last_stopped_at = new Date().toISOString();
    try {
      writeJsonAtomic(launchStatePath, latest);
    } catch {
      // Ignore.
    }

    for (const f of [
      lockfilePath,
      singletonLockPath,
      singletonSocketLink,
      singletonCookieLink,
      windowStatePath,
      quitSignalPath,
      focusSignalPath,
      chromiumSocketPath,
    ]) {
      removeNoFollow(f);
    }
    try {
      fs.rmdirSync(chromiumSocketDir);
    } catch {
      // Non-empty or already removed.
    }
    process.exit(code);
  }

  if (opts.crash) {
    setTimeout(() => {
      if (helperChild && helperChild.pid) {
        try {
          helperChild.kill('SIGKILL');
        } catch {
          // Ignore.
        }
      }
      const latest = readJsonSafe(launchStatePath, launchState);
      latest.crashed = true;
      latest.clean_shutdown = false;
      latest.exit_reason = 'crash';
      try {
        writeJsonAtomic(launchStatePath, latest);
      } catch {
        // Ignore.
      }
      process.exit(1);
    }, opts.crashDelayMs);
    return;
  }

  const effectiveStayMs = opts.stayMs !== null ? opts.stayMs : opts.hangOnQuit ? 2_500 : null;
  if (effectiveStayMs !== null) {
    setTimeout(() => cleanShutdown(0, 'stay_timeout'), effectiveStayMs);
  }

  process.on('SIGTERM', () => {
    if (opts.hangOnQuit) {
      return;
    }
    cleanShutdown(0, 'sigterm');
  });
  process.on('SIGINT', () => cleanShutdown(0, 'sigint'));

  pollTimer = setInterval(() => {
    if (fs.existsSync(focusSignalPath)) {
      removeNoFollow(focusSignalPath);
      const latest = readJsonSafe(launchStatePath, launchState);
      latest.focus_count = (Number.isInteger(latest.focus_count) ? latest.focus_count : 0) + 1;
      latest.last_focused_at = new Date().toISOString();
      try {
        writeJsonAtomic(launchStatePath, latest);
        const win = readJsonSafe(windowStatePath, {});
        win.focus_count = latest.focus_count;
        win.visible = true;
        writeJsonAtomic(windowStatePath, win);
      } catch {
        // Ignore.
      }
    }

    if (fs.existsSync(quitSignalPath)) {
      if (opts.hangOnQuit) {
        return;
      }
      removeNoFollow(quitSignalPath);
      if (opts.delayedExitMs > 0) {
        if (!delayedExitScheduled) {
          delayedExitScheduled = true;
          setTimeout(() => cleanShutdown(0, 'graceful_quit_signal'), opts.delayedExitMs);
        }
      } else {
        cleanShutdown(0, 'graceful_quit_signal');
      }
    }
  }, 40);
}

main().catch((err) => {
  process.stderr.write(`fixture-macos-app fatal error: ${err && err.stack ? err.stack : err}\n`);
  process.exit(1);
});
