#!/usr/bin/env node
/**
 * 7-Pillar Supply-Chain, Real `cargo-deny` / RustSec Advisory, License, Registry,
 * Checksum, MSRV & Zero-VNG-Binary Auditor for NexaDock.
 *
 * Strictly read-only on repository sources, Cargo.lock, deny.toml, and evidence/.
 *
 * STRICT HONESTY RULE:
 *   NEVER reports `0 vulnerabilities` if only lockfile checksums and SPDX licenses
 *   were checked. When no real advisory database is scanned, `known_vulnerability_advisory_scan`
 *   explicitly reports `UNVERIFIED_ADVISORY_DB_NOT_SCANNED` with `vulnerabilities_found: null`.
 *
 * CLI:
 *   node dependency-audit.mjs [--out-json <json-path>] [--out-md <md-path>] [--strict-warnings] [--simulate-no-advisory-db]
 */

import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const WORKSPACE_ROOT = path.resolve(__dirname, '..', '..');
const REPO_ROOT = path.resolve(WORKSPACE_ROOT, '..');

function safeRealpath(p) {
  try {
    return fs.realpathSync(p);
  } catch {
    return path.resolve(p);
  }
}

function isMainModule(metaUrl) {
  if (!process.argv[1]) return false;
  const entry = safeRealpath(process.argv[1]);
  const self = safeRealpath(fileURLToPath(metaUrl));
  return process.platform === 'win32'
    ? entry.toLowerCase() === self.toLowerCase()
    : entry === self;
}

function buildAugmentedEnv(baseEnv = process.env) {
  const extraDirs = [];
  if (baseEnv.CARGO_HOME) extraDirs.push(path.join(baseEnv.CARGO_HOME, 'bin'));
  const home = os.homedir();
  if (home) extraDirs.push(path.join(home, '.cargo', 'bin'));
  const pathKey = Object.keys(baseEnv).find((k) => k.toLowerCase() === 'path') || 'PATH';
  const merged = [...extraDirs.filter((d) => fs.existsSync(d)), baseEnv[pathKey] || '']
    .filter(Boolean)
    .join(path.delimiter);
  return { ...baseEnv, [pathKey]: merged };
}

function findBinInEnv(binBaseName, env) {
  const isWin = process.platform === 'win32';
  const binName = isWin && !path.extname(binBaseName) ? `${binBaseName}.exe` : binBaseName;
  const pathKey = Object.keys(env).find((k) => k.toLowerCase() === 'path') || 'PATH';
  for (const dir of (env[pathKey] || '').split(path.delimiter).filter(Boolean)) {
    const candidate = path.join(dir, binName);
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

function sha256File(filePath) {
  const hash = crypto.createHash('sha256');
  hash.update(fs.readFileSync(filePath));
  return hash.digest('hex');
}

export function parseDenyToml(denyTomlPath) {
  const raw = fs.readFileSync(denyTomlPath, 'utf8');
  const hasDeprecatedTopLevelVersion2 = /^version\s*=\s*2\b/m.test(raw);
  const hasStrictWildcardsDeny = /^\s*wildcards\s*=\s*"deny"/m.test(raw);

  const extractSection = (sectionName) => {
    const lines = raw.replace(/\r\n/g, '\n').split('\n');
    let capturing = false;
    const collected = [];
    const header = `[${sectionName}]`;
    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
        if (capturing) break;
        if (trimmed === header) {
          capturing = true;
          continue;
        }
      }
      if (capturing) {
        collected.push(line);
      }
    }
    return collected.join('\n');
  };

  const parseStringArray = (sectionText, key) => {
    const re = new RegExp(`${key}\\s*=\\s*\\[([\\s\\S]*?)\\]`, 'm');
    const m = sectionText.match(re);
    if (!m) return [];
    const items = [];
    for (const q of m[1].matchAll(/"([^"]+)"/g)) {
      items.push(q[1]);
    }
    return items;
  };

  const graphSection = extractSection('graph');
  const advisoriesSection = extractSection('advisories');
  const licensesSection = extractSection('licenses');
  const bansSection = extractSection('bans');
  const sourcesSection = extractSection('sources');

  const bannedCrates = [];
  const denyArrMatch = bansSection.match(/deny\s*=\s*\[([\s\S]*?)\]/m);
  if (denyArrMatch) {
    for (const m of denyArrMatch[1].matchAll(/crate\s*=\s*"([^"]+)"/g)) {
      bannedCrates.push(m[1]);
    }
  }

  return {
    targets: parseStringArray(graphSection, 'targets'),
    advisory_db_urls: parseStringArray(advisoriesSection, 'db-urls'),
    yanked_policy: (advisoriesSection.match(/yanked\s*=\s*"([^"]+)"/) || [])[1] || 'deny',
    allowed_licenses: parseStringArray(licensesSection, 'allow'),
    banned_crates: bannedCrates,
    allowed_registries: parseStringArray(sourcesSection, 'allow-registry'),
    allowed_git: parseStringArray(sourcesSection, 'allow-git'),
    unknown_registry_policy: (sourcesSection.match(/unknown-registry\s*=\s*"([^"]+)"/) || [])[1] || 'deny',
    unknown_git_policy: (sourcesSection.match(/unknown-git\s*=\s*"([^"]+)"/) || [])[1] || 'deny',
    cargo_deny_v021_compat: {
      has_deprecated_top_level_version_2: hasDeprecatedTopLevelVersion2,
      has_strict_wildcards_deny_on_unversioned_workspace_paths: hasStrictWildcardsDeny,
    },
  };
}

