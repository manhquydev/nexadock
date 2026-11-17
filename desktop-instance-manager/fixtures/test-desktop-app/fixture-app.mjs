#!/usr/bin/env node
/**
 * MultiZaloDesktop multi-instance test fixture (`fixture-app.mjs`).
 *
 * Mimics Zalo Desktop's single-instance UserData lockfile contract using pure Node.js built-ins
 * so integration tests run natively under vendor-signed Node.js even on Smart App Control hosts.
 */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';

function parseArgs(argv) {
  const opts = {
    forceQuit: false,
    crash: false,
    crashDelayMs: 150,
    stayMs: null,
    userId: process.env.FIXTURE_ZALO_USER_ID || '100001',
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--z-update-force-quit') {
      opts.forceQuit = true;
    } else if (arg === '--crash') {
      opts.crash = true;
    } else if (arg === '--crash-delay-ms' && i + 1 < argv.length) {
      opts.crashDelayMs = Math.max(10, Number.parseInt(argv[i + 1], 10) || 150);
      i += 1;
    } else if (arg.startsWith('--crash-delay-ms=')) {
      opts.crashDelayMs = Math.max(10, Number.parseInt(arg.slice('--crash-delay-ms='.length), 10) || 150);
    } else if (arg === '--stay-ms' && i + 1 < argv.length) {
      const parsed = Number.parseInt(argv[i + 1], 10);
      if (Number.isFinite(parsed) && parsed > 0) opts.stayMs = parsed;
      i += 1;
    } else if (arg.startsWith('--stay-ms=')) {
      const parsed = Number.parseInt(arg.slice('--stay-ms='.length), 10);
      if (Number.isFinite(parsed) && parsed > 0) opts.stayMs = parsed;
    } else if (arg === '--user-id' && i + 1 < argv.length) {
      opts.userId = argv[i + 1];
      i += 1;
    } else if (arg.startsWith('--user-id=')) {
      opts.userId = arg.slice('--user-id='.length);
    }
  }

  return opts;
}

function resolveAppDataDir() {
  if (process.env.APPDATA && process.env.APPDATA.trim().length > 0) {
    return process.env.APPDATA.trim();
  }
  if (process.platform === 'darwin') {
    const home = process.env.CFFIXED_USER_HOME || process.env.HOME || os.homedir();
    return path.join(home, 'Library', 'Application Support');
  }
  if (process.env.XDG_CONFIG_HOME && process.env.XDG_CONFIG_HOME.trim().length > 0) {
    return process.env.XDG_CONFIG_HOME.trim();
  }
  const home = process.env.HOME || process.env.USERPROFILE || os.homedir();
  return path.join(home, '.config');
}

function isPidAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) {
    return false;
  }
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return err && err.code === 'EPERM';
  }
}

function readJsonSafe(filePath, fallback) {
  try {
    const raw = fs.readFileSync(filePath, 'utf8');
    return JSON.parse(raw);
  } catch {
    return fallback;
  }
}

