#!/usr/bin/env node
/**
 * Cross-platform synthetic desktop application fixture (`xf-fixture.mjs`).
 *
 * Companion to the native Rust `xf-desktop-fixture` binary so `LaunchTarget::NodeFixture`
 * runs under Authenticode-signed `node.exe` on Windows Smart App Control (SAC=1) hosts
 * as well as macOS and Ubuntu Linux, sharing identical CLI flags and JSON state schemas:
 *   - Instance ID, isolated profile/ZaloData directory resolution, observed env snapshot
 *   - Live open OS file descriptor on `ZaloData/lockfile` (visible to Windows Restart Manager)
 *   - Chromium-compatible dangling `SingletonLock` (`<hostname>-<pid>`) symlink on macOS/Linux
 *   - Child worker process trees (`--spawn-children <N>`, `--orphan-children`)
 *   - Deterministic SplitMix64-style seeded chunk writes + SHA-256 `fixture-files.json`
 *   - Persistent state across restarts (`fixture-state.json` + `launch-state.json`)
 *   - Fault injection: slow startup (pre-lock & post-lock), process hang (`--hang-on-quit`),
 *     simulated crashes (`immediate`, `after-lock`, `mid-write`, `corrupt-state`),
 *     slow disk I/O (`--slow-io-ms`), and controlled errors (`--error-mode`).
 */

import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);

