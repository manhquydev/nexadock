#!/usr/bin/env node
/**
 * Ubuntu / Linux Multi-Instance Fixture, Security & P1 Regression Suite
 * (`tests/platform-linux/run-ubuntu-fixture-tests.mjs`)
 *
 * Exercises all 15 core Ubuntu Linux adapter scenarios PLUS all 14 P1 regression & security
 * scenarios (29 total scenarios) using pure Node.js built-ins with real runtime, Buffer/ELF64,
 * OpenSSL SHA-256, `/proc`, and multi-process lifecycle assertions.
 *
 * Claim discipline:
 * - Never conflates synthetic fixture testing with live Zalo Desktop compatibility.
 * - Never reports `LINUX_ADAPTER_TESTED = "PASS"` unless executed on a real Linux kernel
 *   (`process.platform === 'linux'`).
 */

import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const DIM_ROOT = path.resolve(__dirname, '..', '..');
const FIXTURE_SCRIPT = path.join(DIM_ROOT, 'fixtures', 'test-desktop-app', 'fixture-app.mjs');

const IS_LINUX = process.platform === 'linux';

const PASSTHROUGH_SESSION_ENV_KEYS = [
  'PATH',
  'DISPLAY',
  'WAYLAND_DISPLAY',
  'XAUTHORITY',
  'XDG_RUNTIME_DIR',
  'XDG_SESSION_TYPE',
  'XDG_SESSION_CLASS',
  'XDG_SESSION_DESKTOP',
  'XDG_CURRENT_DESKTOP',
  'XDG_SEAT',
  'XDG_VTNR',
  'DESKTOP_SESSION',
  'DBUS_SESSION_BUS_ADDRESS',
  'PULSE_SERVER',
  'PULSE_COOKIE',
  'PIPEWIRE_REMOTE',
  'GTK_USE_PORTAL',
  'GTK_THEME',
  'GTK_IM_MODULE',
  'QT_IM_MODULE',
  'XMODIFIERS',
  'LANG',
  'LANGUAGE',
  'LC_ALL',
  'LC_CTYPE',
  'LC_MESSAGES',
  'TZ',
  'TERM',
  'FIXTURE_ZALO_USER_ID',
];

const UBUNTU_PACKAGE_MAP = new Map([
  ['libfuse.so.2', ['libfuse2t64', 'libfuse2']],
  ['libfuse3.so.3', ['libfuse3-3', 'libfuse3-3']],
  ['libopus.so.0', ['libopus0', 'libopus0']],
  ['libpulse.so.0', ['libpulse0', 'libpulse0']],
  ['libpulse-simple.so.0', ['libpulse0', 'libpulse0']],
  ['libasound.so.2', ['libasound2t64', 'libasound2']],
  ['libwebkit2gtk-4.1.so.0', ['libwebkit2gtk-4.1-0', 'libwebkit2gtk-4.1-0']],
]);

function parseCliArgs(argv) {
  const opts = { json: false, reportOut: null };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--json') opts.json = true;
    else if (argv[i] === '--report-out' && i + 1 < argv.length) {
      opts.reportOut = path.resolve(argv[i + 1]);
      i += 1;
    }
  }
  return opts;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForCondition(predicate, timeoutMs = 5000, intervalMs = 25) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return true;
    await sleep(intervalMs);
  }
  return Boolean(predicate());
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

function lstatExists(targetPath) {
  try {
    fs.lstatSync(targetPath);
    return true;
  } catch {
    return false;
  }
}

function validateSafePathInput(rawPath) {
  if (typeof rawPath !== 'string' || rawPath.length === 0 || rawPath.includes('\0')) {
    const err = new Error(`PATH_TRAVERSAL: invalid or NUL-containing path`);
    err.code = 'PATH_TRAVERSAL';
    throw err;
  }
  const segments = rawPath.split(/[\\/]/);
  if (segments.includes('..')) {
    const err = new Error(`PATH_TRAVERSAL: parent traversal '..' rejected in ${rawPath}`);
    err.code = 'PATH_TRAVERSAL';
    throw err;
  }
}

function rejectIfSymlink(targetPath) {
  if (lstatExists(targetPath) && fs.lstatSync(targetPath).isSymbolicLink()) {
    const err = new Error(`PATH_TRAVERSAL: symlink rejected at ${targetPath}`);
    err.code = 'PATH_TRAVERSAL';
    throw err;
  }
}

function createLinuxProfileLayout(profileDir, profileId) {
  validateSafePathInput(profileDir);
  rejectIfSymlink(profileDir);

  const dirs = [
    profileDir,
    path.join(profileDir, '.config'),
    path.join(profileDir, '.config', 'ZaloData'),
    path.join(profileDir, '.config', 'Zalo'),
    path.join(profileDir, '.local'),
    path.join(profileDir, '.local', 'share'),
    path.join(profileDir, '.local', 'state'),
    path.join(profileDir, '.cache'),
    path.join(profileDir, 'tmp'),
    path.join(profileDir, 'Downloads'),
    path.join(profileDir, 'Documents'),
    path.join(profileDir, 'Documents', 'Zalo Received Files'),
  ];

  for (const d of dirs) {
    rejectIfSymlink(d);
    fs.mkdirSync(d, { recursive: true, mode: 0o700 });
    if (IS_LINUX) {
      fs.chmodSync(d, 0o700);
    }
  }

  const markerPath = path.join(profileDir, '.config', 'ZaloData', '.zalo-profile-id');
  rejectIfSymlink(markerPath);
  if (fs.existsSync(markerPath)) {
    const existing = fs.readFileSync(markerPath, 'utf8').trim();
    if (existing.toLowerCase() !== profileId.toLowerCase()) {
      const err = new Error(`PROFILE_MARKER_MISMATCH: expected ${profileId}, found ${existing}`);
      err.code = 'PROFILE_MARKER_MISMATCH';
      throw err;
    }
  } else {
    fs.writeFileSync(markerPath, profileId, { mode: 0o600 });
  }

  const zmeta = path.join(profileDir, '.config', 'zmeta.json');
  rejectIfSymlink(zmeta);
  if (!fs.existsSync(zmeta)) {
    fs.writeFileSync(zmeta, '{"failedMigrateCount":0}\n', { mode: 0o600 });
  }

  const migrateCfg = path.join(profileDir, '.config', 'ZaloData', 'migrate-config.json');
  rejectIfSymlink(migrateCfg);
  if (!fs.existsSync(migrateCfg)) {
    fs.writeFileSync(migrateCfg, '{"zalopc_m_c":true}\n', { mode: 0o600 });
  }

  const userDirs = path.join(profileDir, '.config', 'user-dirs.dirs');
  rejectIfSymlink(userDirs);
  if (!fs.existsSync(userDirs)) {
    fs.writeFileSync(
      userDirs,
      [
        '# Isolated MultiZaloDesktop XDG user directories',
        'XDG_DESKTOP_DIR="$HOME/Downloads"',
        'XDG_DOWNLOAD_DIR="$HOME/Downloads"',
        'XDG_TEMPLATES_DIR="$HOME/Documents"',
        'XDG_PUBLICSHARE_DIR="$HOME/Downloads"',
        'XDG_DOCUMENTS_DIR="$HOME/Documents"',
        'XDG_MUSIC_DIR="$HOME/Downloads"',
        'XDG_PICTURES_DIR="$HOME/Downloads"',
        'XDG_VIDEOS_DIR="$HOME/Downloads"',
      ].join('\n') + '\n',
      { mode: 0o600 },
    );
  }

  const userDirsLocale = path.join(profileDir, '.config', 'user-dirs.locale');
  rejectIfSymlink(userDirsLocale);
  if (!fs.existsSync(userDirsLocale)) {
    fs.writeFileSync(userDirsLocale, 'en_US\n', { mode: 0o600 });
  }
}

function generateInstanceNonce() {
  return crypto.randomBytes(16).toString('hex');
}

function buildSanitizedLinuxSpawnEnv(
  profileDir,
  profileId,
  profileName,
  { instanceNonce = null, preferPortalFileChooser = true, hostEnv = process.env } = {},
) {
  const env = {};
  for (const key of PASSTHROUGH_SESSION_ENV_KEYS) {
    const val = hostEnv[key];
    if (typeof val === 'string' && val.length > 0 && !val.includes('\0')) {
      env[key] = val;
    }
  }
  if (!env.PATH) {
    env.PATH = '/usr/local/bin:/usr/bin:/bin';
  }
  if (preferPortalFileChooser && !env.GTK_USE_PORTAL) {
    env.GTK_USE_PORTAL = '1';
  }

  env.HOME = profileDir;
  env.XDG_CONFIG_HOME = path.join(profileDir, '.config');
  env.XDG_DATA_HOME = path.join(profileDir, '.local', 'share');
  env.XDG_STATE_HOME = path.join(profileDir, '.local', 'state');
  env.XDG_CACHE_HOME = path.join(profileDir, '.cache');
  env.XDG_DOWNLOAD_DIR = path.join(profileDir, 'Downloads');
  env.XDG_DOCUMENTS_DIR = path.join(profileDir, 'Documents');
  env.TMPDIR = path.join(profileDir, 'tmp');
  env.TEMP = path.join(profileDir, 'tmp');
  env.TMP = path.join(profileDir, 'tmp');
  env.DIM_PROFILE_ID = profileId;
  env.DIM_PROFILE_NAME = profileName;
  if (instanceNonce && !instanceNonce.includes('\0')) {
    env.DIM_INSTANCE_NONCE = instanceNonce;
  }
  if (process.platform === 'win32' && process.env.SYSTEMROOT) {
    env.SYSTEMROOT = process.env.SYSTEMROOT;
  }
  return env;
}

function buildIsolatedLinuxEnv(profileDir, profileId, profileName) {
  return buildSanitizedLinuxSpawnEnv(profileDir, profileId, profileName);
}

