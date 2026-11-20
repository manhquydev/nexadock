#!/usr/bin/env node
/**
 * Generates reproducible build provenance artifacts for desktop-instance-manager:
 *   - evidence/artifact-manifest.json (SHA-256 + byte sizes + toolchain/commit metadata)
 *   - evidence/sbom-cyclonedx.json (CycloneDX 1.5 JSON SBOM from `cargo metadata`)
 *   - evidence/dependency-audit.json (Cargo.lock policy check against `deny.toml` rules)
 */

import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const WORKSPACE_ROOT = path.resolve(__dirname, '..');

function runCmd(bin, args, cwd = WORKSPACE_ROOT) {
  return execFileSync(bin, args, { cwd, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 }).trim();
}

function sha256File(filePath) {
  const hash = crypto.createHash('sha256');
  hash.update(fs.readFileSync(filePath));
  return hash.digest('hex');
}

function collectArtifacts(targetDir) {
  const ext = process.platform === 'win32' ? '.exe' : '';
  const candidates = [
    path.join(WORKSPACE_ROOT, 'target', 'release', `dim-cli${ext}`),
    path.join(WORKSPACE_ROOT, 'target', 'release', `dim-test-fixture${ext}`),
    path.join(WORKSPACE_ROOT, 'target', 'debug', `dim-cli${ext}`),
    path.join(WORKSPACE_ROOT, 'target', 'debug', `dim-test-fixture${ext}`),
    path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'target', 'release', `dim-desktop${ext}`),
    path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'target', 'debug', `dim-desktop${ext}`),
  ];
  const artifacts = [];
  for (const abs of candidates) {
    if (fs.existsSync(abs) && fs.statSync(abs).isFile()) {
      const stat = fs.statSync(abs);
      artifacts.push({
        relative_path: path.relative(WORKSPACE_ROOT, abs).replace(/\\/g, '/'),
        size_bytes: stat.size,
        sha256: sha256File(abs),
      });
    }
  }
  return artifacts;
}

function parseCargoLockPackages(lockPath) {
  const text = fs.readFileSync(lockPath, 'utf8');
  const blocks = text.split(/^\[\[package\]\]/m).slice(1);
  return blocks.map((blk) => {
    const getField = (name) => {
      const m = blk.match(new RegExp(`^${name}\\s*=\\s*"([^"]+)"`, 'm'));
      return m ? m[1] : null;
    };
    return {
      name: getField('name'),
      version: getField('version'),
      source: getField('source'),
      checksum: getField('checksum'),
    };
  }).filter((p) => p.name && p.version);
}

function runCargoMetadata(cargoBin, manifestRel) {
  const args = ['metadata', '--format-version', '1', '--locked'];
  if (manifestRel) {
    args.push('--manifest-path', manifestRel);
  }
  return JSON.parse(runCmd(cargoBin, args));
}

function isLicenseExpressionSatisfied(expr, allowed) {
  if (!expr || typeof expr !== 'string') return false;
  const trimmed = expr.trim();
  if (!trimmed) return false;
  if (allowed.has(trimmed)) return true;

  // Strip outer parentheses and normalize slash shorthand into SPDX OR
  const normalized = trimmed.replace(/\s*\/\s*/g, ' OR ');
  const evalAnd = (part) => {
    const andTerms = [];
    let depth = 0;
    let buf = '';
    const tokens = normalized_split(part, 'AND');
    for (const term of tokens) {
      let t = term.trim();
      while (t.startsWith('(') && t.endsWith(')')) {
        t = t.slice(1, -1).trim();
      }
      if (!isLicenseExpressionSatisfied(t, allowed)) return false;
    }
    return tokens.length > 0;
  };
  function normalized_split(str, op) {
    const parts = [];
    let depth = 0;
    let start = 0;
    const target = ` ${op} `;
    for (let i = 0; i < str.length; i += 1) {
      if (str[i] === '(') depth += 1;
      else if (str[i] === ')') depth = Math.max(0, depth - 1);
      else if (depth === 0 && str.slice(i, i + target.length) === target) {
        parts.push(str.slice(start, i));
        i += target.length - 1;
        start = i + 1;
      }
    }
    parts.push(str.slice(start));
    return parts;
  }

  let stripped = normalized;
  while (stripped.startsWith('(') && stripped.endsWith(')')) {
    let depth = 0;
    let wrapsAll = true;
    for (let i = 0; i < stripped.length; i += 1) {
      if (stripped[i] === '(') depth += 1;
      else if (stripped[i] === ')') depth -= 1;
      if (depth === 0 && i < stripped.length - 1) {
        wrapsAll = false;
        break;
      }
    }
    if (!wrapsAll) break;
    stripped = stripped.slice(1, -1).trim();
  }
  if (allowed.has(stripped)) return true;

  const orParts = normalized_split(stripped, 'OR');
  if (orParts.length > 1) {
    return orParts.some((p) => isLicenseExpressionSatisfied(p, allowed));
  }
  const andParts = normalized_split(stripped, 'AND');
  if (andParts.length > 1) {
    return andParts.every((p) => isLicenseExpressionSatisfied(p, allowed));
  }
  return allowed.has(stripped);
}