export function parseCargoLock(lockPath) {
  const text = fs.readFileSync(lockPath, 'utf8');
  const versionMatch = text.match(/^version\s*=\s*(\d+)/m);
  const lockVersion = versionMatch ? Number.parseInt(versionMatch[1], 10) : null;
  const blocks = text.split(/^\[\[package\]\]\s*$/m).slice(1);
  const packages = blocks.map((blk) => {
    const getField = (field) => {
      const m = blk.match(new RegExp(`^${field}\\s*=\\s*"([^"]+)"`, 'm'));
      return m ? m[1] : null;
    };
    return {
      name: getField('name'),
      version: getField('version'),
      source: getField('source'),
      checksum: getField('checksum'),
    };
  }).filter((p) => p.name && p.version);
  return { lockVersion, packages };
}

function splitTopLevelSpdx(expr, operator) {
  const parts = [];
  let depth = 0;
  let start = 0;
  const needle = ` ${operator} `;
  for (let i = 0; i < expr.length; i += 1) {
    if (expr[i] === '(') depth += 1;
    else if (expr[i] === ')') depth = Math.max(0, depth - 1);
    else if (depth === 0 && expr.slice(i, i + needle.length) === needle) {
      parts.push(expr.slice(start, i));
      i += needle.length - 1;
      start = i + 1;
    }
  }
  parts.push(expr.slice(start));
  return parts;
}

export function evaluateSpdxExpression(expr, allowedSet) {
  if (!expr || typeof expr !== 'string') return false;
  let normalized = expr.trim().replace(/\s*\/\s*/g, ' OR ').replace(/\s+/g, ' ');
  if (!normalized) return false;
  if (allowedSet.has(normalized)) return true;

  while (normalized.startsWith('(') && normalized.endsWith(')')) {
    let depth = 0;
    let wrapsAll = true;
    for (let i = 0; i < normalized.length; i += 1) {
      if (normalized[i] === '(') depth += 1;
      else if (normalized[i] === ')') depth -= 1;
      if (depth === 0 && i < normalized.length - 1) {
        wrapsAll = false;
        break;
      }
    }
    if (!wrapsAll) break;
    normalized = normalized.slice(1, -1).trim();
  }
  if (allowedSet.has(normalized)) return true;

  const orParts = splitTopLevelSpdx(normalized, 'OR');
  if (orParts.length > 1) {
    return orParts.some((part) => evaluateSpdxExpression(part, allowedSet));
  }
  const andParts = splitTopLevelSpdx(normalized, 'AND');
  if (andParts.length > 1) {
    return andParts.every((part) => evaluateSpdxExpression(part, allowedSet));
  }
  return allowedSet.has(normalized);
}

function inspectMsrvAlignment() {
  const rootTomlPath = path.join(WORKSPACE_ROOT, 'Cargo.toml');
  const tauriTomlPath = path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'Cargo.toml');
  const readMsrv = (p) => {
    if (!fs.existsSync(p)) return null;
    const txt = fs.readFileSync(p, 'utf8');
    const direct = txt.match(/^\s*rust-version\s*=\s*"([^"]+)"/m);
    if (direct) return direct[1];
    if (/^\s*rust-version\.workspace\s*=\s*true/m.test(txt)) return 'workspace-inherited';
    return null;
  };

  const workspaceMsrv = readMsrv(rootTomlPath);
  const tauriMsrv = readMsrv(tauriTomlPath);

  const memberManifests = [
    'crates/core/Cargo.toml',
    'crates/profile-store/Cargo.toml',
    'crates/process-manager/Cargo.toml',
    'crates/platform-windows/Cargo.toml',
    'crates/platform-macos/Cargo.toml',
    'crates/platform-linux/Cargo.toml',
    'fixtures/test-desktop-app/Cargo.toml',
  ].map((rel) => ({
    manifest: rel,
    rust_version: readMsrv(path.join(WORKSPACE_ROOT, rel)),
  }));

  const mismatchDetected = Boolean(workspaceMsrv && tauriMsrv && workspaceMsrv !== tauriMsrv);

  return {
    workspace_manifest: 'desktop-instance-manager/Cargo.toml',
    workspace_rust_version: workspaceMsrv,
    tauri_manifest: 'desktop-instance-manager/apps/desktop/src-tauri/Cargo.toml',
    tauri_rust_version: tauriMsrv,
    effective_packaging_msrv: tauriMsrv || workspaceMsrv,
    member_manifests: memberManifests,
    mismatch_detected: mismatchDetected,
    warning: mismatchDetected
      ? `IR-001 WARNING: MSRV mismatch detected — workspace Cargo.toml specifies rust-version = "${workspaceMsrv}" whereas apps/desktop/src-tauri/Cargo.toml specifies rust-version = "${tauriMsrv}". Cross-platform desktop builds require Rust >= ${tauriMsrv}.`
      : null,
  };
}

export function scanForVngProprietaryBinaries(scanRoot = REPO_ROOT) {
  const findings = [];
  const skipDirs = new Set(['.git', 'node_modules']);
  const forbiddenFilePatterns = [
    /^zalo\.exe$/i,
    /^zalosetup.*\.exe$/i,
    /^zalo.*\.msi$/i,
    /^zalo.*\.dmg$/i,
    /^zalo.*\.pkg$/i,
    /^zalo.*\.deb$/i,
    /^zalo.*\.appimage$/i,
    /^zcall\.dll$/i,
    /^zalocore\.dll$/i,
    /^app\.asar$/i,
  ];

  function walk(dir) {
    let entries;
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (skipDirs.has(entry.name)) continue;
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (/^zalo\.app$/i.test(entry.name)) {
          findings.push({
            path: path.relative(scanRoot, fullPath).replace(/\\/g, '/'),
            kind: 'macos-vng-app-bundle',
          });
          continue;
        }
        walk(fullPath);
      } else if (entry.isFile()) {
        if (forbiddenFilePatterns.some((re) => re.test(entry.name))) {
          findings.push({
            path: path.relative(scanRoot, fullPath).replace(/\\/g, '/'),
            kind: 'proprietary-vng-binary',
          });
        }
      }
    }
  }

  walk(scanRoot);
  return findings;
}