function parseCliArgs(argv) {
  const cfg = {
    instanceId:
      process.env.DIM_INSTANCE_ID ||
      process.env.DIM_PROFILE_ID ||
      process.env.MULTIZALO_PROFILE_ID ||
      `inst-${process.pid.toString(16).padStart(8, '0')}`,
    profileDirOverride: null,
    userDataDirOverride: null,
    userId: process.env.FIXTURE_ZALO_USER_ID || '100001',
    forceQuit: false,
    crashMode: 'none',
    crashDelayMs: 150,
    slowStartupPreLockMs: 0,
    slowStartupPostLockMs: 0,
    hangOnQuit: false,
    quitDelayMs: 0,
    stayMs: null,
    writeFiles: 2,
    writeBytes: 512,
    slowIoMs: 0,
    seed: 0x20261117n,
    lockMode: 'read-shared',
    spawnChildren: 0,
    orphanChildren: false,
    workerOfPid: null,
    workerIndex: 0,
    errorMode: 'none',
    errorExitCode: 42,
    persistKv: {},
    heartbeatMs: 0,
  };

  const nextVal = (idx, prefix) => {
    const cur = argv[idx];
    if (prefix && cur.startsWith(prefix)) {
      return { val: cur.slice(prefix.length), nextIdx: idx };
    }
    if (idx + 1 < argv.length) {
      return { val: argv[idx + 1], nextIdx: idx + 1 };
    }
    return { val: null, nextIdx: idx };
  };

  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--z-update-force-quit') {
      cfg.forceQuit = true;
    } else if (a === '--crash') {
      if (cfg.crashMode === 'none') cfg.crashMode = 'after-lock';
    } else if (a === '--crash-mode' || a.startsWith('--crash-mode=')) {
      const r = nextVal(i, '--crash-mode=');
      i = r.nextIdx;
      if (r.val) cfg.crashMode = r.val.trim().toLowerCase();
    } else if (a === '--crash-delay-ms' || a.startsWith('--crash-delay-ms=')) {
      const r = nextVal(i, '--crash-delay-ms=');
      i = r.nextIdx;
      cfg.crashDelayMs = Math.max(5, Number.parseInt(r.val, 10) || 150);
    } else if (a === '--instance-id' || a.startsWith('--instance-id=')) {
      const r = nextVal(i, '--instance-id=');
      i = r.nextIdx;
      if (r.val && r.val.trim().length > 0) cfg.instanceId = r.val.trim();
    } else if (a === '--profile-dir' || a.startsWith('--profile-dir=')) {
      const r = nextVal(i, '--profile-dir=');
      i = r.nextIdx;
      if (r.val) cfg.profileDirOverride = r.val;
    } else if (a === '--user-data-dir' || a.startsWith('--user-data-dir=')) {
      const r = nextVal(i, '--user-data-dir=');
      i = r.nextIdx;
      if (r.val) cfg.userDataDirOverride = r.val;
    } else if (a === '--user-id' || a.startsWith('--user-id=')) {
      const r = nextVal(i, '--user-id=');
      i = r.nextIdx;
      if (r.val) cfg.userId = r.val;
    } else if (
      a === '--slow-startup-pre-lock-ms' ||
      a.startsWith('--slow-startup-pre-lock-ms=') ||
      a === '--slow-startup-ms' ||
      a.startsWith('--slow-startup-ms=')
    ) {
      const pref = a.startsWith('--slow-startup-ms=')
        ? '--slow-startup-ms='
        : a.startsWith('--slow-startup-pre-lock-ms=')
          ? '--slow-startup-pre-lock-ms='
          : null;
      const r = nextVal(i, pref);
      i = r.nextIdx;
      cfg.slowStartupPreLockMs = Math.max(0, Number.parseInt(r.val, 10) || 0);
    } else if (a === '--slow-startup-post-lock-ms' || a.startsWith('--slow-startup-post-lock-ms=')) {
      const r = nextVal(i, '--slow-startup-post-lock-ms=');
      i = r.nextIdx;
      cfg.slowStartupPostLockMs = Math.max(0, Number.parseInt(r.val, 10) || 0);
    } else if (a === '--hang-on-quit' || a === '--hang') {
      cfg.hangOnQuit = true;
    } else if (a === '--quit-delay-ms' || a.startsWith('--quit-delay-ms=')) {
      const r = nextVal(i, '--quit-delay-ms=');
      i = r.nextIdx;
      cfg.quitDelayMs = Math.max(0, Number.parseInt(r.val, 10) || 0);
    } else if (a === '--stay-ms' || a.startsWith('--stay-ms=')) {
      const r = nextVal(i, '--stay-ms=');
      i = r.nextIdx;
      const n = Number.parseInt(r.val, 10);
      if (Number.isFinite(n) && n >= 0) cfg.stayMs = n;
    } else if (a === '--write-files' || a.startsWith('--write-files=')) {
      const r = nextVal(i, '--write-files=');
      i = r.nextIdx;
      cfg.writeFiles = Math.max(0, Number.parseInt(r.val, 10) ?? 2);
    } else if (a === '--write-bytes' || a.startsWith('--write-bytes=')) {
      const r = nextVal(i, '--write-bytes=');
      i = r.nextIdx;
      cfg.writeBytes = Math.max(0, Number.parseInt(r.val, 10) ?? 512);
    } else if (a === '--slow-io-ms' || a.startsWith('--slow-io-ms=')) {
      const r = nextVal(i, '--slow-io-ms=');
      i = r.nextIdx;
      cfg.slowIoMs = Math.max(0, Number.parseInt(r.val, 10) || 0);
    } else if (a === '--seed' || a.startsWith('--seed=')) {
      const r = nextVal(i, '--seed=');
      i = r.nextIdx;
      try {
        cfg.seed = BigInt(r.val);
      } catch {
        cfg.seed = 0x20261117n;
      }
    } else if (a === '--lock-mode' || a.startsWith('--lock-mode=')) {
      const r = nextVal(i, '--lock-mode=');
      i = r.nextIdx;
      if (r.val) cfg.lockMode = r.val.trim().toLowerCase();
    } else if (a === '--spawn-children' || a.startsWith('--spawn-children=')) {
      const r = nextVal(i, '--spawn-children=');
      i = r.nextIdx;
      cfg.spawnChildren = Math.min(16, Math.max(0, Number.parseInt(r.val, 10) || 0));
    } else if (a === '--orphan-children') {
      cfg.orphanChildren = true;
    } else if (a === '--worker-of' || a.startsWith('--worker-of=')) {
      const r = nextVal(i, '--worker-of=');
      i = r.nextIdx;
      const p = Number.parseInt(r.val, 10);
      if (Number.isInteger(p) && p > 0) cfg.workerOfPid = p;
    } else if (a === '--worker-index' || a.startsWith('--worker-index=')) {
      const r = nextVal(i, '--worker-index=');
      i = r.nextIdx;
      cfg.workerIndex = Math.max(0, Number.parseInt(r.val, 10) || 0);
    } else if (a === '--error-mode' || a.startsWith('--error-mode=')) {
      const r = nextVal(i, '--error-mode=');
      i = r.nextIdx;
      if (r.val) cfg.errorMode = r.val.trim().toLowerCase();
    } else if (a === '--error-code' || a.startsWith('--error-code=')) {
      const r = nextVal(i, '--error-code=');
      i = r.nextIdx;
      cfg.errorExitCode = Math.max(1, Math.min(255, Number.parseInt(r.val, 10) || 42));
    } else if (a === '--set-kv' || a.startsWith('--set-kv=')) {
      const r = nextVal(i, '--set-kv=');
      i = r.nextIdx;
      if (r.val && r.val.includes('=')) {
        const eq = r.val.indexOf('=');
        cfg.persistKv[r.val.slice(0, eq).trim()] = r.val.slice(eq + 1);
      }
    } else if (a === '--heartbeat-ms' || a.startsWith('--heartbeat-ms=')) {
      const r = nextVal(i, '--heartbeat-ms=');
      i = r.nextIdx;
      cfg.heartbeatMs = Math.max(0, Number.parseInt(r.val, 10) || 0);
    }
  }

  return cfg;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
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

