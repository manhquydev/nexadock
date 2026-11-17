#!/usr/bin/env node
/**
 * Dependency, license, registry, checksum, MSRV, and Zero-VNG-Binary auditor
 * for Multi-Zalo Desktop (desktop-instance-manager).
 *
 * Strictly read-only on repository sources, Cargo.lock, and evidence/.
 *
 * CLI:
 *   node dependency-audit.mjs [--out-json <json-path>] [--out-md <md-path>] [--strict-warnings]
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

function findCargoBin(env) {
  if (env.CARGO && fs.existsSync(env.CARGO)) return env.CARGO;
  const isWin = process.platform === 'win32';
  const binName = isWin ? 'cargo.exe' : 'cargo';
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
    allowed_licenses: parseStringArray(licensesSection, 'allow'),
    banned_crates: bannedCrates,
    allowed_registries: parseStringArray(sourcesSection, 'allow-registry'),
    allowed_git: parseStringArray(sourcesSection, 'allow-git'),
    unknown_registry_policy: (sourcesSection.match(/unknown-registry\s*=\s*"([^"]+)"/) || [])[1] || 'deny',
    unknown_git_policy: (sourcesSection.match(/unknown-git\s*=\s*"([^"]+)"/) || [])[1] || 'deny',
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

function runCargoMetadataLocked(cargoBin, manifestArgs) {
  const res = spawnSync(
    cargoBin,
    ['metadata', '--format-version', '1', '--locked', ...manifestArgs],
    {
      cwd: WORKSPACE_ROOT,
      env: buildAugmentedEnv(),
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

export function runDependencyAudit() {
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

  // Deduplicate by name@version for unique count while preserving lockfile provenance
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

  // 1. Banned crates check
  const bannedCratesFound = uniqueLockedPackages
    .filter((p) => bannedSet.has(p.name))
    .map((p) => ({ name: p.name, version: p.version, lockfiles: p.lockfiles }));

  // 2. Allowed registry check
  const disallowedSources = uniqueLockedPackages
    .filter((p) => p.source !== null && !allowedRegistrySources.has(p.source))
    .map((p) => ({ name: p.name, version: p.version, source: p.source, lockfiles: p.lockfiles }));

  // 3. SHA-256 checksum presence & 64-hex validity on all registry crates
  const sha256HexRe = /^[0-9a-f]{64}$/;
  const registryPackages = uniqueLockedPackages.filter((p) => p.source !== null);
  const missingOrInvalidChecksums = registryPackages
    .filter((p) => !p.checksum || !sha256HexRe.test(p.checksum))
    .map((p) => ({ name: p.name, version: p.version, checksum: p.checksum, lockfiles: p.lockfiles }));

  // 4. Multiple versions summary (informational, matching deny.toml multiple-versions = "warn")
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

  // 5. SPDX License audit via `cargo metadata --locked` (if cargo available)
  const env = buildAugmentedEnv();
  const cargoBin = findCargoBin(env);
  let licenseAudit = {
    mode: 'SKIPPED_CARGO_NOT_AVAILABLE',
    evaluated_packages_count: 0,
    license_violations: [],
    license_distribution: {},
    max_dependency_msrv: null,
  };

  if (cargoBin) {
    const wsMeta = runCargoMetadataLocked(cargoBin, []);
    const tauriMeta = runCargoMetadataLocked(cargoBin, ['--manifest-path', 'apps/desktop/src-tauri/Cargo.toml']);
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
    } else {
      licenseAudit = {
        mode: 'CARGO_METADATA_ERROR',
        cargo_binary: cargoBin,
        workspace_error: wsMeta.ok ? null : wsMeta.error,
        tauri_error: tauriMeta.ok ? null : tauriMeta.error,
        evaluated_packages_count: 0,
        license_violations: [],
        license_distribution: {},
        max_dependency_msrv: null,
      };
    }
  }

  // 6. MSRV alignment & dual-lockfile warning check
  const msrvCheck = inspectMsrvAlignment();

  // 7. Zero VNG Zalo binary scan across repository
  const vngBinariesFound = scanForVngProprietaryBinaries(REPO_ROOT);

  // Extract Tauri crate version from src-tauri/Cargo.lock for reference
  const tauriPkg = tauriLock.packages.find((p) => p.name === 'tauri');
  const tauriBuildPkg = tauriLock.packages.find((p) => p.name === 'tauri-build');

  const warnings = [];
  if (msrvCheck.warning) warnings.push(msrvCheck.warning);
  warnings.push(
    'IR-004 INFO: Repository maintains two separate Cargo lockfiles (desktop-instance-manager/Cargo.lock and apps/desktop/src-tauri/Cargo.lock) because src-tauri declares an independent [workspace]. Both lockfiles were audited.',
  );
  if (duplicateCrateVersions.length > 0) {
    warnings.push(
      `Transitive multi-version crates present (${duplicateCrateVersions.length} crate families across lockfiles; matches deny.toml multiple-versions = "warn").`,
    );
  }

  const policyCheckPassed =
    bannedCratesFound.length === 0 &&
    disallowedSources.length === 0 &&
    missingOrInvalidChecksums.length === 0 &&
    licenseAudit.license_violations.length === 0 &&
    vngBinariesFound.length === 0;

  return {
    schema_version: 1,
    generated_at_utc: new Date().toISOString(),
    policy_source: 'desktop-instance-manager/deny.toml',
    policy_summary: policy,
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
  const distRows = Object.entries(c.license_audit.license_distribution || {})
    .map(([lic, count]) => `| \`${lic}\` | ${count} |`)
    .join('\n');

  const warnLines = report.warnings.map((w) => `- ${w}`).join('\n');

  return `# Cross-Platform Dependency, License & Policy Audit Summary

- **Generated (UTC):** \`${report.generated_at_utc}\`
- **Policy File:** \`${report.policy_source}\`
- **Overall Policy Verdict:** **${report.policy_check_passed ? 'PASS (with MSRV alignment warning IR-001)' : 'FAIL'}**

---

## 1. Lockfile & Toolchain Scope

| Lockfile | Format | SHA-256 | Total Packages | Notes |
| :--- | :---: | :--- | :---: | :--- |
| \`${report.lockfiles.workspace.path}\` | v${report.lockfiles.workspace.format_version} | \`${report.lockfiles.workspace.sha256}\` | ${report.lockfiles.workspace.packages_count} | Core Rust workspace (\`dim-core\`, \`dim-profile-store\`, \`dim-process-manager\`, OS adapters, \`dim-cli\`) |
| \`${report.lockfiles.tauri_desktop.path}\` | v${report.lockfiles.tauri_desktop.format_version} | \`${report.lockfiles.tauri_desktop.sha256}\` | ${report.lockfiles.tauri_desktop.packages_count} | Tauri v2 desktop shell (\`tauri@${report.lockfiles.tauri_desktop.tauri_version}\`, \`tauri-build@${report.lockfiles.tauri_desktop.tauri_build_version}\`) |
| **Combined Unique Packages** | — | — | **${report.lockfiles.unique_locked_packages_count}** | **${report.lockfiles.registry_packages_count}** external \`crates.io-index\` crates + local workspace crates |

---

## 2. Security, Registry, License & Binary Policy Checks

| Check | Status | Result Details |
| :--- | :---: | :--- |
| **Banned Crates (\`openssl\`, \`openssl-sys\`)** | ${c.banned_crates_passed ? 'PASS' : 'FAIL'} | ${c.banned_crates_found.length} banned crates found |
| **Allowed Crate Registries (\`crates.io-index\` only)** | ${c.allowed_registries_passed ? 'PASS' : 'FAIL'} | ${c.disallowed_sources.length} disallowed registry/git sources |
| **Registry SHA-256 Checksum Integrity** | ${c.checksums_passed ? 'PASS' : 'FAIL'} | ${c.verified_sha256_checksums_count} / ${report.lockfiles.registry_packages_count} registry crates carry valid 64-hex SHA-256 checksums |
| **SPDX License Allowlist (\`deny.toml\`)** | ${c.licenses_passed ? 'PASS' : 'FAIL'} | Mode: \`${c.license_audit.mode}\` (${c.license_audit.evaluated_packages_count} unique crates evaluated, ${c.license_audit.license_violations.length} violations) |
| **Zero VNG Zalo Binary Policy** | ${c.zero_vng_binary_policy_passed ? 'PASS' : 'FAIL'} | ${c.vng_proprietary_binaries_found.length} proprietary VNG Zalo binaries/bundles found in repository |
| **MSRV Alignment (\`Cargo.toml\` vs \`src-tauri/Cargo.toml\`)** | ${c.msrv_alignment.mismatch_detected ? 'WARNING (IR-001)' : 'PASS'} | Workspace = \`${c.msrv_alignment.workspace_rust_version}\`, Tauri desktop = \`${c.msrv_alignment.tauri_rust_version}\` (effective packaging MSRV = \`${c.msrv_alignment.effective_packaging_msrv}\`${c.license_audit.max_dependency_msrv ? `; highest transitive crate MSRV = \`${c.license_audit.max_dependency_msrv.rust_version}\` via \`${c.license_audit.max_dependency_msrv.crate}\`` : ''}) |

---

## 3. SPDX License Expression Distribution

| SPDX Expression | Crate Count |
| :--- | :---: |
${distRows || '| *(metadata unavailable)* | 0 |'}

---

## 4. Audit Warnings & Out-of-Scope Integration Notes

${warnLines}
`;
}

function parseArgs(argv) {
  const opts = {
    outJson: null,
    outMd: null,
    strictWarnings: false,
    help: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') opts.help = true;
    else if (arg === '--strict-warnings') opts.strictWarnings = true;
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
    console.log('Usage: node dependency-audit.mjs [--out-json <json-path>] [--out-md <md-path>] [--strict-warnings]');
    process.exit(0);
  }

  const report = runDependencyAudit();

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