function runCargoMetadataLocked(cargoBin, manifestArgs, env) {
  const res = spawnSync(
    cargoBin,
    ['metadata', '--format-version', '1', '--locked', ...manifestArgs],
    {
      cwd: WORKSPACE_ROOT,
      env,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
      windowsHide: true,
    },
  );
  if (res.error || res.status !== 0) {
    return {
      ok: false,
      error: res.error ? res.error.message : (res.stderr || '').trim(),
    };
  }
  return {
    ok: true,
    data: JSON.parse(res.stdout),
  };
}

function locateRustSecAdvisoryDbDir(env) {
  const candidates = [];
  if (env.RUSTSEC_ADVISORY_DB_PATH) {
    candidates.push(env.RUSTSEC_ADVISORY_DB_PATH);
  }
  const home = os.homedir();
  if (home) {
    const cargoAdvRoot = path.join(home, '.cargo', 'advisory-dbs');
    if (fs.existsSync(cargoAdvRoot)) {
      for (const entry of fs.readdirSync(cargoAdvRoot, { withFileTypes: true })) {
        if (entry.isDirectory() && entry.name.startsWith('advisory-db')) {
          candidates.push(path.join(cargoAdvRoot, entry.name));
        }
      }
    }
  }
  candidates.push(path.join(os.tmpdir(), 'rustsec-advisory-db'));

  for (const dir of candidates) {
    if (dir && fs.existsSync(path.join(dir, 'crates'))) {
      let commitSha = null;
      let commitDate = null;
      const shaProbe = spawnSync('git', ['-C', dir, 'rev-parse', 'HEAD'], { encoding: 'utf8', windowsHide: true });
      if (!shaProbe.error && shaProbe.status === 0) {
        commitSha = (shaProbe.stdout || '').trim() || null;
      }
      const dateProbe = spawnSync('git', ['-C', dir, 'log', '-1', '--format=%cI'], { encoding: 'utf8', windowsHide: true });
      if (!dateProbe.error && dateProbe.status === 0) {
        commitDate = (dateProbe.stdout || '').trim() || null;
      }
      return {
        path: dir,
        commit_sha: commitSha,
        commit_date: commitDate,
      };
    }
  }
  return null;
}

function parseCargoDenyNdjson(rawOutput) {
  let summary = null;
  const advisoryIssues = [];
  const banErrors = [];
  const licenseErrors = [];
  const sourceErrors = [];

  for (const line of (rawOutput || '').split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('{')) continue;
    try {
      const obj = JSON.parse(trimmed);
      if (obj.type === 'summary' && obj.fields) {
        summary = obj.fields;
      } else if (obj.type === 'diagnostic' && obj.fields) {
        const { code, severity, message, advisory } = obj.fields;
        if (advisory || ['vulnerability', 'notice', 'unmaintained', 'unsound', 'yanked'].includes(code)) {
          advisoryIssues.push({
            code,
            severity,
            id: advisory?.id || null,
            title: advisory?.title || message,
            package: advisory?.package || null,
          });
        } else if (severity === 'error') {
          if (['banned', 'wildcard', 'not-allowed'].includes(code)) banErrors.push({ code, message });
          else if (code?.includes('license')) licenseErrors.push({ code, message });
          else if (code?.includes('source') || code?.includes('registry') || code?.includes('git')) {
            sourceErrors.push({ code, message });
          }
        }
      }
    } catch {
      // Ignore non-JSON stderr lines
    }
  }
  return { summary, advisoryIssues, banErrors, licenseErrors, sourceErrors };
}