function resolveDirectories(cfg) {
  if (cfg.userDataDirOverride) {
    const ud = path.resolve(cfg.userDataDirOverride);
    const prof = cfg.profileDirOverride
      ? path.resolve(cfg.profileDirOverride)
      : path.resolve(ud, '..', '..', '..');
    return { profileDir: prof, zaloDataDir: ud };
  }
  if (cfg.profileDirOverride) {
    const p = path.resolve(cfg.profileDirOverride);
    if (process.platform === 'win32') {
      return { profileDir: p, zaloDataDir: path.join(p, 'AppData', 'Roaming', 'ZaloData') };
    }
    if (process.platform === 'darwin') {
      return { profileDir: p, zaloDataDir: path.join(p, 'Library', 'Application Support', 'ZaloData') };
    }
    return { profileDir: p, zaloDataDir: path.join(p, '.config', 'ZaloData') };
  }
  if (process.env.APPDATA && process.env.APPDATA.trim().length > 0) {
    const roaming = process.env.APPDATA.trim();
    const prof =
      (process.env.USERPROFILE && process.env.USERPROFILE.trim()) ||
      path.resolve(roaming, '..', '..');
    return { profileDir: prof, zaloDataDir: path.join(roaming, 'ZaloData') };
  }
  if (process.platform === 'darwin') {
    const home = process.env.CFFIXED_USER_HOME || process.env.HOME || os.homedir();
    return {
      profileDir: home,
      zaloDataDir: path.join(home, 'Library', 'Application Support', 'ZaloData'),
    };
  }
  if (process.env.XDG_CONFIG_HOME && process.env.XDG_CONFIG_HOME.trim().length > 0) {
    const xdg = process.env.XDG_CONFIG_HOME.trim();
    const home = (process.env.HOME && process.env.HOME.trim()) || path.dirname(xdg);
    return { profileDir: home, zaloDataDir: path.join(xdg, 'ZaloData') };
  }
  const home = process.env.HOME || process.env.USERPROFILE || os.homedir();
  return { profileDir: home, zaloDataDir: path.join(home, '.config', 'ZaloData') };
}

function captureObservedEnv() {
  const keys = [
    'USERPROFILE',
    'APPDATA',
    'LOCALAPPDATA',
    'TEMP',
    'TMP',
    'HOMEDRIVE',
    'HOMEPATH',
    'HOME',
    'CFFIXED_USER_HOME',
    'XDG_CONFIG_HOME',
    'XDG_DATA_HOME',
    'XDG_CACHE_HOME',
    'TMPDIR',
    'DIM_PROFILE_ID',
    'DIM_PROFILE_NAME',
    'DIM_INSTANCE_ID',
  ];
  const out = {};
  for (const k of keys) {
    if (typeof process.env[k] === 'string') {
      out[k] = process.env[k];
    }
  }
  return out;
}