function writeJsonAtomic(filePath, value) {
  const tmpPath = `${filePath}.tmp.${process.pid}`;
  fs.writeFileSync(tmpPath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
  fs.renameSync(tmpPath, filePath);
}

function appendLog(logPath, line) {
  try {
    fs.appendFileSync(logPath, `${new Date().toISOString()} ${line}\n`, 'utf8');
  } catch {
    // Ignore log errors so lock handling never aborts unexpectedly.
  }
}

function acquireExclusiveLock(lockfilePath, startupLogPath, focusSignalPath, quitSignalPath, forceQuit) {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      const fd = fs.openSync(lockfilePath, 'wx');
      const payload = {
        pid: process.pid,
        started_at: new Date().toISOString(),
        app: 'dim-test-fixture',
        profile_id: process.env.DIM_PROFILE_ID || process.env.MULTIZALO_PROFILE_ID || null,
      };
      fs.writeSync(fd, `${JSON.stringify(payload, null, 2)}\n`, 0, 'utf8');
      fs.closeSync(fd);
      return { acquired: true, recoveredStale: attempt > 0 };
    } catch (err) {
      if (!err || err.code !== 'EEXIST') {
        throw err;
      }

      const existing = readJsonSafe(lockfilePath, null);
      const existingPid = existing && Number.isInteger(existing.pid) ? existing.pid : null;

      if (existingPid !== null && existingPid !== process.pid && isPidAlive(existingPid)) {
        if (forceQuit) {
          writeJsonAtomic(quitSignalPath, {
            requested_at: new Date().toISOString(),
            requester_pid: process.pid,
            target_pid: existingPid,
          });
          process.exit(0);
        }

        appendLog(startupLogPath, `SECOND_INSTANCE_REJECTED active_pid=${existingPid} requester_pid=${process.pid}`);
        try {
          writeJsonAtomic(focusSignalPath, {
            focused_at: new Date().toISOString(),
            requester_pid: process.pid,
          });
        } catch {
          // Best-effort focus notification.
        }
        process.exit(2);
      }

      // Stale lockfile from crashed or exited process.
      if (forceQuit) {
        try {
          fs.unlinkSync(lockfilePath);
        } catch {
          // Ignore race.
        }
        process.exit(0);
      }

      try {
        fs.unlinkSync(lockfilePath);
      } catch {
        // Retry if another racing process cleaned it first.
      }
    }
  }

  appendLog(startupLogPath, `SECOND_INSTANCE_REJECTED lock_contention requester_pid=${process.pid}`);
  process.exit(2);
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  const appDataDir = resolveAppDataDir();
  const userDataDir = path.join(appDataDir, 'ZaloData');

  fs.mkdirSync(appDataDir, { recursive: true });
  fs.mkdirSync(userDataDir, { recursive: true });

  const lockfilePath = path.join(userDataDir, 'lockfile');
  const singletonLockPath = path.join(userDataDir, 'SingletonLock');
  const startupLogPath = path.join(userDataDir, 'startup.log');
  const focusSignalPath = path.join(userDataDir, '.focus-signal');
  const quitSignalPath = path.join(userDataDir, '.quit-signal');
  const launchStatePath = path.join(userDataDir, 'launch-state.json');

  const lockOutcome = acquireExclusiveLock(
    lockfilePath,
    startupLogPath,
    focusSignalPath,
    quitSignalPath,
    opts.forceQuit,
  );

  // If --z-update-force-quit ran when no primary instance was running, release lock and exit 0.
  if (opts.forceQuit) {
    try {
      fs.unlinkSync(lockfilePath);
    } catch {
      // Ignore.
    }
    process.exit(0);
  }

  // Clear leftover signals from previous lifecycle before starting poll loop.
  for (const sigFile of [quitSignalPath, focusSignalPath]) {
    if (fs.existsSync(sigFile)) {
      try {
        fs.unlinkSync(sigFile);
      } catch {
        // Ignore.
      }
    }
  }

  if (process.platform !== 'win32') {
    try {
      if (fs.existsSync(singletonLockPath)) fs.unlinkSync(singletonLockPath);
      fs.symlinkSync(`${os.hostname()}-${process.pid}`, singletonLockPath);
    } catch {
      // Non-fatal if symlink unavailable.
    }
  }

  // Pre-seed Zalo-compatible isolation markers and data files.
  const zmetaPath = path.join(appDataDir, 'zmeta.json');
  if (!fs.existsSync(zmetaPath)) {
    fs.writeFileSync(zmetaPath, '{"failedMigrateCount":0}', 'utf8');
  }
  const migrateConfigPath = path.join(userDataDir, 'migrate-config.json');
  if (!fs.existsSync(migrateConfigPath)) {
    fs.writeFileSync(migrateConfigPath, '{"zalopc_m_c":true}', 'utf8');
  }

  writeJsonAtomic(path.join(userDataDir, 'Local State'), {
    os_crypt: { fixture_mode: true },
    profile: {
      info_cache: {
        Default: {
          active_time: Date.now(),
        },
      },
    },
  });

  writeJsonAtomic(path.join(userDataDir, 'config.json'), {
    fixture: true,
    app_version: 'fixture-1.0.0',
    updated_at: new Date().toISOString(),
  });

  const prodUserDir = path.join(userDataDir, 'Database', '_production', opts.userId);
  fs.mkdirSync(prodUserDir, { recursive: true });
  writeJsonAtomic(path.join(prodUserDir, 'session.json'), {
    zalo_user_id: opts.userId,
    pid: process.pid,
    updated_at: new Date().toISOString(),
  });

  const prevState = readJsonSafe(launchStatePath, {});
  const launchState = {
    launch_count: (Number.isInteger(prevState.launch_count) ? prevState.launch_count : 0) + 1,
    focus_count: Number.isInteger(prevState.focus_count) ? prevState.focus_count : 0,
    last_pid: process.pid,
    last_started_at: new Date().toISOString(),
    last_focused_at: prevState.last_focused_at || null,
    last_stopped_at: prevState.last_stopped_at || null,
    clean_shutdown: false,
    crashed: false,
    zalo_user_id: opts.userId,
  };
  writeJsonAtomic(launchStatePath, launchState);

  if (lockOutcome.recoveredStale) {
    appendLog(startupLogPath, `STALE_LOCK_RECOVERED pid=${process.pid}`);
  }
  appendLog(
    startupLogPath,
    `PRIMARY_STARTED pid=${process.pid} launch_count=${launchState.launch_count} user_id=${opts.userId}`,
  );

  try {
    process.stdout.write(
      `${JSON.stringify({
        event: 'ready',
        pid: process.pid,
        userDataDir,
        launch_count: launchState.launch_count,
        zalo_user_id: opts.userId,
      })}\n`,
    );
  } catch {
    // Ignore closed stdout pipes.
  }

  let shuttingDown = false;
  let pollTimer = null;

  function cleanUpAndExit(exitCode = 0) {
    if (shuttingDown) return;
    shuttingDown = true;
    if (pollTimer) clearInterval(pollTimer);

    const latest = readJsonSafe(launchStatePath, launchState);
    latest.clean_shutdown = exitCode === 0;
    latest.last_stopped_at = new Date().toISOString();
    try {
      writeJsonAtomic(launchStatePath, latest);
    } catch {
      // Ignore flush failures on shutdown.
    }

    appendLog(startupLogPath, `PRIMARY_STOPPED pid=${process.pid} code=${exitCode}`);

    for (const f of [lockfilePath, singletonLockPath, quitSignalPath, focusSignalPath]) {
      try {
        if (fs.lstatSync(f)) {
          fs.unlinkSync(f);
        }
      } catch {
        // File already absent.
      }
    }
    process.exit(exitCode);
  }

  if (opts.crash) {
    setTimeout(() => {
      const latest = readJsonSafe(launchStatePath, launchState);
      latest.crashed = true;
      latest.clean_shutdown = false;
      latest.last_stopped_at = new Date().toISOString();
      try {
        writeJsonAtomic(launchStatePath, latest);
      } catch {
        // Ignore.
      }
      appendLog(startupLogPath, `CRASH_SIMULATED pid=${process.pid} stale_lock_left=true`);
      // Intentionally leave lockfile behind to exercise crash recovery.
      process.exit(1);
    }, opts.crashDelayMs);
    return;
  }

  if (opts.stayMs !== null) {
    setTimeout(() => cleanUpAndExit(0), opts.stayMs);
  }

  process.on('SIGTERM', () => cleanUpAndExit(0));
  process.on('SIGINT', () => cleanUpAndExit(0));

  if (process.stdin && typeof process.stdin.on === 'function') {
    let stdinBuf = '';
    process.stdin.on('error', () => {
      // Ignore detached/broken pipe stdin errors.
    });
    process.stdin.on('data', (chunk) => {
      stdinBuf += chunk.toString('utf8');
      if (stdinBuf.split(/\r?\n/).some((line) => line.trim().toLowerCase() === 'quit')) {
        cleanUpAndExit(0);
      }
      if (stdinBuf.length > 4096) {
        stdinBuf = stdinBuf.slice(-512);
      }
    });
  }

  pollTimer = setInterval(() => {
    if (fs.existsSync(focusSignalPath)) {
      let focusedAt = new Date().toISOString();
      try {
        const rawFocus = fs.readFileSync(focusSignalPath, 'utf8').trim();
        if (rawFocus.startsWith('{')) {
          const sig = JSON.parse(rawFocus);
          if (sig && typeof sig.focused_at === 'string' && sig.focused_at.length > 0) {
            focusedAt = sig.focused_at;
          }
        } else if (rawFocus.length > 0) {
          focusedAt = rawFocus;
        }
      } catch {
        // Fallback to current timestamp.
      }
      try {
        fs.unlinkSync(focusSignalPath);
      } catch {
        // Ignore.
      }
      const latest = readJsonSafe(launchStatePath, launchState);
      latest.focus_count = (Number.isInteger(latest.focus_count) ? latest.focus_count : 0) + 1;
      latest.last_focused_at = focusedAt;
      try {
        writeJsonAtomic(launchStatePath, latest);
      } catch {
        // Ignore.
      }
      appendLog(startupLogPath, `FOCUS_RECEIVED focus_count=${latest.focus_count}`);
    }

    if (fs.existsSync(quitSignalPath)) {
      try {
        fs.unlinkSync(quitSignalPath);
      } catch {
        // Ignore.
      }
      cleanUpAndExit(0);
    }
  }, 50);
}

main();
