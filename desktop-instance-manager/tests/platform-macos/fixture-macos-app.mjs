#!/usr/bin/env node
/**
 * macOS-compatible multi-instance desktop test fixture (`fixture-macos-app.mjs`).
 *
 * Simulates Zalo.app / Electron / Chromium runtime behavior under `CFFIXED_USER_HOME`, `HOME`,
 * `TMPDIR`, and `--user-data-dir` with zero real Zalo data:
 *   - Isolated `Library/Application Support/ZaloData` (`SingletonLock`, `SingletonSocket`, `SingletonCookie`, `lockfile`)
 *   - Isolated `Library/Preferences/com.vng.zalo.plist` & `Library/Caches/com.vng.zalo/cache-meta.json`
 *   - `TMPDIR` Chromium socket path (`<TMPDIR>/.org.chromium.Chromium.<pid>/SingletonSocket`) with `SUN_LEN` (103B) telemetry
 *   - Optional child helper process (`--spawn-helper` -> `--type=renderer --user-data-dir=<zaloDataDir>`)
 *   - `.window-state.json`, `.focus-signal`, `.quit-signal`, and `--crash` stale-lock simulation
 */

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const MACOS_SOCKET_MAX_BYTES = 103; // sockaddr_un.sun_path[104] minus NUL