function readJsonSafe(filePath, fallback = null) {
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
  } catch {
    return fallback;
  }
}

function writeBytesAtomic(filePath, buf) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  const base = path.basename(filePath);
  const tmp = path.join(path.dirname(filePath), `.${base}.tmp.${process.pid}`);
  const fd = fs.openSync(tmp, 'w');
  try {
    fs.writeSync(fd, buf);
    fs.fsyncSync(fd);
  } finally {
    fs.closeSync(fd);
  }
  fs.renameSync(tmp, filePath);
}

function writeJsonAtomic(filePath, value) {
  writeBytesAtomic(filePath, Buffer.from(`${JSON.stringify(value, null, 2)}\n`, 'utf8'));
}

function appendLine(filePath, line) {
  try {
    fs.appendFileSync(filePath, `${new Date().toISOString()} ${line}\n`, 'utf8');
  } catch {
    // Non-fatal logging helper.
  }
}

function splitMix64Buffer(seedBigInt, byteLength) {
  const mask = 0xffffffffffffffffn;
  let state = seedBigInt & mask;
  const out = Buffer.alloc(byteLength);
  let offset = 0;
  while (offset < byteLength) {
    state = (state + 0x9e3779b97f4a7c15n) & mask;
    let z = state;
    z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & mask;
    z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & mask;
    z = (z ^ (z >> 31n)) & mask;
    for (let b = 0; b < 8 && offset < byteLength; b += 1) {
      out[offset] = Number((z >> BigInt(b * 8)) & 0xffn);
      offset += 1;
    }
  }
  return out;
}

async function runWorker(cfg) {
  const { zaloDataDir } = resolveDirectories(cfg);
  fs.mkdirSync(zaloDataDir, { recursive: true });
  const marker = path.join(zaloDataDir, `worker-${cfg.workerIndex}.pid`);
  fs.writeFileSync(marker, String(process.pid), 'utf8');

  let stopRequested = false;
  process.on('SIGTERM', () => {
    if (!cfg.hangOnQuit) stopRequested = true;
  });
  process.on('SIGINT', () => {
    if (!cfg.hangOnQuit) stopRequested = true;
  });

  const started = Date.now();
  const maxLifetime = cfg.stayMs !== null ? cfg.stayMs + 5000 : 120000;
  while (!stopRequested) {
    if (!cfg.orphanChildren && !isPidAlive(cfg.workerOfPid)) {
      break;
    }
    if (Date.now() - started >= maxLifetime) {
      break;
    }
    await sleep(40);
  }

  try {
    fs.unlinkSync(marker);
  } catch {
    // Ignore already removed marker.
  }
  return 0;
}

function tryAcquireHeldLock(lockfilePath, singletonLockPath, record) {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      const fd = fs.openSync(lockfilePath, 'wx+');
      const payload = `${JSON.stringify(record, null, 2)}\n`;
      fs.writeSync(fd, payload, 0, 'utf8');
      fs.fsyncSync(fd);

      if (process.platform !== 'win32') {
        try {
          if (fs.lstatSync(singletonLockPath)) fs.unlinkSync(singletonLockPath);
        } catch {
          // Not present.
        }
        try {
          fs.symlinkSync(`${os.hostname()}-${process.pid}`, singletonLockPath);
        } catch {
          // Non-fatal.
        }
      }
      return { fd, recoveredStale: attempt > 0 };
    } catch (err) {
      if (!err || err.code !== 'EEXIST') throw err;
      const existing = readJsonSafe(lockfilePath, null);
      const ownerPid = existing && Number.isInteger(existing.pid) ? existing.pid : null;
      if (ownerPid && ownerPid !== process.pid && isPidAlive(ownerPid)) {
        return null;
      }
      try {
        fs.unlinkSync(lockfilePath);
      } catch {
        return null;
      }
    }
  }
  return null;
}