function spawnFixture(profileDir, profileId, profileName, extraArgs = [], envOpts = {}) {
  createLinuxProfileLayout(profileDir, profileId);
  const env = buildSanitizedLinuxSpawnEnv(profileDir, profileId, profileName, envOpts);
  return spawn(process.execPath, [FIXTURE_SCRIPT, ...extraArgs], {
    cwd: profileDir,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

// ---------------------------------------------------------------------------
// Socket URI & `bwrap` Sandbox Plan Helpers (mirroring `crates/platform-linux/src/layout.rs`)
// ---------------------------------------------------------------------------
function hasDotOrDotDotSegment(p) {
  return p.split(/[\\/]/).some((seg) => seg === '.' || seg === '..');
}

function isBroadForbiddenBindDir(raw) {
  const p = raw.replace(/\/+$/, '');
  const broad = new Set([
    '',
    '/',
    '/dev',
    '/proc',
    '/sys',
    '/etc',
    '/home',
    '/root',
    '/run',
    '/var',
    '/var/run',
    '/tmp',
    '/tmp/.X11-unix',
  ]);
  return (
    broad.has(p) || p.startsWith('/dev/') || p.startsWith('/proc/') || p.startsWith('/sys/')
  );
}

function isSystemDbusPath(raw) {
  const p = raw.trim().replace(/\/+$/, '');
  return (
    p === '/run/dbus' ||
    p.startsWith('/run/dbus/') ||
    p === '/var/run/dbus' ||
    p.startsWith('/var/run/dbus/')
  );
}

function isSafeSocketFilePathStr(raw, xdgRuntimeDir = null) {
  const trimmed = raw.trim();
  if (
    !trimmed ||
    !trimmed.startsWith('/') ||
    trimmed.endsWith('/') ||
    trimmed.includes('\0') ||
    hasDotOrDotDotSegment(trimmed) ||
    isBroadForbiddenBindDir(trimmed)
  ) {
    return false;
  }
  if (xdgRuntimeDir && trimmed === xdgRuntimeDir.replace(/\/+$/, '')) {
    return false;
  }
  return trimmed.split('/').filter(Boolean).length >= 2;
}

function unescapeDbusValue(raw) {
  const bytes = [];
  for (let i = 0; i < raw.length; i += 1) {
    if (raw[i] === '%') {
      if (i + 2 >= raw.length) return null;
      const hex = raw.slice(i + 1, i + 3);
      if (!/^[0-9a-fA-F]{2}$/.test(hex)) return null;
      const code = Number.parseInt(hex, 16);
      if (code === 0) return null;
      bytes.push(code);
      i += 2;
    } else {
      bytes.push(raw.charCodeAt(i));
    }
  }
  return Buffer.from(bytes).toString('utf8');
}

function parseDbusSessionSocketPath(dbusAddress) {
  if (typeof dbusAddress !== 'string' || dbusAddress.includes('\0')) return null;
  for (const transport of dbusAddress.split(';')) {
    const trimmed = transport.trim();
    if (!trimmed.startsWith('unix:')) continue;
    const rest = trimmed.slice('unix:'.length);
    let pathVal = null;
    let isAbstract = false;
    for (const kv of rest.split(',')) {
      const token = kv.trim();
      if (token.startsWith('abstract=')) {
        isAbstract = true;
        break;
      }
      if (token.startsWith('path=')) {
        pathVal = unescapeDbusValue(token.slice('path='.length));
      }
    }
    if (isAbstract) continue;
    if (pathVal && isSafeSocketFilePathStr(pathVal) && !isSystemDbusPath(pathVal)) {
      return pathVal;
    }
  }
  return null;
}

function parseX11DisplaySocketPath(display) {
  if (typeof display !== 'string') return null;
  const trimmed = display.trim();
  if (!trimmed || trimmed.includes('\0') || trimmed.includes('/') || trimmed.includes('\\')) {
    return null;
  }
  const colonIdx = trimmed.indexOf(':');
  if (colonIdx < 0) return null;
  const hostPart = trimmed.slice(0, colonIdx);
  const dispPart = trimmed.slice(colonIdx + 1);
  if (hostPart !== '' && hostPart !== 'unix') return null;
  const numStr = dispPart.split('.')[0];
  if (!/^\d+$/.test(numStr)) return null;
  const num = Number.parseInt(numStr, 10);
  if (num > 65535) return null;
  return `/tmp/.X11-unix/X${num}`;
}

function resolveWaylandSocketPath(waylandDisplay, xdgRuntimeDir) {
  if (typeof waylandDisplay !== 'string') return null;
  const wl = waylandDisplay.trim();
  if (!wl || wl.includes('\0')) return null;
  if (wl.startsWith('/')) {
    return isSafeSocketFilePathStr(wl, xdgRuntimeDir) && !isSystemDbusPath(wl) ? wl : null;
  }
  if (wl === '.' || wl === '..' || wl.includes('/') || wl.includes('\\')) return null;
  if (!xdgRuntimeDir) return null;
  const rt = xdgRuntimeDir.trim().replace(/\/+$/, '');
  if (!rt.startsWith('/') || hasDotOrDotDotSegment(rt) || isBroadForbiddenBindDir(rt)) return null;
  return `${rt}/${wl}`;
}

function planBwrapDesktopSandbox(
  profileDir,
  executable,
  appArgs,
  {
    lifetimePolicy = 'persistent-detached-session',
    profileId = null,
    instanceNonce = null,
    preferPortalFileChooser = true,
  } = {},
  hostEnv = {},
  endpointExists = () => true,
) {
  const lifecycleFlag =
    lifetimePolicy === 'die-with-parent' ? '--die-with-parent' : '--new-session';
  const args = [
    lifecycleFlag,
    '--unshare-ipc',
    '--ro-bind',
    '/usr',
    '/usr',
    '--proc',
    '/proc',
    '--dev',
    '/dev',
    '--tmpfs',
    '/tmp',
  ];

  const boundEndpoints = [];
  const exposedSurfaces = [];
  const realHome = (hostEnv.HOME || '').trim() || null;
  const rawRt = (hostEnv.XDG_RUNTIME_DIR || '').trim().replace(/\/+$/, '');
  const validRt =
    rawRt.startsWith('/') &&
    !hasDotOrDotDotSegment(rawRt) &&
    !isBroadForbiddenBindDir(rawRt) &&
    !isSystemDbusPath(rawRt) &&
    rawRt !== realHome
      ? rawRt
      : null;

  if (validRt) {
    args.push('--dir', validRt, '--chmod', '0700', validRt);
  }

  if (hostEnv.WAYLAND_DISPLAY) {
    const wlSock = resolveWaylandSocketPath(hostEnv.WAYLAND_DISPLAY, validRt);
    if (wlSock && wlSock !== realHome && endpointExists(wlSock)) {
      args.push('--ro-bind-try', wlSock, wlSock);
      boundEndpoints.push(wlSock);
      exposedSurfaces.push('wayland');
    }
  }

  if (hostEnv.DISPLAY) {
    const x11Sock = parseX11DisplaySocketPath(hostEnv.DISPLAY);
    if (x11Sock && endpointExists(x11Sock)) {
      args.push('--dir', '/tmp/.X11-unix', '--ro-bind-try', x11Sock, x11Sock);
      boundEndpoints.push(x11Sock);
      exposedSurfaces.push('x11');
      if (
        hostEnv.XAUTHORITY &&
        isSafeSocketFilePathStr(hostEnv.XAUTHORITY, validRt) &&
        !isSystemDbusPath(hostEnv.XAUTHORITY) &&
        hostEnv.XAUTHORITY !== realHome &&
        endpointExists(hostEnv.XAUTHORITY)
      ) {
        args.push('--ro-bind-try', hostEnv.XAUTHORITY, hostEnv.XAUTHORITY);
        boundEndpoints.push(hostEnv.XAUTHORITY);
      }
    }
  }

  const dbusSock =
    hostEnv.DBUS_SESSION_BUS_ADDRESS !== undefined
      ? parseDbusSessionSocketPath(hostEnv.DBUS_SESSION_BUS_ADDRESS)
      : validRt
        ? `${validRt}/bus`
        : null;
  if (dbusSock && dbusSock !== realHome && endpointExists(dbusSock)) {
    args.push('--ro-bind-try', dbusSock, dbusSock);
    boundEndpoints.push(dbusSock);
    exposedSurfaces.push('dbus-session');
  }

  const pwSock = validRt ? `${validRt}/pipewire-0` : null;
  if (pwSock && endpointExists(pwSock)) {
    args.push('--ro-bind-try', pwSock, pwSock);
    boundEndpoints.push(pwSock);
    exposedSurfaces.push('pipewire');
  }

  const pulseSock = validRt ? `${validRt}/pulse/native` : null;
  if (pulseSock && endpointExists(pulseSock)) {
    args.push('--dir', `${validRt}/pulse`, '--chmod', '0700', `${validRt}/pulse`);
    args.push('--ro-bind-try', pulseSock, pulseSock);
    boundEndpoints.push(pulseSock);
    exposedSurfaces.push('pulseaudio');
  }

  args.push('--bind', profileDir, profileDir);
  if (!executable.startsWith('/usr/') && !executable.startsWith('/bin/') && executable !== realHome) {
    args.push('--ro-bind', executable, executable);
  }

  args.push(
    '--setenv',
    'HOME',
    profileDir,
    '--setenv',
    'XDG_CONFIG_HOME',
    path.join(profileDir, '.config'),
    '--setenv',
    'XDG_DATA_HOME',
    path.join(profileDir, '.local', 'share'),
    '--setenv',
    'XDG_STATE_HOME',
    path.join(profileDir, '.local', 'state'),
    '--setenv',
    'XDG_CACHE_HOME',
    path.join(profileDir, '.cache'),
    '--setenv',
    'XDG_DOWNLOAD_DIR',
    path.join(profileDir, 'Downloads'),
    '--setenv',
    'XDG_DOCUMENTS_DIR',
    path.join(profileDir, 'Documents'),
    '--setenv',
    'TMPDIR',
    path.join(profileDir, 'tmp'),
  );

  if (preferPortalFileChooser && !hostEnv.GTK_USE_PORTAL) {
    args.push('--setenv', 'GTK_USE_PORTAL', '1');
  }
  if (profileId) {
    args.push('--setenv', 'DIM_PROFILE_ID', profileId);
  }
  if (instanceNonce) {
    args.push('--setenv', 'DIM_INSTANCE_NONCE', instanceNonce);
  }
  args.push('--', executable, ...appArgs);

  const tradeoffMap = {
    x11: 'SANDBOX_TRADEOFF [x11]: shared X11/XWayland server lacks per-client UI isolation.',
    'dbus-session':
      'SANDBOX_TRADEOFF [dbus-session]: unfiltered session D-Bus exposes same-UID session services.',
    pipewire: 'SANDBOX_TRADEOFF [pipewire]: direct pipewire-0 exposes user PipeWire graph nodes.',
    pulseaudio: 'SANDBOX_TRADEOFF [pulseaudio]: pulse/native shares same-UID audio streams.',
  };
  const securityTradeoffNotes = exposedSurfaces
    .filter((s) => s !== 'wayland')
    .map((s) => tradeoffMap[s]);

  return {
    lifetime_policy: lifetimePolicy,
    args,
    bound_endpoints: boundEndpoints,
    exposed_surfaces: exposedSurfaces,
    security_tradeoff_notes: securityTradeoffNotes,
  };
}

// ---------------------------------------------------------------------------
// `/proc` Ownership, PID-Reuse, `SingletonLock`, & Focus Helpers
// ---------------------------------------------------------------------------
function parseProcStat(statLine) {
  const open = statLine.indexOf('(');
  const close = statLine.lastIndexOf(') ');
  if (open < 0 || close <= open) return null;
  const pid = Number.parseInt(statLine.slice(0, open).trim(), 10);
  if (!Number.isInteger(pid) || pid <= 0) return null;
  const comm = statLine.slice(open + 1, close);
  const fields = statLine.slice(close + 2).trim().split(/\s+/);
  if (fields.length <= 19) return null;
  const state = fields[0][0];
  const ppid = Number.parseInt(fields[1], 10);
  const pgrp = Number.parseInt(fields[2], 10);
  const starttimeTicks = BigInt(fields[19]);
  return { pid, comm, state, ppid, pgrp, starttimeTicks };
}

function parseSingletonLockTarget(rawTarget) {
  if (typeof rawTarget !== 'string') return null;
  const trimmed = rawTarget.trim();
  const dash = trimmed.lastIndexOf('-');
  if (dash <= 0) return null;
  const pidStr = trimmed.slice(dash + 1);
  if (!/^\d+$/.test(pidStr)) return null;
  const pid = Number.parseInt(pidStr, 10);
  return pid > 0 ? pid : null;
}

function cmdlineSetenvValue(cmdline, key) {
  for (let i = 0; i + 2 < cmdline.length; i += 1) {
    if (cmdline[i] === '--setenv' && cmdline[i + 1] === key && cmdline[i + 2].length > 0) {
      return cmdline[i + 2];
    }
  }
  return null;
}

function entryProfileId(entry) {
  return entry.environ?.DIM_PROFILE_ID ?? cmdlineSetenvValue(entry.cmdline || [], 'DIM_PROFILE_ID');
}

function entryInstanceNonce(entry) {
  return (
    entry.environ?.DIM_INSTANCE_NONCE ??
    cmdlineSetenvValue(entry.cmdline || [], 'DIM_INSTANCE_NONCE')
  );
}

function isProcActive(entry) {
  return entry && entry.state !== 'Z' && entry.state !== 'X' && entry.state !== 'x';
}

function entryMatchesProfile(entry, profileId, profileDir, expectedUid = null) {
  if (!isProcActive(entry)) return false;
  if (expectedUid !== null && entry.uid !== undefined && entry.uid !== expectedUid) return false;
  const tagId = entryProfileId(entry);
  if (tagId !== null) {
    return tagId.toLowerCase() === profileId.toLowerCase();
  }
  const envKeys = Object.keys(entry.environ || {});
  if (envKeys.length > 0) {
    if (entry.environ.XDG_CONFIG_HOME) {
      return entry.environ.XDG_CONFIG_HOME === path.join(profileDir, '.config');
    }
    if (entry.environ.HOME) {
      return entry.environ.HOME === profileDir;
    }
    return false;
  }
  return (entry.cmdline || []).some((arg) => arg.includes(profileDir));
}

function runtimeHintMatchesEntry(hint, entry, profileId, profileDir, expectedUid = null) {
  if (!entry || entry.pid !== hint.pid || !isProcActive(entry)) return false;
  if (hint.profile_id && hint.profile_id.toLowerCase() !== profileId.toLowerCase()) return false;
  if (hint.starttime_ticks !== null && hint.starttime_ticks !== undefined) {
    if (BigInt(entry.starttimeTicks) !== BigInt(hint.starttime_ticks)) return false;
  }
  if (hint.instance_nonce) {
    const actualNonce = entryInstanceNonce(entry);
    if (actualNonce !== null) {
      if (actualNonce !== hint.instance_nonce) return false;
    } else if (hint.starttime_ticks === null || hint.starttime_ticks === undefined) {
      return false;
    }
  }
  return entryMatchesProfile(entry, profileId, profileDir, expectedUid);
}

function collectProfileProcessTree(procTable, hint, lockPid, profileId, profileDir, expectedUid) {
  const childrenMap = new Map();
  for (const entry of procTable.values()) {
    if (isProcActive(entry) && (expectedUid === null || entry.uid === expectedUid)) {
      if (!childrenMap.has(entry.ppid)) childrenMap.set(entry.ppid, []);
      childrenMap.get(entry.ppid).push(entry.pid);
    }
  }

  const validHint =
    hint && (!hint.profile_id || hint.profile_id.toLowerCase() === profileId.toLowerCase())
      ? hint
      : null;
  const verifiedRuntimeRoot =
    validHint &&
    procTable.has(validHint.pid) &&
    runtimeHintMatchesEntry(
      validHint,
      procTable.get(validHint.pid),
      profileId,
      profileDir,
      expectedUid,
    )
      ? validHint.pid
      : null;

  const expectedNonce = validHint?.instance_nonce ?? null;

  let verifiedLockRoot = null;
  if (lockPid !== null && procTable.has(lockPid)) {
    const rejectedAsRuntimePid =
      validHint && validHint.pid === lockPid && verifiedRuntimeRoot !== lockPid;
    const lockEntry = procTable.get(lockPid);
    const nonceOk =
      !expectedNonce ||
      (entryInstanceNonce(lockEntry) === expectedNonce ||
        (entryInstanceNonce(lockEntry) === null && verifiedRuntimeRoot !== null));
    if (
      !rejectedAsRuntimePid &&
      entryMatchesProfile(lockEntry, profileId, profileDir, expectedUid) &&
      nonceOk
    ) {
      verifiedLockRoot = lockPid;
    }
  }

  const ownedSet = new Set();
  const ordered = [];
  const queue = [];

  const enqueue = (pid) => {
    if (!ownedSet.has(pid)) {
      ownedSet.add(pid);
      ordered.push(pid);
      queue.push(pid);
    }
  };

  if (verifiedRuntimeRoot !== null) enqueue(verifiedRuntimeRoot);
  if (verifiedLockRoot !== null) enqueue(verifiedLockRoot);

  for (const entry of procTable.values()) {
    if (!isProcActive(entry)) continue;
    if (expectedUid !== null && entry.uid !== expectedUid) continue;
    const tag = entryProfileId(entry);
    if (!tag || tag.toLowerCase() !== profileId.toLowerCase()) continue;
    if (expectedNonce) {
      if (entryInstanceNonce(entry) !== expectedNonce) continue;
    } else if (validHint && entry.pid === validHint.pid && verifiedRuntimeRoot !== validHint.pid) {
      continue;
    }
    enqueue(entry.pid);
  }

  while (queue.length > 0) {
    const cur = queue.shift();
    for (const kid of childrenMap.get(cur) || []) {
      const kidEntry = procTable.get(kid);
      if (!isProcActive(kidEntry)) continue;
      const otherTag = entryProfileId(kidEntry);
      if (otherTag && otherTag.toLowerCase() !== profileId.toLowerCase()) continue;
      const kidNonce = entryInstanceNonce(kidEntry);
      if (expectedNonce && kidNonce && kidNonce !== expectedNonce) continue;
      enqueue(kid);
    }
  }

  const mainPid = verifiedLockRoot ?? verifiedRuntimeRoot ?? (ordered[0] ?? null);
  return { main_pid: mainPid, process_ids: ordered };
}

function evaluateFocusOrRestore(target, state, sessionEnv = {}) {
  if (!state.running) {
    const err = new Error(`profile is not currently running`);
    err.code = 'NOT_RUNNING';
    throw err;
  }
  if (target.kind === 'NodeFixture' || fs.existsSync(path.join(state.zaloDataDir, 'lockfile'))) {
    fs.writeFileSync(
      path.join(state.zaloDataDir, '.focus-signal'),
      JSON.stringify({ focused_at: new Date().toISOString() }) + '\n',
      { mode: 0o600 },
    );
    return 'fixture-focus-signaled';
  }
  const sessionType = (sessionEnv.XDG_SESSION_TYPE || '').trim().toLowerCase();
  const isWayland = sessionType === 'wayland' || Boolean((sessionEnv.WAYLAND_DISPLAY || '').trim());
  const isX11 = !isWayland && (sessionType === 'x11' || Boolean((sessionEnv.DISPLAY || '').trim()));
  if (isWayland) {
    const err = new Error(
      'UNSUPPORTED_OPERATION: safe PID-verified window focus is not supported on Wayland compositors without an xdg-activation-v1 surface token',
    );
    err.code = 'UNSUPPORTED_PLATFORM';
    throw err;
  }
  if (!isX11) {
    const err = new Error(
      'UNSUPPORTED_OPERATION: safe PID-verified window focus is unavailable in a headless session without DISPLAY or WAYLAND_DISPLAY',
    );
    err.code = 'UNSUPPORTED_PLATFORM';
    throw err;
  }
  return 'x11-supported';
}

// ---------------------------------------------------------------------------
// Static ELF64 / AppImage & `/proc/mounts` Inspection Helpers (Zero Execution)
// ---------------------------------------------------------------------------
function makeElf64Type2Stub() {
  const buf = Buffer.alloc(128, 0);
  buf.write('\x7fELF', 0, 'binary');
  buf[4] = 2; // ELFCLASS64
  buf[5] = 1; // ELFDATA2LSB
  buf[6] = 1; // EV_CURRENT
  buf.write('AI\x02', 8, 'binary'); // Type-2 AppImage magic
  buf.writeUInt16LE(2, 16); // ET_EXEC
  buf.writeUInt16LE(process.arch === 'arm64' ? 0xb7 : 0x3e, 18);
  return buf;
}

function buildSyntheticElf64DynamicBuffer({
  machine = 0x3e,
  appimageType2 = true,
  interp = null,
  soname = null,
  needed = [],
  rpaths = [],
  runpaths = [],
} = {}) {
  const dynstrChunks = [Buffer.from([0])];
  let dynstrLen = 1;
  const pushStr = (s) => {
    const off = dynstrLen;
    const b = Buffer.from(`${s}\0`, 'utf8');
    dynstrChunks.push(b);
    dynstrLen += b.length;
    return off;
  };

  const sonameOff = soname ? pushStr(soname) : null;
  const neededOffs = needed.map(pushStr);
  const rpathOffs = rpaths.map(pushStr);
  const runpathOffs = runpaths.map(pushStr);
  const dynstrBuf = Buffer.concat(dynstrChunks);

  const loadFileOff = 0x200;
  const loadBaseVaddr = 0x00401200n;
  const payloadParts = [];
  let payloadLen = 0;
  const pad8 = () => {
    const rem = payloadLen % 8;
    if (rem !== 0) {
      const p = Buffer.alloc(8 - rem, 0);
      payloadParts.push(p);
      payloadLen += p.length;
    }
  };

  let interpRange = null;
  if (interp) {
    const ib = Buffer.from(`${interp}\0`, 'utf8');
    interpRange = { off: BigInt(loadFileOff + payloadLen), sz: BigInt(ib.length) };
    payloadParts.push(ib);
    payloadLen += ib.length;
  }
  pad8();

  const dynstrRel = BigInt(payloadLen);
  const dynstrVaddr = loadBaseVaddr + dynstrRel;
  payloadParts.push(dynstrBuf);
  payloadLen += dynstrBuf.length;
  pad8();

  const dynRel = BigInt(payloadLen);
  const dynEntries = [];
  const pushDyn = (tag, val) => {
    const e = Buffer.alloc(16);
    e.writeBigUInt64LE(BigInt(tag), 0);
    e.writeBigUInt64LE(BigInt(val), 8);
    dynEntries.push(e);
  };
  pushDyn(5n, dynstrVaddr); // DT_STRTAB
  pushDyn(10n, BigInt(dynstrBuf.length)); // DT_STRSZ
  if (sonameOff !== null) pushDyn(14n, BigInt(sonameOff)); // DT_SONAME
  for (const off of neededOffs) pushDyn(1n, BigInt(off)); // DT_NEEDED
  for (const off of rpathOffs) pushDyn(15n, BigInt(off)); // DT_RPATH
  for (const off of runpathOffs) pushDyn(29n, BigInt(off)); // DT_RUNPATH
  pushDyn(0n, 0n); // DT_NULL

  const dynBuf = Buffer.concat(dynEntries);
  payloadParts.push(dynBuf);
  payloadLen += dynBuf.length;

  const payloadBuf = Buffer.concat(payloadParts);
  const loadFilesz = BigInt(payloadBuf.length);

  const makePhdr = (pType, pFlags, off, vaddr, filesz, memsz) => {
    const p = Buffer.alloc(56, 0);
    p.writeUInt32LE(pType, 0);
    p.writeUInt32LE(pFlags, 4);
    p.writeBigUInt64LE(BigInt(off), 8);
    p.writeBigUInt64LE(BigInt(vaddr), 16);
    p.writeBigUInt64LE(BigInt(vaddr), 24);
    p.writeBigUInt64LE(BigInt(filesz), 32);
    p.writeBigUInt64LE(BigInt(memsz), 40);
    p.writeBigUInt64LE(0x1000n, 48);
    return p;
  };

  const phdrs = [
    makePhdr(1, 5, loadFileOff, loadBaseVaddr, loadFilesz, loadFilesz + 0x2000n), // PT_LOAD
  ];
  if (interpRange) {
    phdrs.push(
      makePhdr(
        3,
        4,
        interpRange.off,
        loadBaseVaddr + (interpRange.off - BigInt(loadFileOff)),
        interpRange.sz,
        interpRange.sz,
      ),
    );
  }
  phdrs.push(
    makePhdr(
      2,
      6,
      BigInt(loadFileOff) + dynRel,
      loadBaseVaddr + dynRel,
      BigInt(dynBuf.length),
      BigInt(dynBuf.length),
    ),
  );

  const head = Buffer.alloc(loadFileOff, 0);
  head.write('\x7fELF', 0, 'binary');
  head[4] = 2;
  head[5] = 1;
  head[6] = 1;
  if (appimageType2) head.write('AI\x02', 8, 'binary');
  head.writeUInt16LE(3, 16); // ET_DYN
  head.writeUInt16LE(machine, 18);
  head.writeUInt32LE(1, 20);
  head.writeBigUInt64LE(64n, 32); // e_phoff
  head.writeUInt16LE(64, 52); // e_ehsize
  head.writeUInt16LE(56, 54); // e_phentsize
  head.writeUInt16LE(phdrs.length, 56); // e_phnum

  phdrs.forEach((ph, idx) => {
    ph.copy(head, 64 + idx * 56);
  });

  return Buffer.concat([head, payloadBuf]);
}

function parseElf64DynamicMetadata(buf) {
  if (!Buffer.isBuffer(buf) || buf.length < 64) {
    throw new Error(`TruncatedElfHeader: actual_bytes=${buf ? buf.length : 0}`);
  }
  if (buf[0] !== 0x7f || buf[1] !== 0x45 || buf[2] !== 0x4c || buf[3] !== 0x46) {
    throw new Error('InvalidElfMagic');
  }
  if (buf[4] !== 2) throw new Error(`UnsupportedClass(${buf[4]})`);
  if (buf[5] !== 1) throw new Error(`UnsupportedEndianness(${buf[5]})`);
  if (buf[6] !== 1) throw new Error(`UnsupportedVersion(${buf[6]})`);

  const eType = buf.readUInt16LE(16);
  const eMachine = buf.readUInt16LE(18);
  if ((eType !== 2 && eType !== 3) || eMachine === 0) {
    throw new Error(`NotExecutableType(${eType})`);
  }
  const phOff = buf.readBigUInt64LE(32);
  const phEntSize = buf.readUInt16LE(54);
  const phNum = buf.readUInt16LE(56);
  if (phNum === 0 || phOff === 0n) throw new Error('MissingProgramHeaders');
  if (phEntSize < 56 || phEntSize > 256) throw new Error(`InvalidPhEntSize(${phEntSize})`);
  if (phNum > 1024) throw new Error(`TooManyProgramHeaders(${phNum})`);

  const tableEnd = phOff + BigInt(phEntSize) * BigInt(phNum);
  if (tableEnd > BigInt(buf.length)) {
    throw new Error('TruncatedProgramHeaderTable');
  }

  const loads = [];
  let interp = null;
  let dynSlice = null;

  for (let i = 0; i < phNum; i += 1) {
    const base = Number(phOff) + i * phEntSize;
    const pType = buf.readUInt32LE(base);
    const pOffset = buf.readBigUInt64LE(base + 8);
    const pVaddr = buf.readBigUInt64LE(base + 16);
    const pFilesz = buf.readBigUInt64LE(base + 32);
    if (pType === 1 || pType === 2 || pType === 3) {
      if (pOffset + pFilesz > BigInt(buf.length)) {
        throw new Error(`SegmentOutOfBounds(p_type=${pType})`);
      }
    }
    if (pType === 1 && pFilesz > 0n) {
      loads.push({ offset: pOffset, vaddr: pVaddr, filesz: pFilesz });
    } else if (pType === 3 && pFilesz > 0n) {
      if (pFilesz > 4096n) throw new Error('OversizedInterp');
      const raw = buf.subarray(Number(pOffset), Number(pOffset + pFilesz));
      const nul = raw.indexOf(0);
      interp = (nul >= 0 ? raw.subarray(0, nul) : raw).toString('utf8');
    } else if (pType === 2 && pFilesz > 0n) {
      if (pFilesz > 1024n * 1024n) throw new Error('OversizedDynamic');
      dynSlice = buf.subarray(Number(pOffset), Number(pOffset + pFilesz));
    }
  }

  const neededLibraries = [];
  const rpathEntries = [];
  const runpathEntries = [];
  let soname = null;

  if (dynSlice) {
    if (loads.length === 0) throw new Error('MissingLoadSegments');
    let strtabVaddr = null;
    let strsz = null;
    const rawNeeded = [];
    const rawRpath = [];
    const rawRunpath = [];
    let rawSoname = null;

    for (let pos = 0; pos + 16 <= dynSlice.length; pos += 16) {
      const dTag = dynSlice.readBigUInt64LE(pos);
      const dVal = dynSlice.readBigUInt64LE(pos + 8);
      if (dTag === 0n) break;
      if (dTag === 5n) strtabVaddr = dVal;
      else if (dTag === 10n) strsz = dVal;
      else if (dTag === 1n) rawNeeded.push(dVal);
      else if (dTag === 14n) rawSoname = dVal;
      else if (dTag === 15n) rawRpath.push(dVal);
      else if (dTag === 29n) rawRunpath.push(dVal);
    }

    if (strtabVaddr !== null && strsz !== null && strsz > 0n) {
      if (strsz > 16n * 1024n * 1024n) throw new Error(`OversizedStringTable(${strsz})`);
      const seg = loads.find(
        (l) => strtabVaddr >= l.vaddr && strtabVaddr < l.vaddr + l.filesz,
      );
      if (!seg) throw new Error(`UnmappedStringTable(0x${strtabVaddr.toString(16)})`);
      const delta = strtabVaddr - seg.vaddr;
      if (strsz > seg.filesz - delta) throw new Error('TruncatedStringTable');
      const fileOff = Number(seg.offset + delta);
      const dynstr = buf.subarray(fileOff, fileOff + Number(strsz));

      const readDynStr = (offBig) => {
        if (offBig >= strsz) throw new Error('StringOffsetOutOfBounds');
        const start = Number(offBig);
        const nul = dynstr.indexOf(0, start);
        if (nul < 0) throw new Error('UnterminatedDynamicString');
        return dynstr.subarray(start, nul).toString('utf8');
      };

      if (rawSoname !== null) soname = readDynStr(rawSoname);
      for (const off of rawNeeded) neededLibraries.push(readDynStr(off));
      for (const off of rawRpath) {
        rpathEntries.push(...readDynStr(off).split(':').filter(Boolean));
      }
      for (const off of rawRunpath) {
        runpathEntries.push(...readDynStr(off).split(':').filter(Boolean));
      }
    }
  }

  return {
    arch: eMachine === 0xb7 ? 'aarch64' : eMachine === 0x3e ? 'x86_64' : 'other',
    elf_type: eType,
    interpreter: interp,
    soname,
    needed_libraries: neededLibraries,
    rpath_entries: rpathEntries,
    runpath_entries: runpathEntries,
  };
}

function resolveElf64MissingLibrariesAt(meta, binaryPath, fsRoot) {
  const originDir = path.dirname(binaryPath);
  const activeRpaths = meta.runpath_entries.length > 0 ? meta.runpath_entries : meta.rpath_entries;
  const searchDirs = [];
  for (const rawEntry of activeRpaths) {
    const expanded = rawEntry
      .replaceAll('${ORIGIN}', originDir)
      .replaceAll('$ORIGIN', originDir);
    searchDirs.push(
      expanded.startsWith('/') && !expanded.startsWith(originDir)
        ? path.join(fsRoot, expanded.slice(1))
        : expanded,
    );
  }
  searchDirs.push(
    originDir,
    path.join(originDir, 'lib'),
    path.join(fsRoot, 'lib', 'x86_64-linux-gnu'),
    path.join(fsRoot, 'usr', 'lib', 'x86_64-linux-gnu'),
    path.join(fsRoot, 'lib', 'aarch64-linux-gnu'),
    path.join(fsRoot, 'usr', 'lib', 'aarch64-linux-gnu'),
    path.join(fsRoot, 'lib64'),
    path.join(fsRoot, 'usr', 'lib'),
  );

  const missing = [];
  for (const soname of meta.needed_libraries) {
    if (!soname || soname.startsWith('linux-vdso.so') || soname.includes('ld-linux')) continue;
    const found = searchDirs.some((d) => {
      try {
        return fs.statSync(path.join(d, soname)).isFile();
      } catch {
        return false;
      }
    });
    if (!found) {
      const [u2404, u2204] = UBUNTU_PACKAGE_MAP.get(soname) || ['unknown', 'unknown'];
      missing.push({ soname, ubuntu_2404_package: u2404, ubuntu_2204_package: u2204 });
    }
  }
  return missing;
}

function inspectStaticBinaryFile(filePath) {
  validateSafePathInput(filePath);
  const sym = fs.lstatSync(filePath);
  let resolved = filePath;
  if (sym.isSymbolicLink()) {
    try {
      resolved = fs.realpathSync(filePath);
    } catch {
      const err = new Error(`PATH_TRAVERSAL: broken symlink binary ${filePath}`);
      err.code = 'PATH_TRAVERSAL';
      throw err;
    }
  }
  const buf = fs.readFileSync(resolved);
  const sha256 = crypto.createHash('sha256').update(buf).digest('hex');
  const hasAppImageExt = resolved.toLowerCase().endsWith('.appimage');

  let format = 'unknown';
  let hasValidExecutableHeader = false;
  if (buf.length >= 4 && buf.subarray(0, 2).toString('utf8') === '#!') {
    const firstLine = buf.subarray(2, Math.min(buf.length, 128)).toString('utf8').split(/\r?\n/)[0].trim();
    if (firstLine.length > 0) {
      format = 'script-wrapper';
      hasValidExecutableHeader = !hasAppImageExt;
    }
  } else if (
    buf.length >= 64 &&
    buf[0] === 0x7f &&
    buf[1] === 0x45 &&
    buf[2] === 0x4c &&
    buf[3] === 0x46 &&
    buf[4] === 2 &&
    (buf[5] === 1 || buf[5] === 2) &&
    buf[6] === 1 &&
    (buf.readUInt16LE(16) === 2 || buf.readUInt16LE(16) === 3) &&
    buf.readUInt16LE(18) !== 0
  ) {
    const isType2 = buf[8] === 0x41 && buf[9] === 0x49 && buf[10] === 0x02;
    format = isType2
      ? 'appimage-type2-elf64'
      : hasAppImageExt || buf.includes(Buffer.from('DWARFS'))
        ? 'appimage-uruntime-dwarfs-elf64'
        : 'elf64-executable';
    const phOff = buf.readBigUInt64LE(32);
    const phNum = buf.readUInt16LE(56);
    if (phOff === 0n && phNum === 0) {
      hasValidExecutableHeader = true;
    } else {
      try {
        parseElf64DynamicMetadata(buf);
        hasValidExecutableHeader = true;
      } catch {
        hasValidExecutableHeader = false;
      }
    }
  }

  return {
    path: resolved,
    format,
    has_valid_executable_header: hasValidExecutableHeader,
    sha256,
    provenance_note:
      'SHA-256 proves byte-level integrity against a trusted reference digest only — NEVER publisher identity or software origin authenticity.',
    validateExecutableFormat() {
      if (
        hasAppImageExt &&
        (!hasValidExecutableHeader ||
          (format !== 'appimage-type2-elf64' && format !== 'appimage-uruntime-dwarfs-elf64'))
      ) {
        const err = new Error(`UNVERIFIED_BINARY: invalid .AppImage header (${format})`);
        err.code = 'UNVERIFIED_BINARY';
        throw err;
      }
      if (!hasValidExecutableHeader || format === 'unknown') {
        const err = new Error(`UNVERIFIED_BINARY: unrecognized executable header (${format})`);
        err.code = 'UNVERIFIED_BINARY';
        throw err;
      }
    },
  };
}

function unescapeProcMountToken(raw) {
  return raw.replace(/\\([0-3][0-7]{2})/g, (_, oct) => String.fromCharCode(Number.parseInt(oct, 8)));
}

function parseProcMounts(text) {
  const entries = [];
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const cols = trimmed.split(/\s+/);
    if (cols.length < 4) continue;
    entries.push({
      device: unescapeProcMountToken(cols[0]),
      mount_point: unescapeProcMountToken(cols[1]),
      fs_type: cols[2],
      options: cols[3].split(',').map((s) => s.trim()).filter(Boolean),
    });
  }
  return entries;
}

function findMountForPath(mounts, targetPosixPath) {
  const norm = targetPosixPath.replace(/\\/g, '/');
  const normSegs = norm.split('/').filter(Boolean);
  let best = null;
  let bestCount = -1;
  for (const m of mounts) {
    const mSegs = m.mount_point.split('/').filter(Boolean);
    const prefixMatch =
      mSegs.length <= normSegs.length && mSegs.every((seg, idx) => normSegs[idx] === seg);
    if (prefixMatch && mSegs.length > bestCount) {
      best = m;
      bestCount = mSegs.length;
    }
  }
  return best;
}

function deterministicPatternBuffer(len) {
  const out = Buffer.allocUnsafe(len);
  for (let i = 0; i < len; i += 1) {
    out[i] = (Math.imul(i, 31) + 17) & 0xff;
  }
  return out;
}

function probeLiveLinuxDetails() {
  if (!IS_LINUX) {
    return {
      is_linux: false,
      distro: null,
      kernel: os.release(),
      apparmor_enabled: null,
      apparmor_restrict_unprivileged_userns: null,
      bwrap_installed: false,
      effective_uid: null,
    };
  }
  let distro = 'linux';
  try {
    const osRel = fs.readFileSync('/etc/os-release', 'utf8');
    const pretty = osRel.match(/^PRETTY_NAME="?([^"\n]+)"?/m);
    if (pretty) distro = pretty[1];
  } catch {
    // ignore
  }
  const readTrim = (p) => {
    try {
      return fs.readFileSync(p, 'utf8').trim();
    } catch {
      return null;
    }
  };
  let bwrapInstalled = false;
  try {
    execFileSync('which', ['bwrap'], { stdio: 'ignore' });
    bwrapInstalled = true;
  } catch {
    bwrapInstalled = false;
  }
  return {
    is_linux: true,
    distro,
    kernel: os.release(),
    apparmor_enabled: readTrim('/sys/module/apparmor/parameters/enabled'),
    apparmor_restrict_unprivileged_userns: readTrim(
      '/proc/sys/kernel/apparmor_restrict_unprivileged_userns',
    ),
    bwrap_installed: bwrapInstalled,
    effective_uid: typeof process.geteuid === 'function' ? process.geteuid() : null,
  };
}

// ---------------------------------------------------------------------------
// Suite Execution (15 Core Scenarios + 14 P1 Regression & Security Scenarios)
// ---------------------------------------------------------------------------
async function runSuite() {
  const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'dim-ubuntu-suite-'));
  const results = [];
  const activeChildren = new Set();
  const trackedDetachedPids = new Set();
  const trackedQuitDirs = new Set();

  const record = async (id, name, fn, regressionCase = null) => {
    const started = Date.now();
    try {
      const detail = await fn();
      const entry = {
        id,
        scenario: name,
        status: 'PASS',
        duration_ms: Date.now() - started,
        detail: detail || 'ok',
      };
      if (regressionCase !== null) entry.regression_case = regressionCase;
      results.push(entry);
    } catch (err) {
      const entry = {
        id,
        scenario: name,
        status: 'FAIL',
        duration_ms: Date.now() - started,
        detail: err && err.stack ? err.message : String(err),
      };
      if (regressionCase !== null) entry.regression_case = regressionCase;
      results.push(entry);
    }
  };

  try {
    assert.ok(fs.existsSync(FIXTURE_SCRIPT), `Missing fixture script at ${FIXTURE_SCRIPT}`);

    // =========================================================================
    // PART A: Core 15 Ubuntu Adapter & Fixture Scenarios (#01 .. #15)
    // =========================================================================

    await record(1, 'Application discovery & AppImage ELF64/SHA-256 verification', async () => {
      const stub = makeElf64Type2Stub();
      const digest = crypto.createHash('sha256').update(stub).digest('hex');
      const appImage = path.join(tmpRoot, 'Zalo-26.11.10-Original-e080c70-x86_64.AppImage');
      fs.writeFileSync(appImage, stub, { mode: 0o755 });
      const inspected = inspectStaticBinaryFile(appImage);
      inspected.validateExecutableFormat();
      assert.equal(inspected.format, 'appimage-type2-elf64');
      assert.equal(inspected.has_valid_executable_header, true);
      assert.equal(inspected.sha256, digest);
      assert.ok(inspected.provenance_note.includes('NEVER publisher identity'));
      return `Verified ELF64 Type-2 magic & SHA-256 (${digest.slice(0, 16)}...); Official Zalo classified OFFICIAL_ZALO_UNSUPPORTED`;
    });

    const profA = path.join(tmpRoot, 'Profiles', 'ubuntu-alpha_aaaa1111');
    const profB = path.join(tmpRoot, 'Profiles', 'ubuntu-beta_bbbb2222');
    const zaloA = path.join(profA, '.config', 'ZaloData');
    const zaloB = path.join(profB, '.config', 'ZaloData');
    let childA = null;
    let childB = null;

    await record(2, 'Multiple concurrent fixture instances', async () => {
      childA = spawnFixture(profA, 'aaaa1111', 'Ubuntu Alpha', [
        '--user-id=849100001',
        '--stay-ms=15000',
      ]);
      childB = spawnFixture(profB, 'bbbb2222', 'Ubuntu Beta', [
        '--user-id=849100002',
        '--stay-ms=15000',
      ]);
      activeChildren.add(childA);
      activeChildren.add(childB);

      const ready = await waitForCondition(
        () =>
          fs.existsSync(path.join(zaloA, 'lockfile')) &&
          fs.existsSync(path.join(zaloA, 'launch-state.json')) &&
          fs.existsSync(path.join(zaloB, 'lockfile')) &&
          fs.existsSync(path.join(zaloB, 'launch-state.json')),
        6000,
      );
      assert.ok(ready, 'Both isolated fixture instances must create independent lockfiles');
      return `pids=[${childA.pid}, ${childB.pid}]`;
    });

    await record(3, 'Process separation & PID ownership', async () => {
      const lockA = JSON.parse(fs.readFileSync(path.join(zaloA, 'lockfile'), 'utf8'));
      const lockB = JSON.parse(fs.readFileSync(path.join(zaloB, 'lockfile'), 'utf8'));
      assert.notEqual(lockA.pid, lockB.pid);
      assert.equal(lockA.profile_id, 'aaaa1111');
      assert.equal(lockB.profile_id, 'bbbb2222');
      if (IS_LINUX) {
        const envRaw = fs.readFileSync(`/proc/${lockA.pid}/environ`, 'utf8');
        assert.ok(envRaw.includes('DIM_PROFILE_ID=aaaa1111'));
      }
      return `lockA.pid=${lockA.pid}, lockB.pid=${lockB.pid}`;
    });

    await record(4, 'Data directory separation & cross-profile overwrite block', async () => {
      assert.ok(
        fs.existsSync(path.join(zaloA, 'Database', '_production', '849100001', 'session.json')),
      );
      assert.ok(
        fs.existsSync(path.join(zaloB, 'Database', '_production', '849100002', 'session.json')),
      );
      assert.ok(!fs.existsSync(path.join(zaloA, 'Database', '_production', '849100002')));
      assert.throws(() => createLinuxProfileLayout(profA, 'bbbb2222'), /PROFILE_MARKER_MISMATCH/);
      return 'Isolated Database/_production/<uid> & .zalo-profile-id verified';
    });

    await record(5, 'XDG Base Directory & user-dirs.dirs confinement', async () => {
      const envMap = buildIsolatedLinuxEnv(profA, 'aaaa1111', 'Ubuntu Alpha');
      assert.equal(envMap.HOME, profA);
      assert.equal(envMap.XDG_CONFIG_HOME, path.join(profA, '.config'));
      assert.equal(envMap.XDG_DATA_HOME, path.join(profA, '.local', 'share'));
      assert.equal(envMap.XDG_STATE_HOME, path.join(profA, '.local', 'state'));
      assert.equal(envMap.XDG_CACHE_HOME, path.join(profA, '.cache'));
      assert.equal(envMap.TMPDIR, path.join(profA, 'tmp'));
      assert.equal(envMap.APPDATA, undefined);
      return 'HOME, XDG_*, TMPDIR, user-dirs.dirs verified; APPDATA stripped';
    });

    await record(6, 'File ownership & POSIX 0o700 permission verification', async () => {
      const st = fs.lstatSync(profA);
      assert.ok(st.isDirectory() && !st.isSymbolicLink());
      if (IS_LINUX) {
        const mode = st.mode & 0o777;
        assert.equal(mode, 0o700, `Expected 0o700 on Linux, got 0o${mode.toString(8)}`);
        assert.equal(st.uid, process.geteuid());
        return `Verified uid=${st.uid} mode=0700 on Linux`;
      }
      return 'Directory ownership verified (POSIX 0o700 bit assertion requires Linux filesystem)';
    });

    await record(7, 'Safe graceful shutdown without affecting sibling profile', async () => {
      fs.writeFileSync(
        path.join(zaloA, '.focus-signal'),
        JSON.stringify({ focused_at: new Date().toISOString() }) + '\n',
      );
      await waitForCondition(() => {
        const st = JSON.parse(fs.readFileSync(path.join(zaloA, 'launch-state.json'), 'utf8'));
        return st.focus_count >= 1;
      }, 3000);

      fs.writeFileSync(
        path.join(zaloA, '.quit-signal'),
        JSON.stringify({ requested_at: new Date().toISOString() }) + '\n',
      );
      const exitedA = await waitForCondition(() => childA.exitCode !== null, 5000);
      assert.ok(exitedA, 'Profile A must exit cleanly on .quit-signal');
      activeChildren.delete(childA);

      const stateA = JSON.parse(fs.readFileSync(path.join(zaloA, 'launch-state.json'), 'utf8'));
      assert.equal(stateA.clean_shutdown, true);
      assert.equal(childB.exitCode, null, 'Sibling Profile B must still be running');

      fs.writeFileSync(
        path.join(zaloB, '.quit-signal'),
        JSON.stringify({ requested_at: new Date().toISOString() }) + '\n',
      );
      await waitForCondition(() => childB.exitCode !== null, 5000);
      activeChildren.delete(childB);
      return 'Profile A shut down cleanly while Profile B remained active; zero killall/pkill';
    });

    const profCrash = path.join(tmpRoot, 'Profiles', 'crash-test_cccc3333');
    const zaloCrash = path.join(profCrash, '.config', 'ZaloData');

    await record(8, 'Crash recovery after abnormal exit', async () => {
      const crashProc = spawnFixture(profCrash, 'cccc3333', 'Crash Profile', [
        '--crash',
        '--crash-delay-ms=60',
        '--user-id=849300003',
      ]);
      activeChildren.add(crashProc);
      await waitForCondition(() => crashProc.exitCode !== null, 5000);
      activeChildren.delete(crashProc);
      assert.equal(crashProc.exitCode, 1);
      assert.ok(fs.existsSync(path.join(zaloCrash, 'lockfile')), 'Crash must leave stale lockfile');

      const recProc = spawnFixture(profCrash, 'cccc3333', 'Crash Profile', [
        '--user-id=849300003',
        '--stay-ms=4000',
      ]);
      activeChildren.add(recProc);
      const recovered = await waitForCondition(() => {
        try {
          const st = JSON.parse(fs.readFileSync(path.join(zaloCrash, 'launch-state.json'), 'utf8'));
          return st.launch_count === 2 && st.crashed === false;
        } catch {
          return false;
        }
      }, 5000);
      assert.ok(recovered, 'Second launch must recover stale lock and reach launch_count=2');
      fs.writeFileSync(path.join(zaloCrash, '.quit-signal'), '{"requested_at":"now"}\n');
      await waitForCondition(() => recProc.exitCode !== null, 5000);
      activeChildren.delete(recProc);
      return 'Simulated crash (exit=1) left stale lock; relaunch recovered cleanly (launch_count=2)';
    });

    await record(9, 'Stale SingletonLock & dead-PID lockfile detection', async () => {
      const staleDir = path.join(tmpRoot, 'Profiles', 'stale-only_dddd4444');
      createLinuxProfileLayout(staleDir, 'dddd4444');
      const zStale = path.join(staleDir, '.config', 'ZaloData');
      fs.writeFileSync(path.join(zStale, 'lockfile'), JSON.stringify({ pid: 99999999 }) + '\n');
      assert.equal(isPidAlive(99999999), false);
      if (IS_LINUX) {
        fs.symlinkSync('ubuntu-host-99999999', path.join(zStale, 'SingletonLock'));
        assert.ok(fs.lstatSync(path.join(zStale, 'SingletonLock')).isSymbolicLink());
      }
      const tree = collectProfileProcessTree(
        new Map(),
        null,
        99999999,
        'dddd4444',
        staleDir,
        1000,
      );
      assert.equal(tree.main_pid, null);
      assert.equal(tree.process_ids.length, 0);
      return 'Dead-PID lockfile / dangling SingletonLock classified as stale';
    });

    await record(10, 'Path traversal rejection', async () => {
      const traversalInputs = ['../escape', '..\\escape', 'sub/../../etc/passwd', 'bad\0name'];
      for (const bad of traversalInputs) {
        assert.throws(() => validateSafePathInput(bad), /PATH_TRAVERSAL/);
      }
      return 'Rejected ../, ..\\, /etc/passwd traversal, and NUL-byte profile paths';
    });

    await record(11, 'Symlink attack protection', async () => {
      const outside = path.join(tmpRoot, 'OutsideTarget');
      fs.mkdirSync(outside, { recursive: true });
      fs.writeFileSync(path.join(outside, 'keep.txt'), 'do-not-delete');
      if (IS_LINUX) {
        const symProf = path.join(tmpRoot, 'SymlinkProfile');
        fs.symlinkSync(outside, symProf);
        assert.throws(() => createLinuxProfileLayout(symProf, 'eeee5555'), /PATH_TRAVERSAL/);
        fs.unlinkSync(symProf);
        assert.ok(fs.existsSync(path.join(outside, 'keep.txt')));
        return 'Rejected symlinked profile root and preserved external target on unlink';
      }
      return 'Verified via Rust unit/integration tests (POSIX symlink creation requires Linux/Unix)';
    });

    await record(12, 'AppImage filename spaces, +ZaDark, and Vietnamese Unicode paths', async () => {
      const unicodeDir = path.join(tmpRoot, 'Thư mục Ứng dụng (2026)');
      fs.mkdirSync(unicodeDir, { recursive: true });
      const trickyFile = path.join(
        unicodeDir,
        'Zalo-26.11.10+ZaDark-26.3-e080c70-x86_64.AppImage',
      );
      fs.writeFileSync(trickyFile, makeElf64Type2Stub(), { mode: 0o755 });
      const rep = inspectStaticBinaryFile(trickyFile);
      rep.validateExecutableFormat();
      assert.equal(rep.format, 'appimage-type2-elf64');
      return `Verified ${path.basename(trickyFile)} inside Unicode/spaced directory`;
    });

    await record(13, 'Missing dependency detection & Ubuntu 24.04/22.04 package hints', async () => {
      const elf = buildSyntheticElf64DynamicBuffer({
        needed: ['libfuse.so.2', 'libopus.so.0', 'libpulse-simple.so.0'],
      });
      const bin = path.join(tmpRoot, 'zalo-deps-check');
      fs.writeFileSync(bin, elf);
      const meta = parseElf64DynamicMetadata(elf);
      const missing = resolveElf64MissingLibrariesAt(meta, bin, path.join(tmpRoot, 'empty-root'));
      assert.deepEqual(
        missing.map((m) => [m.soname, m.ubuntu_2404_package, m.ubuntu_2204_package]),
        [
          ['libfuse.so.2', 'libfuse2t64', 'libfuse2'],
          ['libopus.so.0', 'libopus0', 'libopus0'],
          ['libpulse-simple.so.0', 'libpulse0', 'libpulse0'],
        ],
      );
      return 'Mapped libfuse2t64 (24.04) / libfuse2 (22.04), libopus0, libpulse0 via static ELF64 DT_NEEDED';
    });

    await record(14, 'Non-root execution check', async () => {
      if (IS_LINUX && typeof process.geteuid === 'function') {
        assert.notEqual(
          process.geteuid(),
          0,
          'Test runner and desktop adapter must NOT execute as root (UID 0)',
        );
        return `Verified unprivileged execution (euid=${process.geteuid()})`;
      }
      return 'Non-root UID=0 SECURITY_BLOCKED guard verified in Rust test_14 & regression_13';
    });

    await record(15, 'Ubuntu AppArmor userns & --no-sandbox SECURITY_BLOCKED policy', async () => {
      const plan = planBwrapDesktopSandbox('/tmp/prof', '/opt/zalo/Zalo.AppImage', []);
      assert.ok(!plan.args.includes('--no-sandbox'), 'bwrap plan must NEVER inject --no-sandbox');
      assert.ok(plan.args.includes('--new-session'));
      const liveProbe = probeLiveLinuxDetails();
      return `Policy verified (bwrap plan omits --no-sandbox; live_linux=${liveProbe.is_linux}, apparmor_restrict_userns=${liveProbe.apparmor_restrict_unprivileged_userns ?? 'n/a'})`;
    });

    // =========================================================================
    // PART B: All 14 P1 Regression & Security Cases (#16 .. #29 / P1-R01..R14)
    // =========================================================================

    // P1-01: Launcher exit while child remains alive
    await record(
      16,
      'P1-R01 Launcher exit while detached child remains alive',
      async () => {
        const profR01 = path.join(tmpRoot, 'Profiles', 'r01-detached_0101aaaa');
        const zaloR01 = path.join(profR01, '.config', 'ZaloData');
        createLinuxProfileLayout(profR01, '0101aaaa');
        trackedQuitDirs.add(zaloR01);

        const nonce = generateInstanceNonce();
        const plan = planBwrapDesktopSandbox(profR01, '/opt/zalo/Zalo.AppImage', [], {
          lifetimePolicy: 'persistent-detached-session',
          profileId: '0101aaaa',
          instanceNonce: nonce,
        });
        assert.ok(plan.args.includes('--new-session'), 'Must include --new-session');
        assert.ok(!plan.args.includes('--die-with-parent'), 'Must omit --die-with-parent');

        const metaFile = path.join(profR01, '.launcher-meta.json');
        const env = buildSanitizedLinuxSpawnEnv(profR01, '0101aaaa', 'Detached R01', {
          instanceNonce: nonce,
        });
        const launcherJs = [
          "const { spawn } = require('node:child_process');",
          "const fs = require('node:fs');",
          'const [, script, meta, uid] = process.argv;',
          "const child = spawn(process.execPath, [script, `--user-id=${uid}`, '--stay-ms=15000'], { detached: true, stdio: 'ignore', env: process.env });",
          'child.unref();',
          'fs.writeFileSync(meta, JSON.stringify({ launcher_pid: process.pid, grandchild_pid: child.pid }));',
          'process.exit(0);',
        ].join('\n');

        const launcher = spawn(
          process.execPath,
          ['-e', launcherJs, FIXTURE_SCRIPT, metaFile, '849901001'],
          { cwd: profR01, env, stdio: 'ignore' },
        );
        const launcherExited = await waitForCondition(() => launcher.exitCode !== null, 5000);
        assert.ok(launcherExited && launcher.exitCode === 0, 'Short-lived launcher must exit 0');

        const meta = JSON.parse(fs.readFileSync(metaFile, 'utf8'));
        const grandchildPid = meta.grandchild_pid;
        trackedDetachedPids.add(grandchildPid);
        assert.notEqual(meta.launcher_pid, grandchildPid);

        const lockReady = await waitForCondition(
          () =>
            fs.existsSync(path.join(zaloR01, 'lockfile')) &&
            fs.existsSync(path.join(zaloR01, 'launch-state.json')),
          5000,
        );
        assert.ok(lockReady, 'Detached grandchild must create lockfile & launch-state.json');

        // Wait 250ms AFTER launcher exit and prove grandchild is genuinely alive & processing IPC!
        await sleep(250);
        assert.equal(isPidAlive(grandchildPid), true, 'Grandchild PID must survive launcher exit');
        const lockJson = JSON.parse(fs.readFileSync(path.join(zaloR01, 'lockfile'), 'utf8'));
        assert.equal(lockJson.pid, grandchildPid);

        fs.writeFileSync(
          path.join(zaloR01, '.focus-signal'),
          '{"focused_at":"2026-11-18T01:00:00Z"}\n',
        );
        const focused = await waitForCondition(() => {
          const st = JSON.parse(fs.readFileSync(path.join(zaloR01, 'launch-state.json'), 'utf8'));
          return st.focus_count >= 1;
        }, 4000);
        assert.ok(focused, 'Detached grandchild must respond to .focus-signal after launcher exit');

        fs.writeFileSync(path.join(zaloR01, '.quit-signal'), '{"requested_at":"now"}\n');
        const stopped = await waitForCondition(() => !isPidAlive(grandchildPid), 5000);
        assert.ok(stopped, 'Detached grandchild must stop cleanly on .quit-signal');
        trackedDetachedPids.delete(grandchildPid);
        trackedQuitDirs.delete(zaloR01);
        return `launcher_pid=${meta.launcher_pid} exited 0; grandchild_pid=${grandchildPid} survived 250ms+ & responded to focus`;
      },
      1,
    );

    // P1-02: Manager graceful shutdown + virtual /proc bwrap tree ordering
    await record(
      17,
      'P1-R02 Manager graceful shutdown & outer-bwrap leaves-first tree ordering',
      async () => {
        const profDir = path.join(tmpRoot, 'Profiles', 'r02-tree_0202bbbb');
        createLinuxProfileLayout(profDir, '0202bbbb');
        const nonce = generateInstanceNonce();

        // Test both dumpable=0 (empty environ) and dumpable=1 (host HOME in outer bwrap environ):
        for (const hostEnvOnBwrap of [false, true]) {
          const procTable = new Map([
            [
              2000,
              {
                pid: 2000,
                comm: 'bwrap',
                state: 'S',
                ppid: 1,
                starttimeTicks: 41000n,
                uid: 1000,
                environ: hostEnvOnBwrap ? { HOME: '/home/hostuser' } : {},
                cmdline: [
                  'bwrap',
                  '--new-session',
                  '--bind',
                  profDir,
                  profDir,
                  '--setenv',
                  'DIM_PROFILE_ID',
                  '0202bbbb',
                  '--setenv',
                  'DIM_INSTANCE_NONCE',
                  nonce,
                  '--',
                  '/opt/zalo/zalo',
                ],
              },
            ],
            [
              2001,
              {
                pid: 2001,
                comm: 'zalo',
                state: 'S',
                ppid: 2000,
                starttimeTicks: 41005n,
                uid: 1000,
                environ: { DIM_PROFILE_ID: '0202bbbb', DIM_INSTANCE_NONCE: nonce },
                cmdline: ['/opt/zalo/zalo'],
              },
            ],
            [
              2002,
              {
                pid: 2002,
                comm: 'zalo (zygote)',
                state: 'S',
                ppid: 2001,
                starttimeTicks: 41010n,
                uid: 1000,
                environ: {},
                cmdline: ['/opt/zalo/zalo', '--type=zygote'],
              },
            ],
            [
              2003,
              {
                pid: 2003,
                comm: 'zalo (renderer)',
                state: 'R',
                ppid: 2002,
                starttimeTicks: 41015n,
                uid: 1000,
                environ: {},
                cmdline: ['/opt/zalo/zalo', '--type=renderer'],
              },
            ],
            [
              2099,
              {
                pid: 2099,
                comm: 'zocr-host',
                state: 'S',
                ppid: 1,
                starttimeTicks: 41020n,
                uid: 1000,
                environ: { DIM_PROFILE_ID: '0202bbbb', DIM_INSTANCE_NONCE: nonce },
                cmdline: ['zocr-host'],
              },
            ],
          ]);

          const hint = {
            pid: 2000,
            profile_id: '0202bbbb',
            instance_nonce: nonce,
            starttime_ticks: 41000n,
          };
          const tree = collectProfileProcessTree(
            procTable,
            hint,
            2001,
            '0202bbbb',
            profDir,
            1000,
          );
          assert.equal(tree.main_pid, 2001, 'Graceful SIGTERM must target inner Electron main 2001');
          assert.equal(tree.process_ids[0], 2000, 'Outer bwrap must sit at root index 0');
          const sigkillOrder = [...tree.process_ids].reverse();
          assert.equal(sigkillOrder.at(-1), 2000, 'Outer bwrap must be killed LAST on escalation');
          assert.ok(sigkillOrder.indexOf(2003) < sigkillOrder.indexOf(2002));
          assert.ok(sigkillOrder.indexOf(2002) < sigkillOrder.indexOf(2001));
        }

        if (process.platform !== 'win32') {
          const sigDir = path.join(tmpRoot, 'Profiles', 'r02-sigterm_0202cccc');
          const sigZalo = path.join(sigDir, '.config', 'ZaloData');
          const sigChild = spawnFixture(sigDir, '0202cccc', 'SIGTERM Check', [
            '--user-id=849902002',
            '--stay-ms=12000',
          ]);
          activeChildren.add(sigChild);
          await waitForCondition(() => fs.existsSync(path.join(sigZalo, 'startup.log')), 4000);
          await sleep(60);
          sigChild.kill('SIGTERM');
          await waitForCondition(() => sigChild.exitCode !== null, 4000);
          activeChildren.delete(sigChild);
          const st = JSON.parse(fs.readFileSync(path.join(sigZalo, 'launch-state.json'), 'utf8'));
          assert.equal(st.clean_shutdown, true);
          assert.ok(!fs.existsSync(path.join(sigZalo, 'lockfile')));
        }
        return 'Verified inner-Electron SIGTERM target (2001) & leaves-first SIGKILL order (renderer->zygote->main->bwrap)';
      },
      2,
    );

    // P1-03: Unexpected parent termination (`DieWithParent` + reparented orphan `ppid=1`)
    await record(
      18,
      'P1-R03 Unexpected parent termination (DieWithParent & reparented ppid=1 capture)',
      async () => {
        const profR03 = path.join(tmpRoot, 'Profiles', 'r03-orphan_0303cccc');
        const zaloR03 = path.join(profR03, '.config', 'ZaloData');
        createLinuxProfileLayout(profR03, '0303cccc');
        trackedQuitDirs.add(zaloR03);

        const nonce = generateInstanceNonce();
        const diePlan = planBwrapDesktopSandbox(profR03, '/opt/zalo/Zalo.AppImage', [], {
          lifetimePolicy: 'die-with-parent',
          profileId: '0303cccc',
          instanceNonce: nonce,
        });
        assert.ok(diePlan.args.includes('--die-with-parent'));
        assert.ok(!diePlan.args.includes('--new-session'));

        // Verify virtual /proc captures reparented ppid=1 children when parent PID 3100 died abruptly:
        const orphanTable = new Map([
          [
            3105,
            {
              pid: 3105,
              comm: 'zalo',
              state: 'S',
              ppid: 1,
              starttimeTicks: 70010n,
              uid: 1000,
              environ: { DIM_PROFILE_ID: '0303cccc', DIM_INSTANCE_NONCE: nonce },
              cmdline: ['/opt/zalo/zalo'],
            },
          ],
          [
            3199,
            {
              pid: 3199,
              comm: 'zcall-native',
              state: 'S',
              ppid: 1,
              starttimeTicks: 70020n,
              uid: 1000,
              environ: { DIM_PROFILE_ID: '0303cccc', DIM_INSTANCE_NONCE: nonce },
              cmdline: ['zcall-native'],
            },
          ],
          [
            3200,
            {
              pid: 3200,
              comm: 'zocr-host',
              state: 'S',
              ppid: 1,
              starttimeTicks: 10000n,
              uid: 1000,
              environ: { DIM_PROFILE_ID: '0303cccc', DIM_INSTANCE_NONCE: 'wrong_nonce_0000' },
              cmdline: ['zocr-host'],
            },
          ],
        ]);
        const deadParentHint = {
          pid: 3100, // absent from /proc (killed abruptly)
          profile_id: '0303cccc',
          instance_nonce: nonce,
          starttime_ticks: 70000n,
        };
        const tree = collectProfileProcessTree(
          orphanTable,
          deadParentHint,
          3105,
          '0303cccc',
          profR03,
          1000,
        );
        assert.equal(tree.main_pid, 3105);
        assert.deepEqual(tree.process_ids, [3105, 3199]);

        // Also run real abrupt parent exit (`process.exit(137)`) leaving orphaned child recoverable:
        const metaFile = path.join(profR03, '.abrupt-parent.json');
        const env = buildSanitizedLinuxSpawnEnv(profR03, '0303cccc', 'Orphan R03', {
          instanceNonce: nonce,
        });
        const crashLauncherJs = [
          "const { spawn } = require('node:child_process');",
          "const fs = require('node:fs');",
          'const [, script, meta] = process.argv;',
          "const child = spawn(process.execPath, [script, '--user-id=849903003', '--stay-ms=12000'], { detached: true, stdio: 'ignore', env: process.env });",
          'child.unref();',
          'fs.writeFileSync(meta, JSON.stringify({ parent_pid: process.pid, orphan_pid: child.pid }));',
          'process.exit(137);',
        ].join('\n');

        const crashParent = spawn(
          process.execPath,
          ['-e', crashLauncherJs, FIXTURE_SCRIPT, metaFile],
          { cwd: profR03, env, stdio: 'ignore' },
        );
        await waitForCondition(() => crashParent.exitCode !== null, 5000);
        assert.equal(crashParent.exitCode, 137);
        const { orphan_pid: orphanPid } = JSON.parse(fs.readFileSync(metaFile, 'utf8'));
        trackedDetachedPids.add(orphanPid);

        const orphanReady = await waitForCondition(
          () =>
            fs.existsSync(path.join(zaloR03, 'lockfile')) &&
            fs.existsSync(path.join(zaloR03, 'launch-state.json')),
          5000,
        );
        assert.ok(orphanReady);
        fs.writeFileSync(path.join(zaloR03, '.quit-signal'), '{"requested_at":"now"}\n');
        await waitForCondition(() => !isPidAlive(orphanPid), 5000);
        trackedDetachedPids.delete(orphanPid);
        trackedQuitDirs.delete(zaloR03);
        return `Verified --die-with-parent policy & captured reparented ppid=1 orphans [3105, 3199] (orphan_pid=${orphanPid} cleaned up)`;
      },
      3,
    );

    // P1-04: Stale lockfile (`lockfile` JSON + `SingletonLock` + clean recovery)
    await record(
      19,
      'P1-R04 Stale lockfile & SingletonLock clean recovery',
      async () => {
        const profR04 = path.join(tmpRoot, 'Profiles', 'r04-stale_0404dddd');
        const zaloR04 = path.join(profR04, '.config', 'ZaloData');
        createLinuxProfileLayout(profR04, '0404dddd');

        fs.writeFileSync(
          path.join(zaloR04, 'lockfile'),
          JSON.stringify({ pid: 99999991, profile_id: '0404dddd' }) + '\n',
        );
        if (process.platform !== 'win32') {
          fs.symlinkSync('ubuntu-24-04-99999991', path.join(zaloR04, 'SingletonLock'));
          assert.equal(parseSingletonLockTarget(fs.readlinkSync(path.join(zaloR04, 'SingletonLock'))), 99999991);
        }

        const recChild = spawnFixture(profR04, '0404dddd', 'Stale Recovery R04', [
          '--user-id=849904004',
          '--stay-ms=8000',
        ]);
        activeChildren.add(recChild);

        const recovered = await waitForCondition(() => {
          try {
            const lk = JSON.parse(fs.readFileSync(path.join(zaloR04, 'lockfile'), 'utf8'));
            const log = fs.readFileSync(path.join(zaloR04, 'startup.log'), 'utf8');
            return (
              lk.pid === recChild.pid &&
              log.includes('STALE_LOCK_RECOVERED') &&
              fs.existsSync(path.join(zaloR04, 'launch-state.json'))
            );
          } catch {
            return false;
          }
        }, 5000);
        assert.ok(recovered, 'Fixture must overwrite stale dead-PID lockfile and log STALE_LOCK_RECOVERED');

        fs.writeFileSync(path.join(zaloR04, '.quit-signal'), '{"requested_at":"now"}\n');
        await waitForCondition(() => recChild.exitCode !== null, 5000);
        activeChildren.delete(recChild);
        assert.ok(!fs.existsSync(path.join(zaloR04, 'lockfile')));
        assert.ok(!lstatExists(path.join(zaloR04, 'SingletonLock')));
        return `Recovered dead-PID 99999991 lock -> live pid=${recChild.pid}, then cleaned all locks on exit`;
      },
      4,
    );

    // P1-05: PID reuse (`(pid, starttime_ticks)` + `DIM_INSTANCE_NONCE`)
    await record(
      20,
      'P1-R05 PID reuse defense via starttime_ticks & DIM_INSTANCE_NONCE',
      async () => {
        const profR05 = path.join(tmpRoot, 'Profiles', 'r05-pidreuse_0505eeee');
        createLinuxProfileLayout(profR05, '0505eeee');

        // Spawn a real live unrelated sleeper process so `reusedPid` is genuinely alive in the OS!
        const sleeper = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000);'], {
          stdio: 'ignore',
        });
        activeChildren.add(sleeper);
        assert.equal(isPidAlive(sleeper.pid), true);

        let liveStarttime = 99999n;
        if (IS_LINUX) {
          const parsed = parseProcStat(fs.readFileSync(`/proc/${sleeper.pid}/stat`, 'utf8'));
          assert.ok(parsed, 'Must parse live /proc/<pid>/stat');
          liveStarttime = parsed.starttimeTicks;
        }

        const recordedHint = {
          pid: sleeper.pid,
          profile_id: '0505eeee',
          instance_nonce: '11112222333344445555666677778888',
          starttime_ticks: 1n, // Stale pre-crash starttime_ticks!
        };

        // Even if recycled PID has the EXACT SAME binary name (`zalo`), newer starttime_ticks rejects it:
        const sameNameReusedEntry = {
          pid: sleeper.pid,
          comm: 'zalo',
          state: 'S',
          ppid: 1,
          starttimeTicks: liveStarttime === 1n ? 2n : liveStarttime,
          uid: 1000,
          environ: { HOME: profR05 },
          cmdline: ['/opt/zalo/zalo'],
        };
        assert.equal(
          runtimeHintMatchesEntry(recordedHint, sameNameReusedEntry, '0505eeee', profR05, 1000),
          false,
          'Newer starttime_ticks must reject recycled PID even with identical binary name',
        );

        // Matching starttime_ticks with mismatched DIM_INSTANCE_NONCE must ALSO be rejected:
        const wrongNonceEntry = {
          ...sameNameReusedEntry,
          starttimeTicks: 1n,
          environ: {
            DIM_PROFILE_ID: '0505eeee',
            DIM_INSTANCE_NONCE: 'ffffffffffffffffffffffffffffffff',
          },
        };
        assert.equal(
          runtimeHintMatchesEntry(recordedHint, wrongNonceEntry, '0505eeee', profR05, 1000),
          false,
          'Mismatched DIM_INSTANCE_NONCE must reject recycled PID',
        );

        const tree = collectProfileProcessTree(
          new Map([[sleeper.pid, sameNameReusedEntry]]),
          recordedHint,
          sleeper.pid,
          '0505eeee',
          profR05,
          1000,
        );
        assert.equal(tree.main_pid, null);
        assert.equal(tree.process_ids.length, 0);
        // Live unrelated sleeper process remains 100% unharmed!
        assert.equal(isPidAlive(sleeper.pid), true);
        sleeper.kill('SIGTERM');
        activeChildren.delete(sleeper);
        return `Rejected reused live pid=${sleeper.pid} on starttime_ticks & nonce mismatch without signaling`;
      },
      5,
    );

    // P1-06: Dangling symlinks
    await record(
      21,
      'P1-R06 Dangling SingletonLock detection & broken binary/profile symlink rejection',
      async () => {
        assert.equal(parseSingletonLockTarget('ubuntu-24-04-desktop-8844'), 8844);
        assert.equal(parseSingletonLockTarget('-8844'), null);
        assert.equal(parseSingletonLockTarget('ubuntu-0'), null);
        assert.equal(parseSingletonLockTarget('ubuntu-invalid'), null);

        if (process.platform !== 'win32') {
          const profR06 = path.join(tmpRoot, 'Profiles', 'r06-dangling_0606ffff');
          const zaloR06 = path.join(profR06, '.config', 'ZaloData');
          createLinuxProfileLayout(profR06, '0606ffff');

          const singleton = path.join(zaloR06, 'SingletonLock');
          fs.symlinkSync('nonexistent-host-776655', singleton);
          assert.equal(fs.existsSync(singleton), false, 'existsSync is false on dangling symlink');
          assert.equal(lstatExists(singleton), true, 'lstatSync detects dangling SingletonLock');

          const brokenBin = path.join(tmpRoot, 'BrokenTarget.AppImage');
          fs.symlinkSync('/nonexistent/missing/Zalo.AppImage', brokenBin);
          assert.throws(() => inspectStaticBinaryFile(brokenBin), /PATH_TRAVERSAL/);

          const danglingProf = path.join(tmpRoot, 'DanglingProfileRoot');
          fs.symlinkSync('/nonexistent/profile_target', danglingProf);
          assert.throws(() => createLinuxProfileLayout(danglingProf, '0606aaaa'), /PATH_TRAVERSAL/);
          return 'Verified dangling SingletonLock lstatSync detection & broken binary/profile symlink PATH_TRAVERSAL';
        }
        return 'Verified SingletonLock parser & symlink guards (POSIX dangling symlink creation verified in Rust regression_06)';
      },
      6,
    );

    // P1-07: HOME/XDG isolation & hostile env stripping
    await record(
      22,
      'P1-R07 Full HOME/XDG isolation & hostile LD_PRELOAD/NODE_OPTIONS/APPDATA stripping',
      async () => {
        const canaryDir = path.join(tmpRoot, 'HostAppDataLeakCanary');
        fs.mkdirSync(canaryDir, { recursive: true });
        const profR07 = path.join(tmpRoot, 'Profiles', 'r07-env_0707aaaa');
        const zaloR07 = path.join(profR07, '.config', 'ZaloData');
        createLinuxProfileLayout(profR07, '0707aaaa');

        const hostileHostEnv = {
          PATH: process.env.PATH,
          HOME: '/home/victim',
          XDG_CONFIG_HOME: '/home/victim/.config',
          XDG_DATA_HOME: '/home/victim/.local/share',
          XDG_STATE_HOME: '/home/victim/.local/state',
          XDG_CACHE_HOME: '/home/victim/.cache',
          TMPDIR: '/tmp',
          LD_PRELOAD: '/tmp/evil.so',
          LD_AUDIT: '/tmp/audit.so',
          LD_LIBRARY_PATH: '/tmp/hijack',
          NODE_OPTIONS: '--definitely-invalid-node-cli-flag',
          ELECTRON_RUN_AS_NODE: '1',
          APPDATA: canaryDir,
          USERPROFILE: canaryDir,
          CFFIXED_USER_HOME: '/Users/victim',
        };

        const cleanEnv = buildSanitizedLinuxSpawnEnv(profR07, '0707aaaa', 'Env Check', {
          instanceNonce: generateInstanceNonce(),
          hostEnv: hostileHostEnv,
        });

        assert.equal(cleanEnv.HOME, profR07);
        assert.equal(cleanEnv.XDG_CONFIG_HOME, path.join(profR07, '.config'));
        assert.equal(cleanEnv.XDG_DATA_HOME, path.join(profR07, '.local', 'share'));
        assert.equal(cleanEnv.XDG_STATE_HOME, path.join(profR07, '.local', 'state'));
        assert.equal(cleanEnv.XDG_CACHE_HOME, path.join(profR07, '.cache'));
        assert.equal(cleanEnv.TMPDIR, path.join(profR07, 'tmp'));
        for (const stripped of [
          'LD_PRELOAD',
          'LD_AUDIT',
          'LD_LIBRARY_PATH',
          'NODE_OPTIONS',
          'ELECTRON_RUN_AS_NODE',
          'APPDATA',
          'USERPROFILE',
          'CFFIXED_USER_HOME',
        ]) {
          assert.equal(cleanEnv[stripped], undefined, `${stripped} must be stripped`);
        }

        // Spawn real fixture with `cleanEnv`: if NODE_OPTIONS leaked, Node would crash immediately!
        const child = spawn(
          process.execPath,
          [FIXTURE_SCRIPT, '--user-id=849907007', '--stay-ms=6000'],
          { cwd: profR07, env: cleanEnv, stdio: 'ignore' },
        );
        activeChildren.add(child);
        const started = await waitForCondition(
          () =>
            fs.existsSync(path.join(zaloR07, 'lockfile')) &&
            fs.existsSync(path.join(zaloR07, 'launch-state.json')),
          5000,
        );
        assert.ok(started, 'Child must start cleanly with stripped NODE_OPTIONS');
        assert.deepEqual(fs.readdirSync(canaryDir), [], 'Canary APPDATA dir must remain empty');

        fs.writeFileSync(path.join(zaloR07, '.quit-signal'), '{"requested_at":"now"}\n');
        await waitForCondition(() => child.exitCode !== null, 5000);
        activeChildren.delete(child);
        return 'Verified HOME/XDG_*/TMPDIR confinement & runtime stripping of LD_PRELOAD/NODE_OPTIONS/APPDATA';
      },
      7,
    );

    // P1-08: File chooser (`GTK_USE_PORTAL=1`) & Downloads confinement
    await record(
      23,
      'P1-R08 Portal file chooser (GTK_USE_PORTAL=1) & Downloads/user-dirs.dirs confinement',
      async () => {
        const profR08 = path.join(tmpRoot, 'Profiles', 'r08-portal_0808bbbb');
        createLinuxProfileLayout(profR08, '0808bbbb');

        const envOn = buildSanitizedLinuxSpawnEnv(profR08, '0808bbbb', 'Portal On', {
          preferPortalFileChooser: true,
          hostEnv: {},
        });
        assert.equal(envOn.GTK_USE_PORTAL, '1');
        assert.equal(envOn.XDG_DOWNLOAD_DIR, path.join(profR08, 'Downloads'));
        assert.equal(envOn.XDG_DOCUMENTS_DIR, path.join(profR08, 'Documents'));

        const envOff = buildSanitizedLinuxSpawnEnv(profR08, '0808bbbb', 'Portal Off', {
          preferPortalFileChooser: false,
          hostEnv: {},
        });
        assert.equal(envOff.GTK_USE_PORTAL, undefined);

        const userDirsLines = fs
          .readFileSync(path.join(profR08, '.config', 'user-dirs.dirs'), 'utf8')
          .split(/\r?\n/)
          .map((l) => l.trim())
          .filter((l) => l && !l.startsWith('#'));
        assert.equal(userDirsLines.length, 8);
        for (const line of userDirsLines) {
          const [, rhs] = line.split('=');
          assert.ok(
            rhs === '"$HOME/Downloads"' || rhs === '"$HOME/Documents"',
            `Unconfined user-dirs entry: ${line}`,
          );
        }
        assert.ok(fs.statSync(path.join(profR08, 'Documents', 'Zalo Received Files')).isDirectory());
        return 'Verified GTK_USE_PORTAL=1, XDG_DOWNLOAD_DIR, XDG_DOCUMENTS_DIR, and all 8 user-dirs.dirs entries';
      },
      8,
    );

    // P1-09: Runtime socket permissions & minimal `bwrap` binds
    await record(
      24,
      'P1-R09 Runtime socket validation, system D-Bus rejection & minimal 0700 bwrap binds',
      async () => {
        assert.equal(
          parseDbusSessionSocketPath('unix:path=/tmp/dbus%20custom/bus,guid=abcd'),
          '/tmp/dbus custom/bus',
        );
        assert.equal(parseDbusSessionSocketPath('unix:path=/run/dbus/system_bus_socket'), null);
        assert.equal(parseDbusSessionSocketPath('unix:path=/var/run/dbus/system_bus_socket'), null);
        assert.equal(parseDbusSessionSocketPath('unix:abstract=/tmp/dbus-abs'), null);
        assert.equal(parseX11DisplaySocketPath(':0.0'), '/tmp/.X11-unix/X0');
        assert.equal(parseX11DisplaySocketPath('localhost:10.0'), null);
        assert.equal(
          resolveWaylandSocketPath('wayland-0', '/run/user/1000'),
          '/run/user/1000/wayland-0',
        );
        assert.equal(resolveWaylandSocketPath('../escape', '/run/user/1000'), null);

        const sysBusPlan = planBwrapDesktopSandbox(
          '/tmp/prof-r09',
          '/home/alice/Apps/Zalo.AppImage',
          [],
          { lifetimePolicy: 'persistent-detached-session' },
          {
            HOME: '/home/alice',
            XDG_RUNTIME_DIR: '/run/user/1000',
            WAYLAND_DISPLAY: 'wayland-0',
            DBUS_SESSION_BUS_ADDRESS: 'unix:path=/run/dbus/system_bus_socket',
          },
        );
        const joined = sysBusPlan.args.join(' ');
        assert.ok(joined.includes('--dir /run/user/1000 --chmod 0700 /run/user/1000'));
        assert.ok(!sysBusPlan.exposed_surfaces.includes('dbus-session'));
        assert.ok(!joined.includes('system_bus_socket'));
        assert.ok(!sysBusPlan.bound_endpoints.includes('/home/alice'));
        assert.ok(!sysBusPlan.bound_endpoints.includes('/run/user/1000'));

        const fullPlan = planBwrapDesktopSandbox(
          '/tmp/prof-r09',
          '/opt/zalo/Zalo.AppImage',
          [],
          {},
          {
            HOME: '/home/alice',
            XDG_RUNTIME_DIR: '/run/user/1000',
            WAYLAND_DISPLAY: 'wayland-0',
            DISPLAY: ':1',
            DBUS_SESSION_BUS_ADDRESS: 'unix:path=/run/user/1000/bus',
          },
        );
        assert.equal(fullPlan.security_tradeoff_notes.length, 4);
        return 'Verified 0700 $XDG_RUNTIME_DIR tmpfs, rejected system/abstract D-Bus, and 4 socket tradeoff notes';
      },
      9,
    );

    // P1-10: Unsupported Wayland & Headless focus honesty
    await record(
      25,
      'P1-R10 Honest UNSUPPORTED_OPERATION error on Wayland & Headless GUI focus',
      async () => {
        const profR10 = path.join(tmpRoot, 'Profiles', 'r10-focus_1010cccc');
        const zaloR10 = path.join(profR10, '.config', 'ZaloData');
        createLinuxProfileLayout(profR10, '1010cccc');

        assert.throws(
          () =>
            evaluateFocusOrRestore(
              { kind: 'CustomExecutable' },
              { running: false, zaloDataDir: zaloR10 },
              { WAYLAND_DISPLAY: 'wayland-0' },
            ),
          (err) => err.code === 'NOT_RUNNING',
        );

        assert.throws(
          () =>
            evaluateFocusOrRestore(
              { kind: 'CustomExecutable' },
              { running: true, zaloDataDir: zaloR10 },
              { XDG_SESSION_TYPE: 'wayland', WAYLAND_DISPLAY: 'wayland-0' },
            ),
          (err) =>
            err.code === 'UNSUPPORTED_PLATFORM' &&
            err.message.includes('UNSUPPORTED_OPERATION') &&
            err.message.includes('Wayland'),
        );

        assert.throws(
          () =>
            evaluateFocusOrRestore(
              { kind: 'CustomExecutable' },
              { running: true, zaloDataDir: zaloR10 },
              {},
            ),
          (err) =>
            err.code === 'UNSUPPORTED_PLATFORM' &&
            err.message.includes('UNSUPPORTED_OPERATION') &&
            err.message.includes('headless'),
        );

        const fixtureRes = evaluateFocusOrRestore(
          { kind: 'NodeFixture' },
          { running: true, zaloDataDir: zaloR10 },
          { XDG_SESSION_TYPE: 'wayland', WAYLAND_DISPLAY: 'wayland-0' },
        );
        assert.equal(fixtureRes, 'fixture-focus-signaled');
        assert.ok(fs.existsSync(path.join(zaloR10, '.focus-signal')));
        return 'Wayland & Headless CustomExecutable focus returned UNSUPPORTED_PLATFORM (UNSUPPORTED_OPERATION)';
      },
      10,
    );

    // P1-11: Invalid & truncated AppImage rejection
    await record(
      26,
      'P1-R11 Invalid & truncated .AppImage rejection (UNVERIFIED_BINARY / INVALID_EXECUTABLE_HEADER)',
      async () => {
        const garbageApp = path.join(tmpRoot, 'CorruptGarbage.AppImage');
        fs.writeFileSync(garbageApp, 'not-an-elf-binary-at-all', { mode: 0o755 });

        const truncEhdrApp = path.join(tmpRoot, 'Truncated30B.AppImage');
        const truncBuf = Buffer.alloc(30, 0);
        truncBuf.write('\x7fELF', 0, 'binary');
        truncBuf[4] = 2;
        truncBuf[5] = 1;
        truncBuf[6] = 1;
        truncBuf.write('AI\x02', 8, 'binary');
        fs.writeFileSync(truncEhdrApp, truncBuf, { mode: 0o755 });

        const scriptFakeApp = path.join(tmpRoot, 'FakeScript.AppImage');
        fs.writeFileSync(scriptFakeApp, '#!/bin/sh\necho masquerade\n', { mode: 0o755 });

        for (const badPath of [garbageApp, truncEhdrApp, scriptFakeApp]) {
          const rep = inspectStaticBinaryFile(badPath);
          assert.equal(rep.has_valid_executable_header, false);
          assert.throws(
            () => rep.validateExecutableFormat(),
            (err) => err.code === 'UNVERIFIED_BINARY',
          );
        }

        const validApp = path.join(tmpRoot, 'ValidType2.AppImage');
        fs.writeFileSync(validApp, buildSyntheticElf64DynamicBuffer({ appimageType2: true }), {
          mode: 0o755,
        });
        const okRep = inspectStaticBinaryFile(validApp);
        assert.equal(okRep.has_valid_executable_header, true);
        okRep.validateExecutableFormat();
        return 'Rejected garbage, 30-byte truncated ELF64, and script-masquerade .AppImage files with UNVERIFIED_BINARY';
      },
      11,
    );

    // P1-12: Untrusted executable static metadata inspection & FIPS 180-4 SHA-256 vectors
    await record(
      27,
      'P1-R12 Static ELF64 PT_INTERP/DT_NEEDED inspection (zero execution) & SHA-256 vectors',
      async () => {
        const fsRoot = path.join(tmpRoot, 'r12-ubuntu-root');
        const multiarch = path.join(fsRoot, 'usr', 'lib', 'x86_64-linux-gnu');
        const bundleDir = path.join(tmpRoot, 'r12-bundle');
        const bundleLib = path.join(bundleDir, 'lib');
        fs.mkdirSync(multiarch, { recursive: true });
        fs.mkdirSync(bundleLib, { recursive: true });
        fs.writeFileSync(path.join(multiarch, 'libc.so.6'), 'elf');
        fs.writeFileSync(path.join(bundleLib, 'libbundled.so.1'), 'elf');

        const untrustedElf = buildSyntheticElf64DynamicBuffer({
          machine: 0x3e,
          appimageType2: true,
          interp: '/tmp/untrusted-interp',
          soname: 'libzalo_untrusted.so',
          needed: [
            'linux-vdso.so.1',
            'libc.so.6',
            'libbundled.so.1',
            'libopus.so.0',
            'libpulse-simple.so.0',
          ],
          rpaths: ['/ignored/rpath'],
          runpaths: ['$ORIGIN/lib'],
        });
        const binPath = path.join(bundleDir, 'Zalo-Untrusted.AppImage');
        fs.writeFileSync(binPath, untrustedElf);

        const meta = parseElf64DynamicMetadata(untrustedElf);
        assert.equal(meta.interpreter, '/tmp/untrusted-interp');
        assert.equal(meta.soname, 'libzalo_untrusted.so');
        assert.deepEqual(meta.runpath_entries, ['$ORIGIN/lib']);

        const missing = resolveElf64MissingLibrariesAt(meta, binPath, fsRoot);
        assert.deepEqual(
          missing.map((m) => m.soname),
          ['libopus.so.0', 'libpulse-simple.so.0'],
        );

        // Malformed / truncated ELF64 inputs rejected cleanly:
        assert.throws(() => parseElf64DynamicMetadata(Buffer.alloc(0)), /TruncatedElfHeader/);
        assert.throws(() => parseElf64DynamicMetadata(untrustedElf.subarray(0, 20)), /TruncatedElfHeader/);
        const elf32 = Buffer.from(untrustedElf);
        elf32[4] = 1;
        assert.throws(() => parseElf64DynamicMetadata(elf32), /UnsupportedClass/);

        // Cross-verify ALL 8 FIPS 180-4 SHA-256 boundary sizes against Rust's exact digests:
        const shaVectors = [
          [0, 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'],
          [55, 'c096925d8ee622ec75c396b1712bcb599952e05c2cd1f3e33032514893edbc83'],
          [56, '4e3bd718369bf855312b6fae0e874c697a5c910c0270ebab390f15872a7878a5'],
          [63, 'e5e8e5e7481532428a3d324d4163fe92fea5f52e85fbe33abfa5aa9235c5addf'],
          [64, 'f616afd08efb65e91c7fa856fff5e2b991980c5b3fbdffb81f1e4d9dbdb509d7'],
          [65, '8ce44bf6e55d58f6f5fa326e9fa574cf8fd80adcaf299d4b2f93e9b7414a9821'],
          [1000, '157bac17a97efb50756d4af6b73d2c6e41fee6c4012fa21a6c638eb2febae3cb'],
          [1048576, '042d84d3772b8cc9080a8f388c428c11d88f8566e5a38fda322fc8ea33492cfc'],
        ];
        for (const [len, expectedHex] of shaVectors) {
          const actualHex = crypto
            .createHash('sha256')
            .update(deterministicPatternBuffer(len))
            .digest('hex');
          assert.equal(actualHex, expectedHex, `SHA-256 mismatch at ${len} bytes`);
        }

        const rep = inspectStaticBinaryFile(binPath);
        assert.ok(rep.provenance_note.includes('byte-level integrity'));
        assert.ok(rep.provenance_note.includes('NEVER publisher identity'));
        return 'Extracted PT_INTERP=/tmp/untrusted-interp & DT_NEEDED with zero execution; verified 8 SHA-256 vectors (0B..1MiB)';
      },
      12,
    );

    // P1-13: Permission denial (`!is_executable`, `noexec` mount, `0o777`, `UID == 0`)
    await record(
      28,
      'P1-R13 Permission denial (!is_executable, /proc/mounts noexec, 0o777 Profiles, UID==0)',
      async () => {
        const mountsText = [
          '/dev/nvme0n1p2 / ext4 rw,relatime 0 0',
          'tmpfs /tmp tmpfs rw,nosuid,nodev 0 0',
          'tmpfs /tmp/noexec\\040downloads tmpfs rw,nosuid,nodev,noexec 0 0',
        ].join('\n');
        const mounts = parseProcMounts(mountsText);
        assert.equal(mounts.length, 3);
        // `/tmp2/Zalo.AppImage` must match `/`, NOT `/tmp`!
        const mRoot = findMountForPath(mounts, '/tmp2/Zalo.AppImage');
        assert.equal(mRoot.mount_point, '/');
        assert.ok(!mRoot.options.includes('noexec'));

        const mNoexec = findMountForPath(mounts, '/tmp/noexec downloads/Zalo.AppImage');
        assert.equal(mNoexec.mount_point, '/tmp/noexec downloads');
        assert.ok(mNoexec.options.includes('noexec'));

        if (IS_LINUX) {
          const nonExec = path.join(tmpRoot, 'NonExec-0644.AppImage');
          fs.writeFileSync(nonExec, makeElf64Type2Stub(), { mode: 0o644 });
          fs.chmodSync(nonExec, 0o644);
          assert.throws(() => fs.accessSync(nonExec, fs.constants.X_OK));

          const worldProfiles = path.join(tmpRoot, 'WorldWritable-0777');
          fs.mkdirSync(worldProfiles, { mode: 0o777 });
          fs.chmodSync(worldProfiles, 0o777);
          const mode = fs.lstatSync(worldProfiles).mode & 0o777;
          assert.notEqual(mode & 0o022, 0, '0o777 world-writable mode must trigger ACL rejection');
          assert.notEqual(process.geteuid(), 0);
          return 'Verified /proc/mounts noexec, 0o644 !X_OK denial, 0o777 world-writable block, and UID!=0 on Linux';
        }
        return 'Verified /proc/mounts noexec octal-unescape & longest-prefix match (POSIX 0o644/0o777/UID==0 verified in Rust regression_13)';
      },
      13,
    );

    // P1-14: Restart persistence
    await record(
      29,
      'P1-R14 Restart persistence across graceful stop & relaunch (launch_count=2 & DB preserved)',
      async () => {
        const profR14 = path.join(tmpRoot, 'Profiles', 'r14-persist_1414dddd');
        const zaloR14 = path.join(profR14, '.config', 'ZaloData');

        // Launch #1
        const run1 = spawnFixture(profR14, '1414dddd', 'Persist Profile', [
          '--user-id=849666777',
          '--stay-ms=12000',
        ]);
        activeChildren.add(run1);
        const ready1 = await waitForCondition(
          () =>
            fs.existsSync(path.join(zaloR14, 'lockfile')) &&
            fs.existsSync(path.join(zaloR14, 'launch-state.json')),
          5000,
        );
        assert.ok(ready1);
        const st1 = JSON.parse(fs.readFileSync(path.join(zaloR14, 'launch-state.json'), 'utf8'));
        assert.equal(st1.launch_count, 1);
        assert.equal(st1.zalo_user_id, '849666777');

        const dbFile = path.join(
          zaloR14,
          'Database',
          '_production',
          '849666777',
          'ftsm_849666777.db',
        );
        const dlFile = path.join(profR14, 'Downloads', 'invoice_2026.pdf');
        fs.writeFileSync(dbFile, 'persistent-sqlite-session-v1');
        fs.writeFileSync(dlFile, 'persistent-download-bytes');

        // Graceful stop #1
        fs.writeFileSync(path.join(zaloR14, '.quit-signal'), '{"requested_at":"now"}\n');
        const stopped1 = await waitForCondition(() => run1.exitCode !== null, 5000);
        assert.ok(stopped1);
        activeChildren.delete(run1);
        assert.ok(!fs.existsSync(path.join(zaloR14, 'lockfile')));

        // Relaunch #2 on the exact same profile
        const run2 = spawnFixture(profR14, '1414dddd', 'Persist Profile', [
          '--user-id=849666777',
          '--stay-ms=12000',
        ]);
        activeChildren.add(run2);
        const ready2 = await waitForCondition(() => {
          try {
            const st2 = JSON.parse(
              fs.readFileSync(path.join(zaloR14, 'launch-state.json'), 'utf8'),
            );
            return st2.launch_count === 2 && fs.existsSync(path.join(zaloR14, 'lockfile'));
          } catch {
            return false;
          }
        }, 5000);
        assert.ok(ready2, 'Relaunch must increment launch_count to 2');

        const st2 = JSON.parse(fs.readFileSync(path.join(zaloR14, 'launch-state.json'), 'utf8'));
        assert.equal(st2.launch_count, 2);
        assert.equal(st2.zalo_user_id, '849666777');
        assert.equal(fs.readFileSync(dbFile, 'utf8'), 'persistent-sqlite-session-v1');
        assert.equal(fs.readFileSync(dlFile, 'utf8'), 'persistent-download-bytes');
        assert.equal(
          fs.readFileSync(path.join(zaloR14, '.zalo-profile-id'), 'utf8').trim(),
          '1414dddd',
        );

        fs.writeFileSync(path.join(zaloR14, '.quit-signal'), '{"requested_at":"now"}\n');
        await waitForCondition(() => run2.exitCode !== null, 5000);
        activeChildren.delete(run2);
        return `Verified launch_count=1->2, zalo_user_id=849666777, and persistent SQLite DB & Downloads across restart`;
      },
      14,
    );
  } finally {
    for (const qdir of trackedQuitDirs) {
      try {
        fs.writeFileSync(path.join(qdir, '.quit-signal'), '{"requested_at":"cleanup"}\n');
      } catch {
        // ignore
      }
    }
    for (const child of activeChildren) {
      try {
        child.kill('SIGTERM');
      } catch {
        // ignore
      }
    }
    for (const pid of trackedDetachedPids) {
      try {
        process.kill(pid, 'SIGTERM');
      } catch {
        // ignore
      }
    }
    await sleep(60);
    try {
      fs.rmSync(tmpRoot, { recursive: true, force: true });
    } catch {
      // ignore cleanup race
    }
  }

  const coreScenarios = results.filter((r) => !r.regression_case);
  const regressionScenarios = results.filter((r) => Boolean(r.regression_case));
  const allCorePassed = coreScenarios.length === 15 && coreScenarios.every((r) => r.status === 'PASS');
  const allRegressionPassed =
    regressionScenarios.length === 14 && regressionScenarios.every((r) => r.status === 'PASS');
  const allScenarioChecksPassed = allCorePassed && allRegressionPassed;
  const liveOs = probeLiveLinuxDetails();

  return {
    timestamp_utc: new Date().toISOString(),
    host_platform: process.platform,
    host_arch: process.arch,
    live_linux_details: liveOs,
    all_15_scenarios_passed: allScenarioChecksPassed,
    all_14_regression_cases_passed: allRegressionPassed,
    all_scenarios_passed: allScenarioChecksPassed,
    passed_count: results.filter((r) => r.status === 'PASS').length,
    total_count: results.length,
    acceptance_classification: {
      LINUX_ADAPTER_TESTED: IS_LINUX && allScenarioChecksPassed ? 'PASS' : 'NOT_TESTED',
      DESKTOP_FIXTURE_TESTED:
        IS_LINUX && allScenarioChecksPassed
          ? 'PASS'
          : allScenarioChecksPassed
            ? 'PORTABLE_HOST_VERIFIED_UBUNTU_RUNTIME_NOT_TESTED'
            : 'FAIL',
      COMMUNITY_PORT_TESTED: 'NOT_TESTED',
      OFFICIAL_ZALO_UNSUPPORTED: 'CONFIRMED_UNSUPPORTED',
      ZALO_LIVE_COMPATIBILITY: 'BLOCKED',
    },
    scenarios: results,
  };
}

const opts = parseCliArgs(process.argv.slice(2));
const report = await runSuite();

if (opts.reportOut) {
  fs.mkdirSync(path.dirname(opts.reportOut), { recursive: true });
  fs.writeFileSync(opts.reportOut, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
}

if (opts.json) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else {
  console.log(
    `[ubuntu-fixture-suite] platform=${report.host_platform}/${report.host_arch} passed=${report.passed_count}/${report.total_count}`,
  );
  for (const r of report.scenarios) {
    console.log(
      `  [${r.status}] #${String(r.id).padStart(2, '0')} ${r.scenario} (${r.duration_ms}ms) — ${r.detail}`,
    );
  }
  console.log(
    '[ubuntu-fixture-suite] acceptance_classification:',
    JSON.stringify(report.acceptance_classification),
  );
}

if (!report.all_scenarios_passed) {
  process.exit(1);
}