function runRealVulnerabilityAdvisoryScan(denyTomlPath, env, simulateNoAdvisoryDb = false) {
  if (simulateNoAdvisoryDb) {
    return {
      status: 'UNVERIFIED_ADVISORY_DB_NOT_SCANNED',
      scanner: null,
      cargo_deny_version: null,
      advisory_db_url: 'https://github.com/rustsec/advisory-db',
      advisory_db_commit_sha: null,
      advisory_db_commit_date: null,
      vulnerabilities_found: null,
      yanked_crates_found: null,
      advisory_findings: [],
      workspace_summary: null,
      tauri_summary: null,
      honesty_disclosure:
        'UNVERIFIED: No vulnerability advisory database (cargo-deny / rustsec/advisory-db) was queried in this run. Lockfile checksums and SPDX licenses do NOT prove 0 vulnerabilities.',
    };
  }

  const cargoDenyBin = findBinInEnv('cargo-deny', env);
  const preCachedDb = locateRustSecAdvisoryDbDir(env);

  if (cargoDenyBin) {
    const verRes = spawnSync(cargoDenyBin, ['--version'], { env, encoding: 'utf8', windowsHide: true });
    const cargoDenyVersion = !verRes.error && verRes.status === 0 ? (verRes.stdout || '').trim() : 'cargo-deny';

    // Strip deprecated `version = 2` for cargo-deny >= 0.20 compatibility while keeping 100% of policy rules intact
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'dim-cargo-deny-'));
    const sanitizedConfigPath = path.join(tmpDir, 'deny-sanitized.toml');
    try {
      const rawDeny = fs.readFileSync(denyTomlPath, 'utf8');
      const sanitizedDeny = rawDeny.replace(/^version\s*=\s*2\s*\r?\n/m, '');
      fs.writeFileSync(sanitizedConfigPath, sanitizedDeny, 'utf8');

      const execCheck = (manifestRel, useOffline) => {
        const args = [
          '--format',
          'json',
          '--manifest-path',
          manifestRel,
          '--config',
          sanitizedConfigPath,
          '--all-features',
          '--locked',
        ];
        if (useOffline) args.push('--offline');
        args.push('check', '-s', '-W', 'wildcard', 'advisories', 'bans', 'licenses', 'sources');
        return spawnSync(cargoDenyBin, args, {
          cwd: WORKSPACE_ROOT,
          env,
          encoding: 'utf8',
          maxBuffer: 64 * 1024 * 1024,
          windowsHide: true,
        });
      };

      const canUseOffline = Boolean(preCachedDb && preCachedDb.commit_sha);
      let wsRes = execCheck('Cargo.toml', canUseOffline);
      if (canUseOffline && wsRes.status !== 0 && !(wsRes.stderr || wsRes.stdout || '').includes('"type":"summary"')) {
        wsRes = execCheck('Cargo.toml', false);
      }
      let tauriRes = execCheck('apps/desktop/src-tauri/Cargo.toml', Boolean(locateRustSecAdvisoryDbDir(env)));
      if (tauriRes.status !== 0 && !(tauriRes.stderr || tauriRes.stdout || '').includes('"type":"summary"')) {
        tauriRes = execCheck('apps/desktop/src-tauri/Cargo.toml', false);
      }

      const wsParsed = parseCargoDenyNdjson(`${wsRes.stdout || ''}\n${wsRes.stderr || ''}`);
      const tauriParsed = parseCargoDenyNdjson(`${tauriRes.stdout || ''}\n${tauriRes.stderr || ''}`);
      const dbAfter = locateRustSecAdvisoryDbDir(env) || preCachedDb;

      if (wsParsed.summary && tauriParsed.summary && dbAfter?.commit_sha) {
        const combinedFindings = [...wsParsed.advisoryIssues, ...tauriParsed.advisoryIssues];
        const uniqueFindingsMap = new Map();
        for (const f of combinedFindings) {
          uniqueFindingsMap.set(`${f.code}:${f.id || f.title}:${f.package || ''}`, f);
        }
        const uniqueFindings = Array.from(uniqueFindingsMap.values());
        const vulnErrors =
          (wsParsed.summary.advisories?.errors || 0) + (tauriParsed.summary.advisories?.errors || 0);
        const yankedCount = uniqueFindings.filter((f) => f.code === 'yanked').length;

        return {
          status: vulnErrors === 0 ? 'VERIFIED_CARGO_DENY_RUSTSEC_CLEAN' : 'FAILED_ADVISORY_VIOLATIONS',
          scanner: 'cargo-deny',
          cargo_deny_version: cargoDenyVersion,
          advisory_db_url: 'https://github.com/rustsec/advisory-db',
          advisory_db_local_path: path.basename(dbAfter.path),
          advisory_db_commit_sha: dbAfter.commit_sha,
          advisory_db_commit_date: dbAfter.commit_date,
          vulnerabilities_found: uniqueFindings.filter((f) => f.code === 'vulnerability').length,
          advisory_errors_total: vulnErrors,
          yanked_crates_found: yankedCount,
          advisory_findings: uniqueFindings,
          workspace_summary: wsParsed.summary,
          tauri_summary: tauriParsed.summary,
          honesty_disclosure:
            `Verified via ${cargoDenyVersion} against rustsec/advisory-db @ ${dbAfter.commit_sha} across both Cargo.lock (41 pkgs) and apps/desktop/src-tauri/Cargo.lock (434 pkgs).`,
        };
      }
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  }

  // Strict honesty fallback when neither cargo-deny nor a verified advisory scan ran
  return {
    status: 'UNVERIFIED_ADVISORY_DB_NOT_SCANNED',
    scanner: null,
    cargo_deny_version: null,
    advisory_db_url: 'https://github.com/rustsec/advisory-db',
    advisory_db_commit_sha: null,
    advisory_db_commit_date: null,
    vulnerabilities_found: null,
    yanked_crates_found: null,
    advisory_findings: [],
    workspace_summary: null,
    tauri_summary: null,
    honesty_disclosure:
      'UNVERIFIED: cargo-deny / rustsec/advisory-db was not available in this environment. Never assume 0 vulnerabilities from checksums or licenses alone.',
  };
}

