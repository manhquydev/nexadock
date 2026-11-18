/**
 * Shared Windows Native Acceptance Lab utilities (`lib/common.mjs`).
 *
 * Provides deterministic, non-destructive helpers for:
 * 1. PII & host-path redaction (`<USER_HOME>`, `<WORKTREE_ROOT>`, `<TEMP_DIR>`, `<USER>`, `<HOSTNAME>`)
 * 2. Primary `%APPDATA%\ZaloData\config.json` SHA-256 & live `Zalo*` process invariance
 * 3. Orphan managed/fixture process snapshots before & after test runs
 * 4. Isolated temp-sandbox lifecycle (`os.tmpdir()`, never touching real Zalo or default MultiZaloDesktop)
 * 5. Bounded execution & high-resolution timing helpers (zero infinite wait loops)
 */

import crypto from 'node:crypto';
import { execFile, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export const ACCEPTANCE_ROOT = path.resolve(__dirname, '..');
export const EVIDENCE_DIR = path.resolve(ACCEPTANCE_ROOT, 'evidence');
export const DIM_ROOT = path.resolve(ACCEPTANCE_ROOT, '..', '..');
export const WORKTREE_ROOT = path.resolve(DIM_ROOT, '..');
export const WORKTREE_PARENT = path.dirname(WORKTREE_ROOT);
export const QA_DOCS_DIR = path.resolve(DIM_ROOT, 'docs', 'qa', 'windows');

export const HOST_SCRIPT_PATH = path.resolve(DIM_ROOT, 'scripts', 'dim-host.ps1');
export const FIXTURE_APP_PATH = path.resolve(DIM_ROOT, 'fixtures', 'test-desktop-app', 'fixture-app.mjs');
export const UI_SERVER_PATH = path.resolve(DIM_ROOT, 'apps', 'desktop', 'ui-server.mjs');
export const UI_DIR_PATH = path.resolve(DIM_ROOT, 'apps', 'desktop', 'ui');

export function resolveCargoTargetDir() {
  return process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR)
    : path.join(os.tmpdir(), 'zalo-qa-win-acceptance-target');
}

export function resolveCompiledBinaries(targetDir = resolveCargoTargetDir()) {
  const releaseDir = path.join(targetDir, 'release');
  const dimCliExe = path.join(releaseDir, 'dim-cli.exe');
  const dimDesktopExe = path.join(releaseDir, 'dim-desktop.exe');
  const dimTestFixtureExe = path.join(releaseDir, 'dim-test-fixture.exe');
  return {
    targetDir,
    releaseDir,
    dimCliExe,
    dimCliExists: fs.existsSync(dimCliExe),
    dimDesktopExe,
    dimDesktopExists: fs.existsSync(dimDesktopExe),
    dimTestFixtureExe,
    dimTestFixtureExists: fs.existsSync(dimTestFixtureExe),
  };
}