function main() {
  const cargoBin = process.env.CARGO || 'cargo';
  const rustcBin = process.env.RUSTC || 'rustc';

  const rustcVersion = runCmd(rustcBin, ['--version']);
  const cargoVersion = runCmd(cargoBin, ['--version']);
  const gitHead = runCmd('git', ['rev-parse', 'HEAD'], path.resolve(WORKSPACE_ROOT, '..'));
  const metadata = runCargoMetadata(cargoBin, null);
  const tauriMetadata = runCargoMetadata(cargoBin, 'apps/desktop/src-tauri/Cargo.toml');

  const artifacts = collectArtifacts();
  const lockPath = path.join(WORKSPACE_ROOT, 'Cargo.lock');
  const tauriLockPath = path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'Cargo.lock');
  const lockedPkgs = fs.existsSync(lockPath) ? parseCargoLockPackages(lockPath) : [];
  const tauriLockedPkgs = fs.existsSync(tauriLockPath) ? parseCargoLockPackages(tauriLockPath) : [];
  const allLockedPkgs = [...lockedPkgs, ...tauriLockedPkgs];

  const artifactManifest = {
    schema_version: 1,
    generated_at_utc: new Date().toISOString(),
    git_head: gitHead,
    platform: `${process.platform}-${process.arch}`,
    rustc_version: rustcVersion,
    cargo_version: cargoVersion,
    cargo_lock_sha256: fs.existsSync(lockPath) ? sha256File(lockPath) : null,
    tauri_lock_sha256: fs.existsSync(tauriLockPath) ? sha256File(tauriLockPath) : null,
    signing_status: 'UNSIGNED_LOCAL_OR_CI_TEST_BUILD (Gate F requires Authenticode cert before public distribution)',
    artifacts,
  };

  const uniquePackagesMap = new Map();
  for (const pkg of [...metadata.packages, ...tauriMetadata.packages]) {
    const key = `${pkg.name}@${pkg.version}`;
    if (!uniquePackagesMap.has(key)) {
      uniquePackagesMap.set(key, pkg);
    }
  }
  const allPackages = Array.from(uniquePackagesMap.values());

  const components = allPackages.map((pkg) => ({
    type: 'library',
    name: pkg.name,
    version: pkg.version,
    licenses: pkg.license ? [{ expression: pkg.license }] : [],
    purl: pkg.source
      ? `pkg:cargo/${pkg.name}@${pkg.version}`
      : `pkg:generic/${pkg.name}@${pkg.version}`,
    scope: 'required',
  }));

  const sbom = {
    bomFormat: 'CycloneDX',
    specVersion: '1.5',
    serialNumber: `urn:uuid:${crypto.randomUUID()}`,
    version: 1,
    metadata: {
      timestamp: new Date().toISOString(),
      tools: [{ vendor: 'NexaDock', name: 'generate-build-provenance.mjs', version: '1.1.0' }],
      component: {
        type: 'application',
        name: 'desktop-instance-manager',
        version: '0.1.0',
      },
    },
    components,
  };

  const allowedLicenses = new Set([
    'MIT',
    'Apache-2.0',
    'Apache-2.0 WITH LLVM-exception',
    'BSD-2-Clause',
    'BSD-3-Clause',
    'ISC',
    'Unicode-3.0',
    'Zlib',
    'MPL-2.0',
  ]);
  const bannedCrates = new Set(['openssl', 'openssl-sys']);
  const disallowedSource = allLockedPkgs.filter(
    (p) => p.source && p.source !== 'registry+https://github.com/rust-lang/crates.io-index'
  );
  const bannedFound = allPackages.filter((p) => bannedCrates.has(p.name)).map((p) => p.name);
  const unknownLicenses = allPackages
    .filter((p) => p.source && (!p.license || !isLicenseExpressionSatisfied(p.license, allowedLicenses)))
    .map((p) => ({ name: p.name, version: p.version, license: p.license || null }));

  const depAudit = {
    generated_at_utc: new Date().toISOString(),
    cargo_lock_packages_count: lockedPkgs.length,
    tauri_lock_packages_count: tauriLockedPkgs.length,
    unique_resolved_packages_count: allPackages.length,
    banned_crates_found: bannedFound,
    disallowed_sources: disallowedSource,
    license_review_items: unknownLicenses,
    policy_check_passed:
      bannedFound.length === 0 &&
      disallowedSource.length === 0 &&
      unknownLicenses.length === 0,
  };

  const evDir = path.join(WORKSPACE_ROOT, 'evidence');
  fs.mkdirSync(evDir, { recursive: true });
  fs.writeFileSync(path.join(evDir, 'artifact-manifest.json'), `${JSON.stringify(artifactManifest, null, 2)}\n`, 'utf8');
  fs.writeFileSync(path.join(evDir, 'sbom-cyclonedx.json'), `${JSON.stringify(sbom, null, 2)}\n`, 'utf8');
  fs.writeFileSync(path.join(evDir, 'dependency-audit.json'), `${JSON.stringify(depAudit, null, 2)}\n`, 'utf8');

  console.log(JSON.stringify({
    ok: true,
    artifacts_count: artifacts.length,
    sbom_components: components.length,
    dep_audit_passed: depAudit.policy_check_passed,
  }, null, 2));
}

main();