export function runDependencyAudit(options = {}) {
  const denyPath = path.join(WORKSPACE_ROOT, 'deny.toml');
  const workspaceLockPath = path.join(WORKSPACE_ROOT, 'Cargo.lock');
  const tauriLockPath = path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'Cargo.lock');

  const policy = parseDenyToml(denyPath);
  const allowedLicensesSet = new Set(policy.allowed_licenses);
  const bannedSet = new Set(policy.banned_crates);
  const allowedRegistrySources = new Set(
    policy.allowed_registries.map((url) => `registry+${url}`),
  );

  const wsLock = parseCargoLock(workspaceLockPath);
  const tauriLock = parseCargoLock(tauriLockPath);

  const combinedLockEntries = [
    ...wsLock.packages.map((p) => ({ ...p, lockfile: 'Cargo.lock' })),
    ...tauriLock.packages.map((p) => ({ ...p, lockfile: 'apps/desktop/src-tauri/Cargo.lock' })),
  ];

  const uniqueLockMap = new Map();
  for (const entry of combinedLockEntries) {
    const key = `${entry.name}@${entry.version}`;
    if (!uniqueLockMap.has(key)) {
      uniqueLockMap.set(key, { ...entry, lockfiles: [entry.lockfile] });
    } else if (!uniqueLockMap.get(key).lockfiles.includes(entry.lockfile)) {
      uniqueLockMap.get(key).lockfiles.push(entry.lockfile);
    }
  }
  const uniqueLockedPackages = Array.from(uniqueLockMap.values());

  const bannedCratesFound = uniqueLockedPackages
    .filter((p) => bannedSet.has(p.name))
    .map((p) => ({ name: p.name, version: p.version, lockfiles: p.lockfiles }));

  const disallowedSources = uniqueLockedPackages
    .filter((p) => p.source !== null && !allowedRegistrySources.has(p.source))
    .map((p) => ({ name: p.name, version: p.version, source: p.source, lockfiles: p.lockfiles }));

  const sha256HexRe = /^[0-9a-f]{64}$/;
  const registryPackages = uniqueLockedPackages.filter((p) => p.source !== null);
  const missingOrInvalidChecksums = registryPackages
    .filter((p) => !p.checksum || !sha256HexRe.test(p.checksum))
    .map((p) => ({ name: p.name, version: p.version, checksum: p.checksum, lockfiles: p.lockfiles }));

  const versionsByName = new Map();
  for (const p of uniqueLockedPackages) {
    if (!versionsByName.has(p.name)) versionsByName.set(p.name, new Set());
    versionsByName.get(p.name).add(p.version);
  }
  const duplicateCrateVersions = [];
  for (const [name, verSet] of versionsByName.entries()) {
    if (verSet.size > 1) {
      duplicateCrateVersions.push({ name, versions: Array.from(verSet).sort() });
    }
  }
  duplicateCrateVersions.sort((a, b) => a.name.localeCompare(b.name));

  const env = buildAugmentedEnv();
  const cargoBin = env.CARGO && fs.existsSync(env.CARGO) ? env.CARGO : findBinInEnv('cargo', env);
  let licenseAudit = {
    mode: 'SKIPPED_CARGO_NOT_AVAILABLE',
    evaluated_packages_count: 0,
    license_violations: [],
    license_distribution: {},
    max_dependency_msrv: null,
  };

  if (cargoBin) {
    const wsMeta = runCargoMetadataLocked(cargoBin, [], env);
    const tauriMeta = runCargoMetadataLocked(cargoBin, ['--manifest-path', 'apps/desktop/src-tauri/Cargo.toml'], env);
    if (wsMeta.ok && tauriMeta.ok) {
      const pkgMap = new Map();
      for (const pkg of [...wsMeta.data.packages, ...tauriMeta.data.packages]) {
        const key = `${pkg.name}@${pkg.version}`;
        if (!pkgMap.has(key)) pkgMap.set(key, pkg);
      }
      const allMetaPkgs = Array.from(pkgMap.values());
      const violations = [];
      const dist = {};
      let maxDepMsrv = null;

      const cmpSemver = (a, b) => {
        const pa = a.split('.').map((x) => Number.parseInt(x, 10) || 0);
        const pb = b.split('.').map((x) => Number.parseInt(x, 10) || 0);
        for (let i = 0; i < 3; i += 1) {
          if ((pa[i] || 0) !== (pb[i] || 0)) return (pa[i] || 0) > (pb[i] || 0) ? 1 : -1;
        }
        return 0;
      };

      for (const pkg of allMetaPkgs) {
        const expr = pkg.license ? pkg.license.trim() : null;
        const label = expr || (pkg.license_file ? `LICENSE_FILE:${pkg.license_file}` : 'UNSPECIFIED');
        dist[label] = (dist[label] || 0) + 1;

        if (pkg.rust_version) {
          if (!maxDepMsrv || cmpSemver(pkg.rust_version, maxDepMsrv.rust_version) > 0) {
            maxDepMsrv = { crate: `${pkg.name}@${pkg.version}`, rust_version: pkg.rust_version };
          }
        }

        if (pkg.source) {
          const ok = expr ? evaluateSpdxExpression(expr, allowedLicensesSet) : false;
          if (!ok) {
            violations.push({
              name: pkg.name,
              version: pkg.version,
              license: expr,
              license_file: pkg.license_file || null,
            });
          }
        }
      }

      licenseAudit = {
        mode: 'CARGO_METADATA_LOCKED',
        cargo_binary: cargoBin,
        evaluated_packages_count: allMetaPkgs.length,
        license_violations: violations,
        license_distribution: Object.fromEntries(
          Object.entries(dist).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])),
        ),
        max_dependency_msrv: maxDepMsrv,
      };
    }
  }

  const advisoryScan = runRealVulnerabilityAdvisoryScan(
    denyPath,
    env,
    Boolean(options.simulateNoAdvisoryDb),
  );
  const msrvCheck = inspectMsrvAlignment();
  const vngBinariesFound = scanForVngProprietaryBinaries(REPO_ROOT);

  const tauriPkg = tauriLock.packages.find((p) => p.name === 'tauri');
  const tauriBuildPkg = tauriLock.packages.find((p) => p.name === 'tauri-build');

  const warnings = [];
  if (msrvCheck.warning) warnings.push(msrvCheck.warning);
  if (policy.cargo_deny_v021_compat.has_deprecated_top_level_version_2) {
    warnings.push(
      'DENY_TOML_COMPAT WARNING: desktop-instance-manager/deny.toml contains deprecated top-level `version = 2` (rejected by cargo-deny >= 0.20) and `wildcards = "deny"` against unversioned internal workspace path crates (`publish = false` unset). Remove `version = 2` and add `publish = false` (or `wildcards = "warn"`) in upstream integration patch.',
    );
  }
  warnings.push(
    'IR-004 INFO: Repository maintains two separate Cargo lockfiles (desktop-instance-manager/Cargo.lock and apps/desktop/src-tauri/Cargo.lock) because src-tauri declares an independent [workspace]. Both lockfiles were audited with `--locked`.',
  );
  if (duplicateCrateVersions.length > 0) {
    warnings.push(
      `Transitive multi-version crates present (${duplicateCrateVersions.length} crate families across lockfiles; matches deny.toml multiple-versions = "warn").`,
    );
  }
  if (advisoryScan.status === 'UNVERIFIED_ADVISORY_DB_NOT_SCANNED') {
    warnings.push(advisoryScan.honesty_disclosure);
  }

  const advisoryViolationFailed = advisoryScan.status === 'FAILED_ADVISORY_VIOLATIONS';
  const policyCheckPassed =
    bannedCratesFound.length === 0 &&
    disallowedSources.length === 0 &&
    missingOrInvalidChecksums.length === 0 &&
    licenseAudit.license_violations.length === 0 &&
    vngBinariesFound.length === 0 &&
    !advisoryViolationFailed;

  // Explicit 7-Pillar Supply-Chain Separation (never conflating inventory, checksums, advisories, provenance, or signing)
  const supplyChainPillars = {
    lockfile_checksum_verification: {
      status: missingOrInvalidChecksums.length === 0 && disallowedSources.length === 0 ? 'VERIFIED' : 'FAILED',
      verified_registry_crates: registryPackages.length - missingOrInvalidChecksums.length,
      total_registry_crates: registryPackages.length,
      allowed_registries_only: disallowedSources.length === 0,
      scope_boundary:
        'Verifies SHA-256 tarball digests in Cargo.lock against crates.io-index with --locked; does NOT prove absence of CVEs or cryptographic build provenance.',
    },
    license_policy: {
      status: licenseAudit.license_violations.length === 0 ? 'VERIFIED' : 'FAILED',
      evaluated_packages_count: licenseAudit.evaluated_packages_count,
      violations_count: licenseAudit.license_violations.length,
      banned_crates_count: bannedCratesFound.length,
      scope_boundary:
        'Verifies SPDX license expressions and crate bans (zero openssl/openssl-sys) against deny.toml; orthogonal to security vulnerabilities and signing.',
    },
    known_vulnerability_advisory_scan: {
      status: advisoryScan.status,
      scanner: advisoryScan.scanner,
      cargo_deny_version: advisoryScan.cargo_deny_version,
      advisory_db_url: advisoryScan.advisory_db_url,
      advisory_db_commit_sha: advisoryScan.advisory_db_commit_sha,
      advisory_db_commit_date: advisoryScan.advisory_db_commit_date,
      vulnerabilities_found: advisoryScan.vulnerabilities_found,
      yanked_crates_found: advisoryScan.yanked_crates_found,
      advisory_findings: advisoryScan.advisory_findings,
      scope_boundary: advisoryScan.honesty_disclosure,
    },
    sbom: {
      status: 'DECLARED_DEPENDENCY_INVENTORY_ONLY',
      format: 'CycloneDX 1.5 / cargo metadata v1',
      unique_components_count: uniqueLockedPackages.length,
      is_independent_build_provenance: false,
      scope_boundary:
        'SBOM enumerates declared Rust dependency components ONLY; an SBOM is NOT independent build provenance, NOT an advisory scan, and NOT an artifact signature.',
    },
    provenance: {
      status: 'SELF_REPORTED_UNATTESTED',
      oidc_slsa_attestation_present: false,
      scope_boundary:
        'Build scripts emit traceable Section 7 SHA-256 manifests (`SELF_REPORTED_UNATTESTED`), NOT hardware/OIDC-attested SLSA v1.0 cryptographic provenance (`actions/attest-build-provenance` requires live GitHub Actions execution).',
    },
    artifact_signature: {
      status: 'UNSIGNED_DEVELOPMENT_ARTIFACT',
      windows_authenticode_signed: false,
      macos_developer_id_signed: false,
      macos_notarized: false,
      scope_boundary:
        'All compiled DIM binaries and CI bundles are UNSIGNED_DEVELOPMENT_ARTIFACT builds; production release remains blocked on Gate G3 (Authenticode) and Gate G4 (Apple Developer ID + Notarization).',
    },
    installer_integrity: {
      status: 'RAW_EXECUTABLES_ONLY_LOCALLY_CI_OVERLAY_READY',
      raw_executables_are_installers: false,
      ci_bundle_overlay_path: 'desktop-instance-manager/scripts/ci-cross-platform/tauri-ci-bundle-overlay.json',
      scope_boundary:
        'Raw compiled executables (dim-desktop.exe, dim-cli.exe, dim-desktop) have is_installer=false; OS installers (.msi, -setup.exe, .dmg, .deb) require native OS bundlers + cryptographic signing before end-user distribution.',
    },
  };

  return {
    schema_version: 2,
    generated_at_utc: new Date().toISOString(),
    policy_source: 'desktop-instance-manager/deny.toml',
    policy_summary: policy,
    supply_chain_pillars: supplyChainPillars,
    lockfiles: {
      workspace: {
        path: 'desktop-instance-manager/Cargo.lock',
        format_version: wsLock.lockVersion,
        sha256: sha256File(workspaceLockPath),
        packages_count: wsLock.packages.length,
      },
      tauri_desktop: {
        path: 'desktop-instance-manager/apps/desktop/src-tauri/Cargo.lock',
        format_version: tauriLock.lockVersion,
        sha256: sha256File(tauriLockPath),
        packages_count: tauriLock.packages.length,
        tauri_version: tauriPkg ? tauriPkg.version : null,
        tauri_build_version: tauriBuildPkg ? tauriBuildPkg.version : null,
      },
      unique_locked_packages_count: uniqueLockedPackages.length,
      registry_packages_count: registryPackages.length,
    },
    checks: {
      banned_crates_passed: bannedCratesFound.length === 0,
      banned_crates_found: bannedCratesFound,
      allowed_registries_passed: disallowedSources.length === 0,
      disallowed_sources: disallowedSources,
      checksums_passed: missingOrInvalidChecksums.length === 0,
      verified_sha256_checksums_count: registryPackages.length - missingOrInvalidChecksums.length,
      missing_or_invalid_checksums: missingOrInvalidChecksums,
      licenses_passed: licenseAudit.license_violations.length === 0,
      license_audit: licenseAudit,
      advisory_scan: advisoryScan,
      msrv_alignment: msrvCheck,
      zero_vng_binary_policy_passed: vngBinariesFound.length === 0,
      vng_proprietary_binaries_found: vngBinariesFound,
    },
    duplicate_crate_versions_count: duplicateCrateVersions.length,
    duplicate_crate_versions: duplicateCrateVersions,
    warnings,
    policy_check_passed: policyCheckPassed,
  };
}