function parseArgs(argv) {
  const opts = {
    helperType: null,
    spawnHelper: false,
    crash: false,
    crashDelayMs: 120,
    stayMs: null,
    userDataDir: null,
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
    } else if (arg === '--enforce-sun-len') {
      opts.enforceSunLen = true;
    } else if (arg === '--crash') {
      opts.crash = true;
    } else if (arg === '--crash-delay-ms' && i + 1 < argv.length) {
      opts.crashDelayMs = Math.max(10, Number.parseInt(argv[++i], 10) || 120);
    } else if (arg.startsWith('--crash-delay-ms=')) {
      opts.crashDelayMs = Math.max(10, Number.parseInt(arg.slice('--crash-delay-ms='.length), 10) || 120);
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
  const tmpPath = `${filePath}.tmp.${process.pid}`;
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
    // On Windows without Developer Mode symlink privilege, record target text deterministically.
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
  if (profileId && /^[0-9a-f]{6,}$/i.test(profileId)) {
    const num = Number.parseInt(profileId.slice(0, 7), 16) % 8_000_000;
    return String(1_000_000 + num);
  }
  return '100001';
}

function runRendererHelper() {
  // Child helper stays alive until parent exits or SIGTERM arrives.
  const parentPid = process.ppid;
  const timer = setInterval(() => {
    if (parentPid > 1 && !isPidAlive(parentPid)) {
      clearInterval(timer);
      process.exit(0);
    }
  }, 100);
  process.on('SIGTERM', () => {
    clearInterval(timer);
    process.exit(0);
  });
  process.on('SIGINT', () => {
    clearInterval(timer);
    process.exit(0);
  });
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.helperType) {
    runRendererHelper();
    return;
  }

  const profileHome = resolveProfileHome();
  const libraryDir = path.join(profileHome, 'Library');
  const appSupportDir = path.join(libraryDir, 'Application Support');
  // Mirror Zalo's `migration.js`: `app.getPath("appData")` (`<CFFIXED_USER_HOME>/Library/Application Support`) + `ZaloData`.
  const zaloDataDir = path.join(appSupportDir, 'ZaloData');
  const prefsDir = path.join(libraryDir, 'Preferences');
  const cachesDir = path.join(libraryDir, 'Caches', 'com.vng.zalo');
  const effectiveTmpDir = resolveEffectiveTmpDir(profileHome);

  for (const dir of [zaloDataDir, prefsDir, cachesDir, effectiveTmpDir]) {
    fs.mkdirSync(dir, { recursive: true });
  }

  // Simulate Chromium's `<TMPDIR>/.org.chromium.Chromium.<6chars>/SingletonSocket` path check.
  const suffixTag = String(process.pid).padStart(6, '0').slice(-6);
  const chromiumSocketDir = path.join(effectiveTmpDir, `.org.chromium.Chromium.${suffixTag}`);
  const chromiumSocketPath = path.join(chromiumSocketDir, 'SingletonSocket');
  const tmpLeaf = path.basename(effectiveTmpDir);
  const darwinEquivalentSocketPath =
    process.platform === 'win32' && /^dim-zalo-\d+-[0-9a-z-]+$/i.test(tmpLeaf)
      ? `/tmp/${tmpLeaf}/.org.chromium.Chromium.${suffixTag}/SingletonSocket`
      : chromiumSocketPath.replace(/\\/g, '/');
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
  const launchStatePath = path.join(zaloDataDir, 'launch-state.json');

  // Single-instance check within this isolated ZaloData directory.
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

  // Write isolated Preferences plist & Caches marker before publishing SingletonLock readiness.
  const prefPlistPath = path.join(prefsDir, 'com.vng.zalo.plist');
  fs.writeFileSync(
    prefPlistPath,
    `<?xml version="1.0" encoding="UTF-8"?>\n<plist version="1.0"><dict><key>ProfileId</key><string>${profileId || 'primary'}</string><key>ZaloUserId</key><string>${userId}</string></dict></plist>\n`,
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

  const prodUserDir = path.join(zaloDataDir, 'Database', '_production', userId);
  fs.mkdirSync(prodUserDir, { recursive: true });
  const dbWalPath = path.join(prodUserDir, `ftsm_${userId}.db-wal`);
  fs.writeFileSync(dbWalPath, `active-wal-pid-${process.pid}\n`, 'utf8');
  writeJsonAtomic(path.join(prodUserDir, 'session.json'), {
    zalo_user_id: userId,
    profile_id: profileId,
    pid: process.pid,
    account_label: opts.accountLabel,
  });

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
  }

  const windowId = 70_000 + (process.pid % 20_000);
  const windowTitle = opts.accountLabel
    ? `Zalo — ${opts.accountLabel}`
    : `Zalo (${profileId || userId})`;
  writeJsonAtomic(windowStatePath, {
    window_id: windowId,
    owner_pid: process.pid,
    helper_pid: helperChild ? helperChild.pid : null,
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
    symlink_singleton_lock: false,
    effective_tmpdir: effectiveTmpDir,
    socket_path: chromiumSocketPath,
    socket_path_bytes: socketUtf8Bytes,
    zalo_user_id: userId,
    window_id: windowId,
    window_title: windowTitle,
    clean_shutdown: false,
    crashed: false,
  };
  const hostTarget = `${os.hostname()}-${process.pid}`;
  tryCreateSymlink(chromiumSocketPath, singletonSocketLink);
  const symlinkCookie = tryCreateSymlink(`cookie-${process.pid}`, singletonCookieLink);
  launchState.symlink_singleton_lock = symlinkCookie;
  // Persist `launch-state.json` BEFORE publishing `SingletonLock` so relaunch readers always observe updated `launch_count`.
  writeJsonAtomic(launchStatePath, launchState);
  tryCreateSymlink(hostTarget, singletonLockPath);

  let shuttingDown = false;
  let pollTimer = null;

  function cleanShutdown(code = 0) {
    if (shuttingDown) return;
    shuttingDown = true;
    if (pollTimer) clearInterval(pollTimer);

    if (helperChild && helperChild.pid) {
      try {
        helperChild.kill('SIGTERM');
      } catch {
        // Already exited.
      }
    }

    // Flush simulated SQLite WAL on clean shutdown.
    removeNoFollow(dbWalPath);

    const latest = readJsonSafe(launchStatePath, launchState);
    latest.clean_shutdown = code === 0;
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
      try {
        writeJsonAtomic(launchStatePath, latest);
      } catch {
        // Ignore.
      }
      // Deliberately leave SingletonLock, SingletonSocket, SingletonCookie, lockfile, and .window-state.json behind!
      process.exit(1);
    }, opts.crashDelayMs);
    return;
  }

  if (opts.stayMs !== null) {
    setTimeout(() => cleanShutdown(0), opts.stayMs);
  }

  process.on('SIGTERM', () => cleanShutdown(0));
  process.on('SIGINT', () => cleanShutdown(0));

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
      removeNoFollow(quitSignalPath);
      cleanShutdown(0);
    }
  }, 40);
}

main();