async function main() {
  const cfg = parseCliArgs(process.argv.slice(2));
  if (cfg.workerOfPid !== null) {
    const workerCode = await runWorker(cfg);
    process.exit(workerCode);
  }

  const bootStartMs = Date.now();
  const startedAtUtc = new Date().toISOString();
  const { profileDir, zaloDataDir } = resolveDirectories(cfg);
  fs.mkdirSync(zaloDataDir, { recursive: true });

  const lockfilePath = path.join(zaloDataDir, 'lockfile');
  const singletonLockPath = path.join(zaloDataDir, 'SingletonLock');
  const quitSignalPath = path.join(zaloDataDir, '.quit-signal');
  const focusSignalPath = path.join(zaloDataDir, '.focus-signal');
  const walLogPath = path.join(zaloDataDir, 'wal.log');
  const startupLogPath = path.join(zaloDataDir, 'startup.log');
  const statePath = path.join(zaloDataDir, 'fixture-state.json');
  const legacyStatePath = path.join(zaloDataDir, 'launch-state.json');
  const filesManifestPath = path.join(zaloDataDir, 'fixture-files.json');

  // 1. Controlled error simulation
  if (cfg.errorMode !== 'none') {
    const codeMap = {
      'exit-code': cfg.errorExitCode,
      custom: cfg.errorExitCode,
      'disk-full': 28,
      enospc: 28,
      'permission-denied': 13,
      eacces: 13,
      'corrupt-config': 65,
      'lock-conflict': 2,
    };
    const exitCode = codeMap[cfg.errorMode] ?? cfg.errorExitCode;
    writeJsonAtomic(path.join(zaloDataDir, 'last-error.json'), {
      error_mode: cfg.errorMode,
      exit_code: exitCode,
      instance_id: cfg.instanceId,
      pid: process.pid,
      occurred_at_utc: new Date().toISOString(),
      message: `Simulated controlled error (${cfg.errorMode})`,
    });
    appendLine(walLogPath, `CONTROLLED_ERROR mode=${cfg.errorMode} code=${exitCode}`);
    process.exit(exitCode);
  }

  // 2. Immediate crash before lock
  if (cfg.crashMode === 'immediate') {
    appendLine(walLogPath, `CRASH_IMMEDIATE pid=${process.pid}`);
    process.exit(101);
  }

  // 3. Simulated slow startup BEFORE acquiring lockfile
  if (cfg.slowStartupPreLockMs > 0) {
    appendLine(walLogPath, `SLOW_STARTUP_PRE_LOCK_BEGIN pid=${process.pid} delay_ms=${cfg.slowStartupPreLockMs}`);
    await sleep(cfg.slowStartupPreLockMs);
  }

  // 4. Force-quit helper invocation
  if (cfg.forceQuit) {
    const existing = readJsonSafe(lockfilePath, null);
    const ownerPid = existing && Number.isInteger(existing.pid) ? existing.pid : null;
    if (ownerPid && isPidAlive(ownerPid)) {
      writeJsonAtomic(quitSignalPath, {
        requested_at: new Date().toISOString(),
        requester_pid: process.pid,
        target_pid: ownerPid,
      });
    } else {
      try {
        fs.unlinkSync(lockfilePath);
      } catch {
        // Ignore.
      }
    }
    process.exit(0);
  }

  // 5. Acquire live open OS file handle on `lockfile`
  const lockRecord = {
    pid: process.pid,
    instance_id: cfg.instanceId,
    profile_id: process.env.DIM_PROFILE_ID || null,
    started_at: startedAtUtc,
    app: 'xf-desktop-fixture',
    lock_mode: cfg.lockMode,
  };

  const lockAcquired = tryAcquireHeldLock(lockfilePath, singletonLockPath, lockRecord);
  if (!lockAcquired) {
    appendLine(startupLogPath, `SECOND_INSTANCE_REJECTED requester_pid=${process.pid}`);
    try {
      writeJsonAtomic(focusSignalPath, {
        focused_at: new Date().toISOString(),
        requester_pid: process.pid,
      });
    } catch {
      // Ignore.
    }
    process.exit(2);
  }
  let heldLockFd = lockAcquired.fd;

  for (const sig of [quitSignalPath, focusSignalPath]) {
    try {
      if (fs.existsSync(sig)) fs.unlinkSync(sig);
    } catch {
      // Ignore.
    }
  }

  // 6. Spawn child worker processes if requested
  const children = [];
  for (let idx = 0; idx < cfg.spawnChildren; idx += 1) {
    const childArgs = [
      __filename,
      '--worker-of',
      String(process.pid),
      '--worker-index',
      String(idx),
      '--profile-dir',
      profileDir,
      '--user-data-dir',
      zaloDataDir,
    ];
    if (cfg.orphanChildren) childArgs.push('--orphan-children');
    if (cfg.hangOnQuit) childArgs.push('--hang-on-quit');
    if (cfg.stayMs !== null) childArgs.push('--stay-ms', String(cfg.stayMs));
    const child = spawn(process.execPath, childArgs, {
      stdio: 'ignore',
      windowsHide: true,
    });
    children.push(child);
  }
  const childPids = children.map((c) => c.pid).filter((p) => Number.isInteger(p));
  if (childPids.length > 0) {
    writeJsonAtomic(path.join(zaloDataDir, 'children.json'), {
      parent_pid: process.pid,
      child_pids: childPids,
    });
  }

  // 7. Simulated slow startup AFTER acquiring lockfile
  if (cfg.slowStartupPostLockMs > 0) {
    await sleep(cfg.slowStartupPostLockMs);
  }

  // 8. Pre-seed Zalo-compatible files + write seeded storage chunks
  const appDataParent = path.dirname(zaloDataDir);
  const zmetaPath = path.join(appDataParent, 'zmeta.json');
  if (!fs.existsSync(zmetaPath)) {
    fs.writeFileSync(zmetaPath, '{"failedMigrateCount":0}', 'utf8');
  }
  const migrateConfigPath = path.join(zaloDataDir, 'migrate-config.json');
  if (!fs.existsSync(migrateConfigPath)) {
    fs.writeFileSync(migrateConfigPath, '{"zalopc_m_c":true}', 'utf8');
  }
  const prodDir = path.join(zaloDataDir, 'Database', '_production', cfg.userId);
  fs.mkdirSync(prodDir, { recursive: true });
  writeJsonAtomic(path.join(prodDir, 'session.json'), {
    zalo_user_id: cfg.userId,
    instance_id: cfg.instanceId,
    pid: process.pid,
    updated_at: new Date().toISOString(),
  });
  writeJsonAtomic(path.join(zaloDataDir, 'Local State'), {
    os_crypt: { xf_fixture: true },
    instance_id: cfg.instanceId,
  });
  writeJsonAtomic(path.join(zaloDataDir, 'config.json'), {
    fixture: true,
    app_version: 'xf-fixture-1.0.0',
    instance_id: cfg.instanceId,
  });

  const mask64 = 0xffffffffffffffffn;
  let mixSeed = cfg.seed & mask64;
  for (const byte of Buffer.from(cfg.instanceId, 'utf8')) {
    mixSeed = (mixSeed * 1099511628211n + BigInt(byte)) & mask64;
  }
  const chunks = [];
  let totalBytes = 0;
  for (let idx = 0; idx < cfg.writeFiles; idx += 1) {
    if (cfg.slowIoMs > 0) {
      await sleep(cfg.slowIoMs);
    }
    const chunkSeed = (mixSeed + BigInt(idx) * 0x9e3779b9n) & mask64;
    const buf = splitMix64Buffer(chunkSeed, cfg.writeBytes);
    const rel = `storage/chunk_${String(idx).padStart(4, '0')}.bin`;
    writeBytesAtomic(path.join(zaloDataDir, rel), buf);
    const sha256 = crypto.createHash('sha256').update(buf).digest('hex');
    totalBytes += buf.length;
    chunks.push({ relative_path: rel, size_bytes: buf.length, sha256 });
  }
  writeJsonAtomic(filesManifestPath, {
    instance_id: cfg.instanceId,
    pid: process.pid,
    seed: `0x${mixSeed.toString(16).padStart(16, '0')}`,
    total_bytes: totalBytes,
    chunks,
  });

  // 9. Advance persistent state
  const prev = readJsonSafe(statePath, null);
  const uncleanPrev = prev ? !prev.clean_shutdown : false;
  const pidHistory = Array.isArray(prev?.pid_history) ? [...prev.pid_history, process.pid].slice(-32) : [process.pid];
  const state = {
    instance_id: cfg.instanceId,
    profile_dir: profileDir,
    zalo_data_dir: zaloDataDir,
    boot_sequence: (Number.isInteger(prev?.boot_sequence) ? prev.boot_sequence : 0) + 1,
    clean_shutdown_count: Number.isInteger(prev?.clean_shutdown_count) ? prev.clean_shutdown_count : 0,
    unclean_previous_boot: uncleanPrev,
    focus_count: Number.isInteger(prev?.focus_count) ? prev.focus_count : 0,
    last_pid: process.pid,
    child_pids: childPids,
    pid_history: pidHistory,
    started_at_utc: startedAtUtc,
    ready_at_utc: new Date().toISOString(),
    startup_duration_ms: Date.now() - bootStartMs,
    last_focused_at: prev?.last_focused_at || null,
    last_stopped_at: prev?.last_stopped_at || null,
    clean_shutdown: false,
    crashed: false,
    crash_mode: null,
    zalo_user_id: cfg.userId,
    observed_env: captureObservedEnv(),
    persisted_kv: { ...(prev?.persisted_kv || {}), ...cfg.persistKv },
    heartbeat_ticks: Number.isInteger(prev?.heartbeat_ticks) ? prev.heartbeat_ticks : 0,
  };

  const syncStates = () => {
    writeJsonAtomic(statePath, state);
    writeJsonAtomic(legacyStatePath, {
      launch_count: state.boot_sequence,
      focus_count: state.focus_count,
      last_pid: state.last_pid,
      last_started_at: state.started_at_utc,
      last_focused_at: state.last_focused_at,
      last_stopped_at: state.last_stopped_at,
      clean_shutdown: state.clean_shutdown,
      crashed: state.crashed,
      zalo_user_id: state.zalo_user_id,
    });
  };
  syncStates();

  appendLine(
    startupLogPath,
    `PRIMARY_STARTED pid=${process.pid} launch_count=${state.boot_sequence} instance_id=${state.instance_id} user_id=${cfg.userId}`,
  );
  appendLine(walLogPath, `BOOT_READY seq=${state.boot_sequence} pid=${process.pid} children=${JSON.stringify(childPids)}`);

  try {
    process.stdout.write(
      `${JSON.stringify({
        event: 'ready',
        instance_id: state.instance_id,
        pid: process.pid,
        child_pids: childPids,
        profile_dir: profileDir,
        zalo_data_dir: zaloDataDir,
        boot_sequence: state.boot_sequence,
        startup_duration_ms: state.startup_duration_ms,
        zalo_user_id: cfg.userId,
      })}\n`,
    );
  } catch {
    // Ignore closed stdout.
  }

  const killChildren = () => {
    if (cfg.orphanChildren) return;
    for (const c of children) {
      try {
        c.kill('SIGKILL');
      } catch {
        // Ignore already dead child.
      }
    }
  };

  // 10. Post-lock crash injection (intentionally keeps lockfile + SingletonLock behind!)
  if (cfg.crashMode === 'after-lock' || cfg.crashMode === 'mid-write' || cfg.crashMode === 'corrupt-state') {
    await sleep(cfg.crashDelayMs);
    if (cfg.crashMode === 'mid-write') {
      fs.mkdirSync(path.join(zaloDataDir, 'storage'), { recursive: true });
      fs.writeFileSync(path.join(zaloDataDir, 'storage', 'torn_write.bin.tmp'), 'PARTIAL_UNCOMMITTED_FRAME_', 'utf8');
    } else if (cfg.crashMode === 'corrupt-state') {
      fs.writeFileSync(path.join(zaloDataDir, 'corrupt-cache.json'), '{"broken": [1, 2, ', 'utf8');
    }
    state.crashed = true;
    state.crash_mode = cfg.crashMode;
    state.last_stopped_at = new Date().toISOString();
    syncStates();
    appendLine(startupLogPath, `CRASH_SIMULATED mode=${cfg.crashMode} pid=${process.pid} stale_lock_left=true`);
    killChildren();
    const crashExit = cfg.crashMode === 'mid-write' ? 138 : cfg.crashMode === 'corrupt-state' ? 139 : 137;
    process.exit(crashExit);
  }

  // 11. Event loop & signal handlers
  let signalQuit = false;
  process.on('SIGTERM', () => {
    if (cfg.hangOnQuit) {
      appendLine(walLogPath, `HANG_ON_SIGTERM_IGNORED pid=${process.pid}`);
    } else {
      signalQuit = true;
    }
  });
  process.on('SIGINT', () => {
    if (cfg.hangOnQuit) {
      appendLine(walLogPath, `HANG_ON_SIGINT_IGNORED pid=${process.pid}`);
    } else {
      signalQuit = true;
    }
  });

  const loopStart = Date.now();
  let lastHeartbeat = Date.now();

  while (true) {
    if (fs.existsSync(focusSignalPath)) {
      try {
        fs.unlinkSync(focusSignalPath);
      } catch {
        // Ignore.
      }
      state.focus_count += 1;
      state.last_focused_at = new Date().toISOString();
      syncStates();
      appendLine(startupLogPath, `FOCUS_RECEIVED focus_count=${state.focus_count}`);
    }

    if (fs.existsSync(quitSignalPath) || signalQuit) {
      if (cfg.hangOnQuit) {
        if (fs.existsSync(quitSignalPath)) {
          try {
            fs.unlinkSync(quitSignalPath);
          } catch {
            // Ignore.
          }
        }
        signalQuit = false;
        appendLine(walLogPath, `HANG_ON_QUIT_IGNORED pid=${process.pid}`);
      } else {
        try {
          if (fs.existsSync(quitSignalPath)) fs.unlinkSync(quitSignalPath);
        } catch {
          // Ignore.
        }
        if (cfg.quitDelayMs > 0) {
          await sleep(cfg.quitDelayMs);
        }
        break;
      }
    }

    if (cfg.stayMs !== null && Date.now() - loopStart >= cfg.stayMs) {
      break;
    }

    if (cfg.heartbeatMs > 0 && Date.now() - lastHeartbeat >= cfg.heartbeatMs) {
      state.heartbeat_ticks += 1;
      syncStates();
      appendLine(walLogPath, `HEARTBEAT tick=${state.heartbeat_ticks} pid=${process.pid}`);
      lastHeartbeat = Date.now();
    }

    await sleep(25);
  }

  // 12. Graceful shutdown
  killChildren();
  state.clean_shutdown = true;
  state.clean_shutdown_count += 1;
  state.last_stopped_at = new Date().toISOString();
  syncStates();
  appendLine(startupLogPath, `PRIMARY_STOPPED pid=${process.pid} code=0`);
  appendLine(walLogPath, `CLEAN_SHUTDOWN seq=${state.boot_sequence} pid=${process.pid}`);

  if (heldLockFd !== null) {
    try {
      fs.closeSync(heldLockFd);
    } catch {
      // Ignore.
    }
    heldLockFd = null;
  }
  for (const artifact of [
    lockfilePath,
    singletonLockPath,
    quitSignalPath,
    focusSignalPath,
    path.join(zaloDataDir, 'children.json'),
  ]) {
    try {
      if (fs.lstatSync(artifact)) fs.unlinkSync(artifact);
    } catch {
      // Absent.
    }
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`xf-fixture fatal error: ${err.stack || err.message}\n`);
  process.exit(1);
});