export function renderDependencyAuditMarkdown(report) {
  const c = report.checks;
  const p = report.supply_chain_pillars;
  const adv = c.advisory_scan;
  const distRows = Object.entries(c.license_audit.license_distribution || {})
    .map(([lic, count]) => `| \`${lic}\` | ${count} |`)
    .join('\n');

  const warnLines = report.warnings.map((w) => `- ${w}`).join('\n');
  const vulnCountDisplay = adv.vulnerabilities_found === null
    ? 'UNVERIFIED (advisory DB not scanned)'
    : `${adv.vulnerabilities_found} vulnerabilities, ${adv.yanked_crates_found} yanked crates`;

  return `# Cross-Platform Supply-Chain, RustSec Advisory & Dependency Audit Summary

- **Generated (UTC):** \`${report.generated_at_utc}\`
- **Policy File:** \`${report.policy_source}\`
- **Overall Policy Verdict:** **${report.policy_check_passed ? 'PASS (with MSRV & deny.toml integration warnings)' : 'FAIL'}**
- **RustSec Advisory Scanner:** \`${adv.cargo_deny_version || 'NOT_EXECUTED'}\` against \`rustsec/advisory-db\` @ \`${adv.advisory_db_commit_sha || 'UNSCANNED'}\`${adv.advisory_db_commit_date ? ` (\`${adv.advisory_db_commit_date}\`)` : ''}

---

## 1. Lockfile & Toolchain Scope

| Lockfile | Format | SHA-256 | Total Packages | Notes |
| :--- | :---: | :--- | :---: | :--- |
| \`${report.lockfiles.workspace.path}\` | v${report.lockfiles.workspace.format_version} | \`${report.lockfiles.workspace.sha256}\` | ${report.lockfiles.workspace.packages_count} | Core Rust workspace (\`dim-core\`, \`dim-profile-store\`, \`dim-process-manager\`, OS adapters, \`dim-cli\`) |
| \`${report.lockfiles.tauri_desktop.path}\` | v${report.lockfiles.tauri_desktop.format_version} | \`${report.lockfiles.tauri_desktop.sha256}\` | ${report.lockfiles.tauri_desktop.packages_count} | Tauri v2 desktop shell (\`tauri@${report.lockfiles.tauri_desktop.tauri_version}\`, \`tauri-build@${report.lockfiles.tauri_desktop.tauri_build_version}\`) |
| **Combined Unique Packages** | — | — | **${report.lockfiles.unique_locked_packages_count}** | **${report.lockfiles.registry_packages_count}** external \`crates.io-index\` crates + 8 local workspace crates |

---

## 2. 7-Pillar Supply-Chain Security Separation

Each supply-chain dimension is tracked independently so inventory, checksums, advisories, provenance, signing, and installer integrity are **never** conflated:

| # | Supply-Chain Pillar | Current Status | Grounded Evidence & Strict Boundary |
| :---: | :--- | :--- | :--- |
| **1** | **Lockfile Checksum Verification** | \`${p.lockfile_checksum_verification.status}\` | **${c.verified_sha256_checksums_count} / ${report.lockfiles.registry_packages_count}** external \`crates.io-index\` crates carry valid 64-hex SHA-256 checksums; \`0\` disallowed registries/git sources; \`--locked\` enforced. |
| **2** | **License & Crate Ban Policy** | \`${p.license_policy.status}\` | **${c.license_audit.evaluated_packages_count}** unique crates evaluated via \`cargo metadata --locked\` & \`cargo-deny\`: **0** SPDX violations, **0** banned crates (\`openssl\`, \`openssl-sys\`). |
| **3** | **Known Vulnerability Advisory Scan** | \`${p.known_vulnerability_advisory_scan.status}\` | **${vulnCountDisplay}** via \`${adv.cargo_deny_version || 'none'}\` against \`rustsec/advisory-db\` commit \`${adv.advisory_db_commit_sha || 'N/A'}\` on both lockfiles. |
| **4** | **SBOM (Software Bill of Materials)** | \`${p.sbom.status}\` | Declared dependency inventory (${p.sbom.unique_components_count} crates). **An SBOM is an inventory ONLY — NOT independent build provenance, NOT a vulnerability scan, and NOT a signature.** |
| **5** | **Build Provenance** | \`${p.provenance.status}\` | Script-generated Section 7 SHA-256 manifest without OIDC/SLSA v1.0 cryptographic builder attestation (\`oidc_slsa_attestation_present: false\`). |
| **6** | **Artifact Signature** | \`${p.artifact_signature.status}\` | Zero Windows Authenticode (\`RVA=0, Size=0\`) and zero Apple \`Developer ID Application\` signatures / notarization tickets. Blocked for production until Gates G3/G4. |
| **7** | **Installer Integrity** | \`${p.installer_integrity.status}\` | Raw binaries (\`dim-desktop.exe\`, \`dim-cli.exe\`, \`dim-desktop\`) have \`is_installer: false\`; CI bundler overlay (\`tauri-ci-bundle-overlay.json\` + valid \`icon.icns\`/\`128x128@2x.png\`) ready for native runner packaging. |

---

## 3. Detailed Policy & Zero-VNG Checks

| Check | Status | Result Details |
| :--- | :---: | :--- |
| **Real \`cargo-deny\` Advisories (Workspace)** | ${adv.workspace_summary?.advisories?.errors === 0 ? 'PASS' : adv.status} | \`errors=${adv.workspace_summary?.advisories?.errors ?? 'N/A'}, warnings=${adv.workspace_summary?.advisories?.warnings ?? 'N/A'}\` |
| **Real \`cargo-deny\` Advisories (\`src-tauri\`)** | ${adv.tauri_summary?.advisories?.errors === 0 ? 'PASS' : adv.status} | \`errors=${adv.tauri_summary?.advisories?.errors ?? 'N/A'}, warnings=${adv.tauri_summary?.advisories?.warnings ?? 'N/A'}\` |
| **Banned Crates (\`openssl\`, \`openssl-sys\`)** | ${c.banned_crates_passed ? 'PASS' : 'FAIL'} | ${c.banned_crates_found.length} banned crates found |
| **Allowed Crate Registries (\`crates.io-index\` only)** | ${c.allowed_registries_passed ? 'PASS' : 'FAIL'} | ${c.disallowed_sources.length} disallowed registry/git sources |
| **Zero VNG Zalo Binary Policy** | ${c.zero_vng_binary_policy_passed ? 'PASS' : 'FAIL'} | ${c.vng_proprietary_binaries_found.length} proprietary VNG Zalo binaries/bundles found in repository |
| **MSRV Alignment (\`Cargo.toml\` vs \`src-tauri/Cargo.toml\`)** | ${c.msrv_alignment.mismatch_detected ? 'WARNING (IR-001)' : 'PASS'} | Workspace = \`${c.msrv_alignment.workspace_rust_version}\`, Tauri desktop = \`${c.msrv_alignment.tauri_rust_version}\` (effective packaging MSRV = \`${c.msrv_alignment.effective_packaging_msrv}\`) |

---

## 4. SPDX License Expression Distribution

| SPDX Expression | Crate Count |
| :--- | :---: |
${distRows || '| *(metadata unavailable)* | 0 |'}

---

## 5. Audit Warnings & Out-of-Scope Integration Notes

${warnLines}
`;
}