function escapeRegExp(str) {
  return String(str).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function buildPathVariants(rawPath) {
  if (!rawPath || typeof rawPath !== 'string') return [];
  const clean = rawPath.replace(/[\\/]+$/, '');
  if (clean.length < 3) return [];
  const winSlash = clean.replace(/\//g, '\\');
  const posixSlash = clean.replace(/\\/g, '/');
  const jsonEscaped = winSlash.replace(/\\/g, '\\\\');
  return Array.from(new Set([jsonEscaped, winSlash, posixSlash])).sort((a, b) => b.length - a.length);
}

/**
 * Scrubs personal paths, usernames, hostnames, and credential-like tokens from text.
 */
export function redactText(input, extraReplacements = []) {
  if (input == null) return '';
  let out = String(input);

  const orderedMappings = [
    ...extraReplacements,
    { raw: WORKTREE_PARENT, token: '<WORKTREE_ROOT>' },
    { raw: os.tmpdir(), token: '<TEMP_DIR>' },
    { raw: process.env.TEMP, token: '<TEMP_DIR>' },
    { raw: process.env.TMP, token: '<TEMP_DIR>' },
    { raw: process.env.LOCALAPPDATA, token: '<USER_HOME>/AppData/Local' },
    { raw: process.env.APPDATA, token: '<USER_HOME>/AppData/Roaming' },
    { raw: process.env.USERPROFILE, token: '<USER_HOME>' },
    { raw: os.homedir(), token: '<USER_HOME>' },
  ];

  for (const { raw, token } of orderedMappings) {
    for (const variant of buildPathVariants(raw)) {
      out = out.replace(new RegExp(escapeRegExp(variant), 'gi'), token);
    }
  }

  // Catch any remaining Windows user profile paths regardless of casing or slash style
  out = out.replace(/[A-Za-z]:(?:\\\\|\\|\/)+Users(?:\\\\|\\|\/)+[^\\/"'\s<>|]+/gi, '<USER_HOME>');

  let username = '';
  try {
    username = os.userInfo().username || process.env.USERNAME || '';
  } catch {
    username = process.env.USERNAME || '';
  }
  if (username && username.length >= 2) {
    out = out.replace(new RegExp(escapeRegExp(username), 'gi'), '<USER>');
  }

  const hostname = os.hostname() || process.env.COMPUTERNAME || '';
  if (hostname && hostname.length >= 2) {
    out = out.replace(new RegExp(`\\b${escapeRegExp(hostname)}\\b`, 'gi'), '<HOSTNAME>');
  }

  // Redact secret/session patterns
  out = out.replace(
    /(token|cookie|password|secret|authorization|session_key|zpw_sek)\s*([:=])\s*([^\s,;"'}\]]+)/gi,
    '$1$2<REDACTED>',
  );

  // Normalize mixed slashes after placeholder substitution for clean readability
  out = out.replace(/<WORKTREE_ROOT>\\\\+/g, '<WORKTREE_ROOT>/');
  out = out.replace(/<WORKTREE_ROOT>\\+/g, '<WORKTREE_ROOT>/');
  out = out.replace(/<USER_HOME>\\\\+/g, '<USER_HOME>/');
  out = out.replace(/<USER_HOME>\\+/g, '<USER_HOME>/');
  out = out.replace(/<TEMP_DIR>\\\\+/g, '<TEMP_DIR>/');
  out = out.replace(/<TEMP_DIR>\\+/g, '<TEMP_DIR>/');

  return out;
}

/**
 * Recursively clones and redacts all string values and keys inside an object/array.
 */
export function redactDeep(value, extraReplacements = []) {
  if (typeof value === 'string') {
    return redactText(value, extraReplacements);
  }
  if (Array.isArray(value)) {
    return value.map((item) => redactDeep(item, extraReplacements));
  }
  if (value && typeof value === 'object') {
    const out = {};
    for (const [k, v] of Object.entries(value)) {
      const safeKey = redactText(k, extraReplacements);
      out[safeKey] = redactDeep(v, extraReplacements);
    }
    return out;
  }
  return value;
}

/**
 * Verifies a serialized report string contains zero personal paths, usernames, or hostnames.
 */
export function assertZeroPiiLeaks(serializedText, label = 'artifact') {
  const leaks = [];
  let username = '';
  try {
    username = os.userInfo().username || process.env.USERNAME || '';
  } catch {
    username = process.env.USERNAME || '';
  }
  if (username && username.length >= 2 && new RegExp(escapeRegExp(username), 'i').test(serializedText)) {
    leaks.push(`username(${username})`);
  }
  const hostname = os.hostname() || process.env.COMPUTERNAME || '';
  if (hostname && hostname.length >= 3 && new RegExp(`\\b${escapeRegExp(hostname)}\\b`, 'i').test(serializedText)) {
    leaks.push(`hostname(${hostname})`);
  }
  if (/[A-Za-z]:(?:\\\\|\\|\/)+Users(?:\\\\|\\|\/)+/i.test(serializedText)) {
    leaks.push('raw_windows_users_path');
  }
  if (leaks.length > 0) {
    throw new Error(`PII redaction check failed for ${label}: detected ${leaks.join(', ')}`);
  }
}

/**
 * Redacts, verifies zero PII leaks, and atomically writes JSON evidence.
 */
export function writeRedactedJson(filePath, payload, extraReplacements = []) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  const cleaned = redactDeep(payload, extraReplacements);
  const serialized = `${JSON.stringify(cleaned, null, 2)}\n`;
  assertZeroPiiLeaks(serialized, path.basename(filePath));
  const tmpPath = `${filePath}.tmp.${process.pid}`;
  fs.writeFileSync(tmpPath, serialized, 'utf8');
  fs.renameSync(tmpPath, filePath);
  return cleaned;
}

export function sha256FileReadOnly(filePath) {
  if (!filePath || !fs.existsSync(filePath)) return null;
  const buf = fs.readFileSync(filePath);
  return crypto.createHash('sha256').update(buf).digest('hex');
}

/**
 * Executes a bounded PowerShell command and returns stdout/stderr without hanging.
 */
export function runPowerShellJson(psCommand, { timeoutMs = 15_000 } = {}) {
  const raw = execFileSync(
    'powershell.exe',
    ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', psCommand],
    {
      encoding: 'utf8',
      timeout: timeoutMs,
      windowsHide: true,
      maxBuffer: 16 * 1024 * 1024,
    },
  );
  const trimmed = (raw || '').trim();
  if (!trimmed) return [];
  const parsed = JSON.parse(trimmed);
  return Array.isArray(parsed) ? parsed : [parsed];
}

/**
 * Non-destructively captures primary `%APPDATA%\ZaloData\config.json` SHA-256 and running `Zalo*` PIDs.
 */
export function capturePrimaryZaloSnapshot() {
  const appData = process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming');
  const primaryZaloDir = path.join(appData, 'ZaloData');
  const configPath = path.join(primaryZaloDir, 'config.json');
  const dirExists = fs.existsSync(primaryZaloDir);
  const configExists = fs.existsSync(configPath);
  let configSizeBytes = null;
  let configSha256 = null;

  if (configExists) {
    const stat = fs.statSync(configPath);
    configSizeBytes = stat.size;
    configSha256 = sha256FileReadOnly(configPath);
  }

  let zaloProcesses = [];
  try {
    const rows = runPowerShellJson(
      "Get-CimInstance Win32_Process -Filter \"Name LIKE 'Zalo%'\" | Select-Object ProcessId, Name, CreationDate | ConvertTo-Json -Compress",
    );
    zaloProcesses = rows
      .filter((r) => r && Number.isInteger(Number(r.ProcessId)))
      .map((r) => ({
        pid: Number(r.ProcessId),
        name: String(r.Name || ''),
        creationDate: String(r.CreationDate || ''),
      }))
      .sort((a, b) => a.pid - b.pid);
  } catch {
    zaloProcesses = [];
  }

  return {
    capturedAtUtc: new Date().toISOString(),
    primaryZaloDirRedacted: redactText(primaryZaloDir),
    dirExists,
    configExists,
    configSizeBytes,
    configSha256,
    primaryProcessCount: zaloProcesses.length,
    primaryPids: zaloProcesses.map((p) => p.pid),
    primaryProcesses: zaloProcesses,
  };
}

/**
 * Compares before/after snapshots of primary Zalo to prove 100% non-interference.
 */
export function verifyPrimaryZaloInvariance(before, after) {
  const configExistsUnchanged = Boolean(before?.configExists) === Boolean(after?.configExists);
  const configSha256Identical = before?.configSha256 === after?.configSha256;
  const afterPidSet = new Set(after?.primaryPids || []);
  const missingPrimaryPids = (before?.primaryPids || []).filter((pid) => !afterPidSet.has(pid));

  const passed = configExistsUnchanged && configSha256Identical && missingPrimaryPids.length === 0;
  return {
    passed,
    configExistsBefore: Boolean(before?.configExists),
    configExistsAfter: Boolean(after?.configExists),
    configExistsUnchanged,
    configSha256Before: before?.configSha256 || null,
    configSha256After: after?.configSha256 || null,
    configSha256Identical,
    primaryProcessCountBefore: before?.primaryProcessCount ?? 0,
    primaryProcessCountAfter: after?.primaryProcessCount ?? 0,
    missingPrimaryPids,
  };
}

/**
 * Snapshots all `dim-*`, fixture, and UI-server processes so suites can prove zero orphan leaks.
 */
export function captureManagedProcessSnapshot() {
  const selfPid = process.pid;
  const parentPid = process.ppid;
  let rows = [];
  try {
    rows = runPowerShellJson(
      "Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(dim-|xf-desktop-fixture|node\\.exe$)' } | Select-Object ProcessId, ParentProcessId, Name, CommandLine | ConvertTo-Json -Compress",
    );
  } catch {
    rows = [];
  }

  const relevant = [];
  for (const r of rows) {
    if (!r) continue;
    const pid = Number(r.ProcessId);
    if (!Number.isInteger(pid) || pid <= 0 || pid === selfPid || pid === parentPid) continue;
    const name = String(r.Name || '');
    const cmd = String(r.CommandLine || '');
    const isDimBinary = /^(dim-desktop|dim-cli|dim-test-fixture|xf-desktop-fixture)/i.test(name);
    const isManagedNode =
      /^node(\.exe)?$/i.test(name) &&
      /(fixture-app\.mjs|ui-server\.mjs|zalo-win-qa-)/i.test(cmd);
    if (isDimBinary || isManagedNode) {
      relevant.push({
        pid,
        ppid: Number(r.ParentProcessId || 0),
        name,
        commandLineRedacted: redactText(cmd),
      });
    }
  }

  relevant.sort((a, b) => a.pid - b.pid);
  return {
    capturedAtUtc: new Date().toISOString(),
    count: relevant.length,
    pids: relevant.map((p) => p.pid),
    processes: relevant,
  };
}

/**
 * Computes any newly left-behind managed/fixture processes between `before` and `after`.
 */
export function diffOrphanProcesses(before, after, { ignorePids = [] } = {}) {
  const baseline = new Set([...(before?.pids || []), ...ignorePids, process.pid, process.ppid]);
  const orphans = (after?.processes || []).filter((proc) => !baseline.has(proc.pid));
  return {
    clean: orphans.length === 0,
    orphanCount: orphans.length,
    orphans,
  };
}

/**
 * Creates an isolated sandbox directory in `%TEMP%` with tracked cleanup callbacks.
 */
export function createSandboxEnv(label = 'suite') {
  const safeLabel = String(label).replace(/[^a-zA-Z0-9_-]/g, '-').slice(0, 32);
  const rootDir = fs.mkdtempSync(path.join(os.tmpdir(), `zalo-win-qa-${safeLabel}-`));
  const dataDir = path.join(rootDir, 'dim-data');
  fs.mkdirSync(dataDir, { recursive: true });

  const cleanups = [];
  const trackedPids = new Set();

  return {
    rootDir,
    dataDir,
    rootDirRedacted: redactText(rootDir),
    dataDirRedacted: redactText(dataDir),
    trackPid(pid) {
      if (Number.isInteger(pid) && pid > 0) trackedPids.add(pid);
    },
    onCleanup(fn) {
      if (typeof fn === 'function') cleanups.push(fn);
    },
    async cleanup({ primaryProtectedPids = [] } = {}) {
      const protectedSet = new Set(primaryProtectedPids);
      for (let i = cleanups.length - 1; i >= 0; i -= 1) {
        try {
          await cleanups[i]();
        } catch {
          // Best-effort cleanup
        }
      }
      for (const pid of trackedPids) {
        if (protectedSet.has(pid) || pid === process.pid || pid === process.ppid) continue;
        try {
          process.kill(pid, 'SIGTERM');
        } catch {
          // Already exited
        }
      }
      await sleep(120);
      try {
        fs.rmSync(rootDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
      } catch {
        // Fallback via cmd rmdir if NTFS attributes/ACLs needed reset
        try {
          execFileSync('icacls.exe', [rootDir, '/reset', '/T', '/C', '/Q'], {
            windowsHide: true,
            timeout: 5_000,
          });
          fs.rmSync(rootDir, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
        } catch {
          // Ignore final temp cleanup error
        }
      }
    },
  };
}

export function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export class Stopwatch {
  constructor() {
    this.startedAt = performance.now();
  }

  elapsedMs() {
    return Number((performance.now() - this.startedAt).toFixed(2));
  }
}

export async function measureAsync(fn) {
  const sw = new Stopwatch();
  const result = await fn();
  return { result, durationMs: sw.elapsedMs() };
}

export async function withTimeout(promise, timeoutMs, label = 'operation') {
  let timer = null;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => {
          const err = new Error(`${label} timed out after ${timeoutMs}ms`);
          err.code = 'ETIMEDOUT';
          reject(err);
        }, timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

/**
 * Executes `dim-cli.exe` or `scripts/dim-host.ps1` with bounded timeout and JSON parsing.
 */
export function runBackendCli({
  mode = 'rust', // 'rust' | 'ps1'
  dimCliExe = resolveCompiledBinaries().dimCliExe,
  dataDir,
  flags = [],
  positional = [],
  timeoutMs = 20_000,
  envOverrides = {},
}) {
  return new Promise((resolve) => {
    const sw = new Stopwatch();
    const cliArgs = ['--cli', '--json'];
    if (dataDir) {
      cliArgs.push('--data-dir', dataDir);
    }
    cliArgs.push(...flags);
    if (positional.length > 0) {
      cliArgs.push('--', ...positional);
    }

    const exe = mode === 'rust' ? dimCliExe : 'powershell.exe';
    const fullArgs =
      mode === 'rust'
        ? cliArgs
        : ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', HOST_SCRIPT_PATH, ...cliArgs];

    execFile(
      exe,
      fullArgs,
      {
        encoding: 'utf8',
        timeout: timeoutMs,
        killSignal: 'SIGTERM',
        windowsHide: true,
        maxBuffer: 16 * 1024 * 1024,
        env: { ...process.env, ...envOverrides },
      },
      (error, stdout, stderr) => {
        const durationMs = sw.elapsedMs();
        const timedOut = Boolean(error && (error.killed || error.code === 'ETIMEDOUT' || error.signal === 'SIGTERM'));
        const trimmed = (stdout || '').trim();
        let json = null;
        if (trimmed) {
          try {
            json = JSON.parse(trimmed);
          } catch {
            json = null;
          }
        }
        resolve({
          ok: Boolean(json && json.ok === true && !error),
          exitCode: error ? (typeof error.code === 'number' ? error.code : 1) : 0,
          timedOut,
          durationMs,
          json,
          stdoutRedacted: redactText(trimmed),
          stderrRedacted: redactText(stderr || error?.message || ''),
        });
      },
    );
  });
}
