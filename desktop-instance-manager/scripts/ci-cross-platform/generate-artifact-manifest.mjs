#!/usr/bin/env node
/**
 * Section 7 Traceable Build Artifact Manifest & SHA256SUMS Generator
 * for Multi-Zalo Desktop (desktop-instance-manager).
 *
 * Explicitly separates:
 *   - Provenance trust level (`SELF_REPORTED_UNATTESTED` vs CI OIDC SLSA attestation)
 *   - SBOM (declared dependency inventory, NOT independent build provenance)
 *   - Signing status (`UNSIGNED_DEVELOPMENT_ARTIFACT`, never `signed` or `production`)
 *   - Artifact classification (`compiled-executable` `is_installer: false` vs OS installers)
 *
 * CLI:
 *   node generate-artifact-manifest.mjs \
 *     [--artifacts-dir <dir-or-comma-paths>] [--scan-default-targets] [--target <triple>] \
 *     [--build-summary <json>] [--os-label <str>] [--arch <str>] [--test-status <str>] \
 *     [--provenance-trust-level <str>] [--build-log-url <url>] [--merge-manifests <dir>] \
 *     --out-manifest <manifest.json> [--out-sha256 <SHA256SUMS.txt>] [--out-md <summary.md>]
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { inspectArtifact } from './inspect-artifact.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const WORKSPACE_ROOT = path.resolve(__dirname, '..', '..');
const REPO_ROOT = path.resolve(WORKSPACE_ROOT, '..');

export const SECTION_7_REQUIRED_FIELDS = [
  'name',
  'artifact_kind',
  'is_installer',
  'os',
  'cpu_architecture',
  'rust_target_triple',
  'build_commit_sha',
  'rust_version',
  'tauri_version',
  'file_size_bytes',
  'sha256',
  'provenance_trust_level',
  'signing_status',
  'notarization_status',
  'test_status',
  'zalo_runtime_compat_status',
  'vng_binary_embedded',
  'build_log_url',
  'inspection',
];

const FORBIDDEN_VNG_ARTIFACT_RE =
  /^(zalo\.exe|zalo\.app|zalosetup.*\.exe|zalo.*\.(msi|dmg|pkg|deb|appimage)|zcall\.dll|zalocore\.dll|app\.asar)$/i;

const IGNORED_CARGO_DIRS = new Set(['deps', 'build', '.fingerprint', 'incremental', 'examples']);
const IGNORED_INTERMEDIATE_EXTS = new Set([
  '.d',
  '.pdb',
  '.lib',
  '.exp',
  '.rlib',
  '.rmeta',
  '.o',
  '.obj',
  '.a',
  '.so',
  '.dylib',
  '.dll',
  '.json',
  '.txt',
  '.md',
  '.lock',
  '.Cargo-lock',
]);

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

function queryCmdLine(bin, args, cwd = REPO_ROOT) {
  const env = buildAugmentedEnv();
  const isWin = process.platform === 'win32';
  let resolved = bin;
  const pathKey = Object.keys(env).find((k) => k.toLowerCase() === 'path') || 'PATH';
  for (const dir of (env[pathKey] || '').split(path.delimiter).filter(Boolean)) {
    const candidate = path.join(dir, isWin && !path.extname(bin) ? `${bin}.exe` : bin);
    if (fs.existsSync(candidate)) {
      resolved = candidate;
      break;
    }
  }
  const res = spawnSync(resolved, args, { cwd, env, encoding: 'utf8', windowsHide: true });
  if (!res.error && res.status === 0) {
    return (res.stdout || '').trim();
  }
  return null;
}

export function readTauriVersionFromLockfile() {
  const lockPath = path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'Cargo.lock');
  if (!fs.existsSync(lockPath)) return 'unknown';
  const text = fs.readFileSync(lockPath, 'utf8');
  for (const blk of text.split(/^\[\[package\]\]\s*$/m).slice(1)) {
    const nameMatch = blk.match(/^name\s*=\s*"([^"]+)"/m);
    if (nameMatch && nameMatch[1] === 'tauri') {
      const verMatch = blk.match(/^version\s*=\s*"([^"]+)"/m);
      if (verMatch) return verMatch[1];
    }
  }
  return 'unknown';
}

function resolveGitCommitSha() {
  if (process.env.GITHUB_SHA && /^[0-9a-f]{40}$/i.test(process.env.GITHUB_SHA)) {
    return process.env.GITHUB_SHA;
  }
  return queryCmdLine('git', ['rev-parse', 'HEAD'], REPO_ROOT) || 'unknown';
}

function resolveRustVersion(buildSummary) {
  if (buildSummary?.rustc_version) return buildSummary.rustc_version;
  return queryCmdLine('rustc', ['--version'], WORKSPACE_ROOT) || 'unknown';
}

function resolveBuildLogUrl(explicitUrl) {
  if (explicitUrl && explicitUrl.trim()) return explicitUrl.trim();
  if (process.env.GITHUB_SERVER_URL && process.env.GITHUB_REPOSITORY && process.env.GITHUB_RUN_ID) {
    return `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}`;
  }
  return 'LOCAL_WORKTREE_BUILD (not executed via remote GitHub Actions)';
}

function normalizeArchLabel(arch) {
  if (!arch) return null;
  const lower = arch.toLowerCase();
  if (lower === 'x64' || lower === 'amd64' || lower === 'x86_64') return 'x86_64';
  if (lower === 'arm64' || lower === 'aarch64') return 'aarch64';
  return arch;
}

function inferTargetTripleFromPath(filePath, explicitTarget) {
  if (explicitTarget) return explicitTarget;
  const norm = filePath.replace(/\\/g, '/');
  const m = norm.match(/\/target\/([a-z0-9_]+-[a-z0-9_]+-[a-z0-9_-]+)\/release\//i);
  if (m) return m[1];
  if (process.platform === 'win32') return 'x86_64-pc-windows-msvc';
  if (process.platform === 'darwin') return process.arch === 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin';
  return 'x86_64-unknown-linux-gnu';
}

function buildZaloRuntimeCompatStatus(osKind) {
  if (osKind === 'windows') {
    return 'DIM_MANAGER_BUILD_ONLY — Windows L1/L2 multi-instance runtime verified separately against user-installed official VNG Zalo; zero VNG binaries embedded in this artifact';
  }
  if (osKind === 'macos') {
    return 'UNVERIFIED_ZALO_MACOS_RUNTIME — Compiling/packaging dim-desktop on macOS proves manager shell build ONLY, NOT VNG Zalo.app multi-instance runtime isolation';
  }
  if (osKind === 'linux') {
    return 'UNSUPPORTED_OFFICIAL_ZALO_LINUX — Compiling/packaging dim-desktop on Ubuntu 24.04 proves manager shell build ONLY; VNG provides no official Linux Zalo client and community ports are unverified';
  }
  return 'DIM_MANAGER_BUILD_ONLY — Compile/package success does NOT prove Zalo client runtime compatibility';
}

function scanDirectoryForDeliverables(dirPath, collectedSet) {
  if (!fs.existsSync(dirPath)) return;
  const entries = fs.readdirSync(dirPath, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name));
  for (const entry of entries) {
    if (entry.name.startsWith('.')) continue;
    if (IGNORED_CARGO_DIRS.has(entry.name)) continue;

    const full = path.join(dirPath, entry.name);
    if (FORBIDDEN_VNG_ARTIFACT_RE.test(entry.name)) {
      throw new Error(`FATAL Zero-VNG-Binary Policy violation: forbidden artifact detected at ${full}`);
    }

    if (entry.isDirectory()) {
      if (entry.name.toLowerCase().endsWith('.app')) {
        collectedSet.add(path.resolve(full));
        continue;
      }
      scanDirectoryForDeliverables(full, collectedSet);
    } else if (entry.isFile()) {
      const ext = path.extname(entry.name).toLowerCase();
      if (IGNORED_INTERMEDIATE_EXTS.has(ext)) continue;

      if (['.exe', '.msi', '.dmg', '.deb', '.appimage'].includes(ext)) {
        collectedSet.add(path.resolve(full));
      } else if (ext === '') {
        const fd = fs.openSync(full, 'r');
        const head = Buffer.alloc(16);
        let readBytes = 0;
        try {
          readBytes = fs.readSync(fd, head, 0, 16, 0);
        } finally {
          fs.closeSync(fd);
        }
        if (readBytes >= 4) {
          const isElf = head[0] === 0x7f && head[1] === 0x45 && head[2] === 0x4c && head[3] === 0x46;
          const magicLE = head.readUInt32LE(0);
          const magicBE = head.readUInt32BE(0);
          const isMachO =
            magicLE === 0xfeedfacf ||
            magicBE === 0xfeedfacf ||
            (magicBE === 0xcafebabe && readBytes >= 8 && head.readUInt32BE(4) >= 1 && head.readUInt32BE(4) <= 16);
          if (isElf || isMachO) {
            collectedSet.add(path.resolve(full));
          }
        }
      }
    }
  }
}

function discoverDefaultTargetArtifacts(target) {
  const collected = new Set();
  const triples = target
    ? [target]
    : [
        'x86_64-pc-windows-msvc',
        'x86_64-pc-windows-gnu',
        'aarch64-apple-darwin',
        'x86_64-apple-darwin',
        'x86_64-unknown-linux-gnu',
        'aarch64-unknown-linux-gnu',
      ];

  for (const t of triples) {
    scanDirectoryForDeliverables(path.join(WORKSPACE_ROOT, 'target', t, 'release'), collected);
    scanDirectoryForDeliverables(
      path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'target', t, 'release'),
      collected,
    );
  }

  if (!target) {
    scanDirectoryForDeliverables(path.join(WORKSPACE_ROOT, 'target', 'release'), collected);
    scanDirectoryForDeliverables(
      path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'target', 'release'),
      collected,
    );
  }

  return Array.from(collected);
}

function discoverArtifactsFromPaths(artifactsDirArg) {
  const collected = new Set();
  const rawParts = fs.existsSync(artifactsDirArg)
    ? [artifactsDirArg]
    : artifactsDirArg.split(',').map((s) => s.trim()).filter(Boolean);

  for (const part of rawParts) {
    const abs = path.resolve(part);
    if (!fs.existsSync(abs)) {
      throw new Error(`Artifact path does not exist: ${abs}`);
    }
    const base = path.basename(abs);
    if (FORBIDDEN_VNG_ARTIFACT_RE.test(base)) {
      throw new Error(`FATAL Zero-VNG-Binary Policy violation: forbidden artifact detected at ${abs}`);
    }
    const st = fs.statSync(abs);
    if (st.isFile() || base.toLowerCase().endsWith('.app')) {
      collected.add(abs);
    } else if (st.isDirectory()) {
      scanDirectoryForDeliverables(abs, collected);
    }
  }
  return Array.from(collected);
}

export function validateManifestEntrySection7(entry) {
  for (const field of SECTION_7_REQUIRED_FIELDS) {
    if (entry[field] === undefined || entry[field] === null || entry[field] === '') {
      throw new Error(`Manifest entry "${entry.name || '<unnamed>'}" missing required Section 7 field "${field}"`);
    }
  }
  if (!/^[0-9a-f]{64}$/.test(String(entry.sha256))) {
    throw new Error(`Manifest entry "${entry.name}" has invalid SHA-256 digest: ${entry.sha256}`);
  }
  if (entry.artifact_kind === 'compiled-executable' && entry.is_installer !== false) {
    throw new Error(`Classification violation: compiled executable "${entry.name}" cannot have is_installer=true`);
  }
  if (
    typeof entry.signing_status !== 'string' ||
    entry.signing_status.toLowerCase() === 'signed' ||
    entry.signing_status.toLowerCase() === 'production'
  ) {
    throw new Error(`Signing honesty violation on "${entry.name}": invalid signing_status "${entry.signing_status}"`);
  }
  if (entry.vng_binary_embedded !== false) {
    throw new Error(`Zero-VNG-Binary violation on "${entry.name}": vng_binary_embedded must be false`);
  }
}

export function renderManifestMarkdown(manifest) {
  const rows = manifest.artifacts.map((a) => {
    const shortSha = `${a.sha256.slice(0, 16)}...`;
    const installerLabel = a.is_installer ? 'Yes (Installer)' : 'No (Raw Binary / Bundle)';
    return `| \`${a.name}\` | \`${a.artifact_kind}\` | ${installerLabel} | \`${a.os}\` (\`${a.cpu_architecture}\`) | \`${a.rust_target_triple}\` | ${a.file_size_bytes.toLocaleString('en-US')} | \`${shortSha}\` | \`${a.provenance_trust_level}\` | \`${a.signing_status}\` | \`${a.test_status}\` |`;
  }).join('\n');

  return `## Traceable Build Artifact Provenance Manifest

- **Generated (UTC):** \`${manifest.generated_at_utc}\`
- **Commit SHA:** \`${manifest.build_commit_sha}\`
- **Rust Toolchain:** \`${manifest.rust_version}\`
- **Tauri Crate Version:** \`${manifest.tauri_version}\`
- **Provenance Trust Level:** \`${manifest.provenance_trust_level}\` (script-generated manifest without independent OIDC/SLSA cryptographic attestation; SBOM != provenance)
- **Signing Posture:** \`UNSIGNED_DEVELOPMENT_ARTIFACT\` (NOT signed, NOT a production release)
- **Zero VNG Binary Policy:** Verified (\`vng_binary_embedded = false\` across all ${manifest.artifacts.length} artifact(s))

| Artifact | Kind | Installer? | OS (Arch) | Target Triple | Size (Bytes) | SHA-256 (Prefix) | Provenance Trust | Signing Status | Test Status |
| :--- | :--- | :--- | :--- | :--- | ---: | :--- | :--- | :--- | :--- |
${rows || '| *(none)* | — | — | — | — | 0 | — | — | — | — |'}

> **Honesty & Runtime Compatibility Notice:**
> - Raw executables (\`dim-desktop.exe\`, \`dim-cli.exe\`, \`dim-desktop\`) are **compiled executables**, never OS installers.
> - \`SELF_REPORTED_UNATTESTED\` manifests record build metadata & SHA-256 digests without OIDC SLSA v1.0 attestation; SBOMs are dependency inventories, not build provenance.
> - Compiling or packaging \`dim-desktop\` on macOS or Ubuntu 24.04 proves manager build integrity only — it is **not** proof of macOS Zalo multi-instance runtime isolation or official Linux Zalo availability.
`;
}

export function generateManifest(opts) {
  const buildSummary = opts.buildSummary && fs.existsSync(path.resolve(opts.buildSummary))
    ? JSON.parse(fs.readFileSync(path.resolve(opts.buildSummary), 'utf8'))
    : null;

  const commitSha = resolveGitCommitSha();
  const rustVersion = resolveRustVersion(buildSummary);
  const tauriVersion = readTauriVersionFromLockfile();
  const buildLogUrl = resolveBuildLogUrl(opts.buildLogUrl);
  const testStatus = opts.testStatus || buildSummary?.test_status || 'VERIFIED_IN_LOCAL_OR_CI_STAGE';
  const provenanceTrustLevel = opts.provenanceTrustLevel || buildSummary?.provenance_trust_level || 'SELF_REPORTED_UNATTESTED';

  const discoveredSet = new Set();
  if (opts.scanDefaultTargets) {
    for (const p of discoverDefaultTargetArtifacts(opts.target)) discoveredSet.add(p);
  }
  if (opts.artifactsDir) {
    for (const p of discoverArtifactsFromPaths(opts.artifactsDir)) discoveredSet.add(p);
  }

  const artifactPaths = Array.from(discoveredSet).sort((a, b) => a.localeCompare(b));
  if (artifactPaths.length === 0 && !opts.allowEmpty) {
    throw new Error(
      'Zero build artifacts discovered. Pass --artifacts-dir <paths> or --scan-default-targets after building release binaries.',
    );
  }

  const artifacts = artifactPaths.map((absPath) => {
    const baseName = path.basename(absPath);
    if (FORBIDDEN_VNG_ARTIFACT_RE.test(baseName)) {
      throw new Error(`FATAL Zero-VNG-Binary Policy violation: ${absPath}`);
    }

    const inspection = inspectArtifact(absPath);
    const relFromWorkspace = path.relative(WORKSPACE_ROOT, absPath).replace(/\\/g, '/');
    const relativePath = relFromWorkspace.startsWith('..') ? baseName : relFromWorkspace;

    const targetTriple = inferTargetTripleFromPath(absPath, opts.target);
    const normalizedArch = inspection.cpu_architecture && inspection.cpu_architecture !== 'unknown'
      ? inspection.cpu_architecture
      : (normalizeArchLabel(opts.arch) || 'unknown');
    const osCanonical = inspection.os && inspection.os !== 'unknown'
      ? inspection.os
      : (opts.osLabel || process.platform);

    const signingStatus = inspection.signing_status === 'APPLE_DEVELOPER_ID_SIGNED'
      ? 'APPLE_DEVELOPER_ID_SIGNED'
      : 'UNSIGNED_DEVELOPMENT_ARTIFACT';

    const notarizationStatus = osCanonical === 'macos'
      ? 'NOT_NOTARIZED (no Apple Developer credentials configured; Gatekeeper will quarantine on clean macOS)'
      : 'NOT_APPLICABLE (non-macOS artifact; unsigned development build)';

    const entry = {
      name: baseName,
      relative_path: relativePath,
      artifact_kind: inspection.artifact_kind,
      is_installer: inspection.is_installer,
      os: osCanonical,
      os_runner_label: opts.osLabel || osCanonical,
      cpu_architecture: normalizedArch,
      rust_target_triple: targetTriple,
      build_commit_sha: commitSha,
      rust_version: rustVersion,
      tauri_version: tauriVersion,
      file_size_bytes: inspection.file_size_bytes,
      sha256: inspection.sha256,
      provenance_trust_level: provenanceTrustLevel,
      signing_status: signingStatus,
      notarization_status: notarizationStatus,
      test_status: testStatus,
      zalo_runtime_compat_status: buildZaloRuntimeCompatStatus(osCanonical),
      vng_binary_embedded: false,
      build_log_url: buildLogUrl,
      inspection: inspection.details,
    };

    validateManifestEntrySection7(entry);
    return entry;
  });

  const manifest = {
    schema_version: 2,
    manifest_kind: 'cross-platform-artifact-manifest',
    generated_at_utc: new Date().toISOString(),
    os_label: opts.osLabel || process.platform,
    target_triple: opts.target || null,
    build_commit_sha: commitSha,
    rust_version: rustVersion,
    tauri_version: tauriVersion,
    provenance_trust_level: provenanceTrustLevel,
    supply_chain_separation: {
      sbom_vs_provenance: 'SBOM_IS_DECLARED_INVENTORY_NOT_INDEPENDENT_BUILD_PROVENANCE',
      provenance_trust_level: provenanceTrustLevel,
      oidc_slsa_attestation_present: false,
      signing_status: 'UNSIGNED_DEVELOPMENT_ARTIFACT',
      raw_executables_are_installers: false,
    },
    test_status: testStatus,
    runtime_fixture_status: buildSummary?.runtime_fixture_status || null,
    bundle_status: buildSummary?.bundle_status || null,
    bundle_diagnostic_reason: buildSummary?.bundle_diagnostic_reason || null,
    signing_policy: 'UNSIGNED_DEVELOPMENT_ARTIFACT (never production or signed release)',
    zero_vng_binary_policy_passed: true,
    artifacts_count: artifacts.length,
    artifacts,
  };

  const sha256Lines = artifacts.map((a) => `${a.sha256}  ${a.relative_path}`);
  return { manifest, sha256Text: sha256Lines.length > 0 ? `${sha256Lines.join('\n')}\n` : '' };
}

export function mergeManifestsFromDirectory(mergeDir, outManifestPath) {
  const absDir = path.resolve(mergeDir);
  const absOut = outManifestPath ? path.resolve(outManifestPath) : null;
  if (!fs.existsSync(absDir)) {
    throw new Error(`--merge-manifests directory does not exist: ${absDir}`);
  }

  const manifestFiles = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const full = path.join(dir, entry.name);
      if (absOut && path.resolve(full) === absOut) continue;
      if (entry.isDirectory()) {
        walk(full);
      } else if (entry.isFile() && entry.name.endsWith('.json')) {
        try {
          const parsed = JSON.parse(fs.readFileSync(full, 'utf8'));
          if (
            parsed &&
            typeof parsed === 'object' &&
            Array.isArray(parsed.artifacts) &&
            (parsed.manifest_kind === 'cross-platform-artifact-manifest' || entry.name.includes('manifest'))
          ) {
            manifestFiles.push({ path: full, data: parsed });
          }
        } catch {
          // Ignore malformed non-manifest files
        }
      }
    }
  };
  walk(absDir);

  if (manifestFiles.length === 0) {
    throw new Error(`No artifact-manifest JSON files found under ${absDir}`);
  }

  const mergedArtifacts = [];
  const sourceManifestSummaries = [];

  for (const mf of manifestFiles) {
    sourceManifestSummaries.push({
      source_file: path.relative(absDir, mf.path).replace(/\\/g, '/'),
      os_label: mf.data.os_label || null,
      target_triple: mf.data.target_triple || null,
      provenance_trust_level: mf.data.provenance_trust_level || 'SELF_REPORTED_UNATTESTED',
      bundle_status: mf.data.bundle_status || null,
      artifacts_count: mf.data.artifacts.length,
    });

    for (const rawEntry of mf.data.artifacts) {
      const entry = {
        provenance_trust_level: mf.data.provenance_trust_level || 'SELF_REPORTED_UNATTESTED',
        ...rawEntry,
      };
      if (FORBIDDEN_VNG_ARTIFACT_RE.test(String(entry.name || ''))) {
        throw new Error(`FATAL Zero-VNG-Binary Policy violation in ${mf.path}: ${entry.name}`);
      }
      validateManifestEntrySection7(entry);
      mergedArtifacts.push(entry);
    }
  }

  const first = manifestFiles[0].data;
  const mergedManifest = {
    schema_version: 2,
    manifest_kind: 'cross-platform-artifact-manifest-merged',
    generated_at_utc: new Date().toISOString(),
    merged_manifests_count: manifestFiles.length,
    source_manifests: sourceManifestSummaries,
    build_commit_sha: first.build_commit_sha || resolveGitCommitSha(),
    rust_version: first.rust_version || 'multiple-runners',
    tauri_version: first.tauri_version || readTauriVersionFromLockfile(),
    provenance_trust_level: first.provenance_trust_level || 'SELF_REPORTED_UNATTESTED',
    signing_policy: 'UNSIGNED_DEVELOPMENT_ARTIFACT (never production or signed release)',
    zero_vng_binary_policy_passed: true,
    artifacts_count: mergedArtifacts.length,
    artifacts: mergedArtifacts,
  };

  const sha256Lines = mergedArtifacts.map(
    (a) => `${a.sha256}  ${a.rust_target_triple}/${a.relative_path || a.name}`,
  );
  return {
    manifest: mergedManifest,
    sha256Text: sha256Lines.length > 0 ? `${sha256Lines.join('\n')}\n` : '',
  };
}

function parseArgs(argv) {
  const opts = {
    artifactsDir: null,
    scanDefaultTargets: false,
    target: null,
    buildSummary: null,
    osLabel: null,
    arch: null,
    testStatus: null,
    provenanceTrustLevel: null,
    buildLogUrl: null,
    mergeManifests: null,
    outManifest: null,
    outSha256: null,
    outMd: null,
    allowEmpty: false,
    help: false,
  };

  const readVal = (flag, idx) => {
    if (idx + 1 >= argv.length) throw new Error(`Missing value for ${flag}`);
    return argv[idx + 1];
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') opts.help = true;
    else if (arg === '--scan-default-targets') opts.scanDefaultTargets = true;
    else if (arg === '--allow-empty') opts.allowEmpty = true;
    else if (arg === '--artifacts-dir' || arg === '--artifacts') {
      const v = readVal(arg, i++);
      opts.artifactsDir = opts.artifactsDir ? `${opts.artifactsDir},${v}` : v;
    } else if (arg.startsWith('--artifacts-dir=')) {
      const v = arg.slice('--artifacts-dir='.length);
      opts.artifactsDir = opts.artifactsDir ? `${opts.artifactsDir},${v}` : v;
    } else if (arg === '--target') opts.target = readVal(arg, i++);
    else if (arg.startsWith('--target=')) opts.target = arg.slice('--target='.length);
    else if (arg === '--build-summary') opts.buildSummary = readVal(arg, i++);
    else if (arg.startsWith('--build-summary=')) opts.buildSummary = arg.slice('--build-summary='.length);
    else if (arg === '--os-label') opts.osLabel = readVal(arg, i++);
    else if (arg.startsWith('--os-label=')) opts.osLabel = arg.slice('--os-label='.length);
    else if (arg === '--arch') opts.arch = readVal(arg, i++);
    else if (arg.startsWith('--arch=')) opts.arch = arg.slice('--arch='.length);
    else if (arg === '--test-status') opts.testStatus = readVal(arg, i++);
    else if (arg.startsWith('--test-status=')) opts.testStatus = arg.slice('--test-status='.length);
    else if (arg === '--provenance-trust-level') opts.provenanceTrustLevel = readVal(arg, i++);
    else if (arg.startsWith('--provenance-trust-level=')) {
      opts.provenanceTrustLevel = arg.slice('--provenance-trust-level='.length);
    } else if (arg === '--build-log-url') opts.buildLogUrl = readVal(arg, i++);
    else if (arg.startsWith('--build-log-url=')) opts.buildLogUrl = arg.slice('--build-log-url='.length);
    else if (arg === '--merge-manifests') opts.mergeManifests = readVal(arg, i++);
    else if (arg.startsWith('--merge-manifests=')) opts.mergeManifests = arg.slice('--merge-manifests='.length);
    else if (arg === '--out-manifest') opts.outManifest = readVal(arg, i++);
    else if (arg.startsWith('--out-manifest=')) opts.outManifest = arg.slice('--out-manifest='.length);
    else if (arg === '--out-sha256') opts.outSha256 = readVal(arg, i++);
    else if (arg.startsWith('--out-sha256=')) opts.outSha256 = arg.slice('--out-sha256='.length);
    else if (arg === '--out-md') opts.outMd = readVal(arg, i++);
    else if (arg.startsWith('--out-md=')) opts.outMd = arg.slice('--out-md='.length);
    else throw new Error(`Unknown CLI option: ${arg}`);
  }
  return opts;
}

function cliMain() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help || !opts.outManifest) {
    console.log(
      'Usage: node generate-artifact-manifest.mjs [--artifacts-dir <paths>] [--scan-default-targets] [--target <triple>] [--build-summary <json>] [--os-label <str>] [--arch <str>] [--test-status <str>] [--provenance-trust-level <str>] [--build-log-url <url>] [--merge-manifests <dir>] --out-manifest <manifest.json> [--out-sha256 <SHA256SUMS.txt>] [--out-md <summary.md>]',
    );
    process.exit(opts.help ? 0 : 1);
  }

  const { manifest, sha256Text } = opts.mergeManifests
    ? mergeManifestsFromDirectory(opts.mergeManifests, opts.outManifest)
    : generateManifest(opts);

  const outManifestAbs = path.resolve(opts.outManifest);
  fs.mkdirSync(path.dirname(outManifestAbs), { recursive: true });
  fs.writeFileSync(outManifestAbs, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');

  if (opts.outSha256) {
    const outShaAbs = path.resolve(opts.outSha256);
    fs.mkdirSync(path.dirname(outShaAbs), { recursive: true });
    fs.writeFileSync(outShaAbs, sha256Text, 'utf8');
  }

  if (opts.outMd) {
    const outMdAbs = path.resolve(opts.outMd);
    fs.mkdirSync(path.dirname(outMdAbs), { recursive: true });
    fs.writeFileSync(outMdAbs, renderManifestMarkdown(manifest), 'utf8');
  }

  console.log(JSON.stringify({
    manifest_kind: manifest.manifest_kind,
    artifacts_count: manifest.artifacts_count,
    build_commit_sha: manifest.build_commit_sha,
    tauri_version: manifest.tauri_version,
    provenance_trust_level: manifest.provenance_trust_level,
    zero_vng_binary_policy_passed: manifest.zero_vng_binary_policy_passed,
    out_manifest: outManifestAbs,
    out_sha256: opts.outSha256 ? path.resolve(opts.outSha256) : null,
    out_md: opts.outMd ? path.resolve(opts.outMd) : null,
  }, null, 2));
}

if (isMainModule(import.meta.url)) {
  try {
    cliMain();
  } catch (err) {
    console.error(`[generate-artifact-manifest] ERROR: ${err.message}`);
    process.exit(1);
  }
}