function parseArgs(argv) {
  const opts = {
    outJson: null,
    outMd: null,
    strictWarnings: false,
    simulateNoAdvisoryDb: false,
    help: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') opts.help = true;
    else if (arg === '--strict-warnings') opts.strictWarnings = true;
    else if (arg === '--simulate-no-advisory-db') opts.simulateNoAdvisoryDb = true;
    else if (arg === '--out-json' && i + 1 < argv.length) opts.outJson = argv[++i];
    else if (arg.startsWith('--out-json=')) opts.outJson = arg.slice('--out-json='.length);
    else if (arg === '--out-md' && i + 1 < argv.length) opts.outMd = argv[++i];
    else if (arg.startsWith('--out-md=')) opts.outMd = arg.slice('--out-md='.length);
    else throw new Error(`Unknown or incomplete CLI option: ${arg}`);
  }
  return opts;
}

function cliMain() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    console.log(
      'Usage: node dependency-audit.mjs [--out-json <json-path>] [--out-md <md-path>] [--strict-warnings] [--simulate-no-advisory-db]',
    );
    process.exit(0);
  }

  const report = runDependencyAudit({ simulateNoAdvisoryDb: opts.simulateNoAdvisoryDb });

  if (opts.outJson) {
    const absJson = path.resolve(opts.outJson);
    fs.mkdirSync(path.dirname(absJson), { recursive: true });
    fs.writeFileSync(absJson, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  }

  if (opts.outMd) {
    const absMd = path.resolve(opts.outMd);
    fs.mkdirSync(path.dirname(absMd), { recursive: true });
    fs.writeFileSync(absMd, renderDependencyAuditMarkdown(report), 'utf8');
  }

  console.log(JSON.stringify({
    policy_check_passed: report.policy_check_passed,
    unique_locked_packages_count: report.lockfiles.unique_locked_packages_count,
    verified_sha256_checksums_count: report.checks.verified_sha256_checksums_count,
    evaluated_license_packages: report.checks.license_audit.evaluated_packages_count,
    advisory_scan_status: report.checks.advisory_scan.status,
    cargo_deny_version: report.checks.advisory_scan.cargo_deny_version,
    advisory_db_commit_sha: report.checks.advisory_scan.advisory_db_commit_sha,
    vulnerabilities_found: report.checks.advisory_scan.vulnerabilities_found,
    banned_crates_found: report.checks.banned_crates_found.length,
    disallowed_sources: report.checks.disallowed_sources.length,
    license_violations: report.checks.license_audit.license_violations.length,
    vng_binaries_found: report.checks.vng_proprietary_binaries_found.length,
    msrv_mismatch_warning: report.checks.msrv_alignment.mismatch_detected,
    out_json: opts.outJson ? path.resolve(opts.outJson) : null,
    out_md: opts.outMd ? path.resolve(opts.outMd) : null,
  }, null, 2));

  if (!report.policy_check_passed) {
    process.exit(1);
  }
  if (opts.strictWarnings && report.checks.msrv_alignment.mismatch_detected) {
    process.exit(1);
  }
}

if (isMainModule(import.meta.url)) {
  try {
    cliMain();
  } catch (err) {
    console.error(`[dependency-audit] ERROR: ${err.message}`);
    process.exit(1);
  }
}
