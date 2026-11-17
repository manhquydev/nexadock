#!/usr/bin/env node
/**
 * Cross-platform test, release build, bundler wrapper, and Zero-VNG-Binary gate
 * for Multi-Zalo Desktop (desktop-instance-manager).
 *
 * CLI:
 *   node build-and-package.mjs --target <triple> [--bundles <all|msi|nsis|app|dmg|deb|appimage|none>] [--skip-tests] [--require-bundler] [--dry-run] [--out <build-summary-json>]
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { runPrerequisitesCheck } from './check-prerequisites.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const WORKSPACE_ROOT = path.resolve(__dirname, '..', '..');

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

function defaultHostTriple() {
  if (process.platform === 'win32') {
    return process.arch === 'arm64' ? 'aarch64-pc-windows-msvc' : 'x86_64-pc-windows-msvc';
  }
  if (process.platform === 'darwin') {
    return process.arch === 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin';
  }
  return process.arch === 'arm64' ? 'aarch64-unknown-linux-gnu' : 'x86_64-unknown-linux-gnu';
}

function buildPackagingEnv(baseEnv = process.env) {
  const extraDirs = [];
  if (baseEnv.CARGO_HOME) extraDirs.push(path.join(baseEnv.CARGO_HOME, 'bin'));
  const home = os.homedir();
  if (home) extraDirs.push(path.join(home, '.cargo', 'bin'));

  if (process.platform === 'win32') {
    const pf86 = baseEnv['ProgramFiles(x86)'] || 'C:\\Program Files (x86)';
    const pf = baseEnv.ProgramFiles || 'C:\\Program Files';
    if (baseEnv.WIX) extraDirs.push(path.join(baseEnv.WIX, 'bin'));
    extraDirs.push(
      path.join(pf86, 'WiX Toolset v3.14', 'bin'),
      path.join(pf86, 'WiX Toolset v3.11', 'bin'),
      path.join(pf86, 'NSIS'),
      path.join(pf, 'NSIS'),
    );
  }

  const pathKey = Object.keys(baseEnv).find((k) => k.toLowerCase() === 'path') || 'PATH';
  const mergedPath = [...extraDirs.filter((d) => fs.existsSync(d)), baseEnv[pathKey] || '']
    .filter(Boolean)
    .join(path.delimiter);

  const env = { ...baseEnv, [pathKey]: mergedPath };
  if (process.platform === 'linux' && !env.APPIMAGE_EXTRACT_AND_RUN) {
    // Enables linuxdeploy AppImage execution inside CI runners without /dev/fuse mount privileges
    env.APPIMAGE_EXTRACT_AND_RUN = '1';
  }
  return env;
}

function parseTargetPlatformAndArch(triple) {
  let arch = 'unknown';
  if (triple.startsWith('x86_64-')) arch = 'x64';
  else if (triple.startsWith('aarch64-')) arch = 'arm64';
  else if (triple.startsWith('i686-')) arch = 'ia32';

  let platform = 'unknown';
  if (triple.includes('-windows-')) platform = 'win32';
  else if (triple.includes('-apple-darwin')) platform = 'darwin';
  else if (triple.includes('-linux-')) platform = 'linux';

  return { arch, platform };
}

function runStep(label, bin, args, opts = {}) {
  const startedAt = Date.now();
  const env = opts.env || buildPackagingEnv();
  const useShell = process.platform === 'win32' && /\.(cmd|bat)$/i.test(bin);

  console.error(`[build-and-package] >>> ${label}: ${bin} ${args.join(' ')}`);
  if (opts.dryRun) {
    return {
      label,
      command: `${bin} ${args.join(' ')}`,
      exit_code: 0,
      duration_ms: 0,
      ok: true,
      dry_run: true,
      stderr_tail: '',
    };
  }

  const res = spawnSync(bin, args, {
    cwd: opts.cwd || WORKSPACE_ROOT,
    env,
    encoding: 'utf8',
    maxBuffer: 128 * 1024 * 1024,
    shell: useShell,
    windowsHide: true,
  });

  if (res.stdout) process.stderr.write(res.stdout);
  if (res.stderr) process.stderr.write(res.stderr);

  const durationMs = Date.now() - startedAt;
  const stderrTail = (res.stderr || res.stdout || '')
    .trim()
    .split(/\r?\n/)
    .slice(-25)
    .join('\n');

  return {
    label,
    command: `${bin} ${args.join(' ')}`,
    exit_code: res.status,
    duration_ms: durationMs,
    ok: !res.error && res.status === 0,
    error: res.error ? res.error.message : null,
    stderr_tail: stderrTail,
  };
}

export function scanTargetDirsForVngBinaries(target) {
  const roots = [
    path.join(WORKSPACE_ROOT, 'target', target, 'release'),
    path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'target', target, 'release'),
  ];
  const forbidden = [
    /^zalo\.exe$/i,
    /^zalosetup.*\.exe$/i,
    /^zalo.*\.msi$/i,
    /^zalo.*\.dmg$/i,
    /^zalo.*\.deb$/i,
    /^zalo.*\.appimage$/i,
    /^zcall\.dll$/i,
    /^zalocore\.dll$/i,
    /^app\.asar$/i,
  ];
  const violations = [];

  function walk(dir) {
    if (!fs.existsSync(dir)) return;
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
      if (['deps', 'build', '.fingerprint', 'incremental'].includes(entry.name)) continue;
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (/^zalo\.app$/i.test(entry.name)) {
          violations.push(path.relative(WORKSPACE_ROOT, full).replace(/\\/g, '/'));
          continue;
        }
        walk(full);
      } else if (entry.isFile() && forbidden.some((re) => re.test(entry.name))) {
        violations.push(path.relative(WORKSPACE_ROOT, full).replace(/\\/g, '/'));
      }
    }
  }

  for (const r of roots) walk(r);
  return violations;
}

const VALID_BUNDLE_TOKENS = new Set(['all', 'msi', 'nsis', 'app', 'dmg', 'deb', 'appimage', 'none']);

function parseArgs(argv) {
  const opts = {
    target: null,
    bundles: 'all',
    skipTests: false,
    requireBundler: false,
    dryRun: false,
    out: null,
    help: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') opts.help = true;
    else if (arg === '--skip-tests') opts.skipTests = true;
    else if (arg === '--require-bundler') opts.requireBundler = true;
    else if (arg === '--dry-run') opts.dryRun = true;
    else if (arg === '--target' && i + 1 < argv.length) opts.target = argv[++i];
    else if (arg.startsWith('--target=')) opts.target = arg.slice('--target='.length);
    else if (arg === '--bundles' && i + 1 < argv.length) opts.bundles = argv[++i].toLowerCase();
    else if (arg.startsWith('--bundles=')) opts.bundles = arg.slice('--bundles='.length).toLowerCase();
    else if (arg === '--out' && i + 1 < argv.length) opts.out = argv[++i];
    else if (arg.startsWith('--out=')) opts.out = arg.slice('--out='.length);
    else throw new Error(`Unknown or incomplete CLI option: ${arg}`);
  }
  if (!opts.target) opts.target = defaultHostTriple();

  const tokens = opts.bundles.split(',').map((s) => s.trim()).filter(Boolean);
  if (tokens.length === 0 || tokens.some((t) => !VALID_BUNDLE_TOKENS.has(t))) {
    throw new Error(
      `Invalid --bundles value "${opts.bundles}". Expected one or more of: ${Array.from(VALID_BUNDLE_TOKENS).join(', ')}`,
    );
  }
  return opts;
}

function defaultOsBundlesForAll(target) {
  if (target.includes('-windows-')) return ['msi', 'nsis'];
  if (target.includes('-apple-darwin')) return ['app', 'dmg'];
  if (target.includes('-linux-')) return ['deb', 'appimage'];
  return [];
}

function writeSummaryIfRequested(outPath, summary) {
  if (!outPath) return;
  const abs = path.resolve(outPath);
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  fs.writeFileSync(abs, `${JSON.stringify(summary, null, 2)}\n`, 'utf8');
}

export function runBuildAndPackage(opts) {
  const env = buildPackagingEnv();
  const prereq = runPrerequisitesCheck({ target: opts.target });
  const cargoBin = prereq.toolchain.cargo_path || 'cargo';
  const steps = [];

  const { arch: targetArch, platform: targetPlatform } = parseTargetPlatformAndArch(opts.target);
  const isCrossCompile = targetArch !== process.arch || targetPlatform !== process.platform;

  let testStatus = 'NOT_RUN';
  let compileStatus = 'NOT_RUN';
  let bundleStatus = 'NOT_RUN';
  let bundleReason = null;

  // Step 1: Workspace + Tauri unit/integration tests (or cross-arch `--tests` compile check)
  if (opts.skipTests) {
    testStatus = 'SKIPPED_BY_FLAG';
  } else if (! isCrossCompile) {
    const wsTest = runStep(
      'workspace-tests',
      cargoBin,
      ['test', '--workspace', '--all-targets', '--target', opts.target, '--locked'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(wsTest);
    if (!wsTest.ok) {
      testStatus = 'FAILED';
      const failSummary = buildFinalSummary(opts, prereq, isCrossCompile, testStatus, compileStatus, bundleStatus, bundleReason, [], steps);
      writeSummaryIfRequested(opts.out, failSummary);
      return { exitCode: 1, summary: failSummary };
    }

    const tauriTest = runStep(
      'tauri-crate-tests',
      cargoBin,
      ['test', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--target', opts.target, '--locked'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(tauriTest);
    if (!tauriTest.ok) {
      testStatus = 'FAILED';
      const failSummary = buildFinalSummary(opts, prereq, isCrossCompile, testStatus, compileStatus, bundleStatus, bundleReason, [], steps);
      writeSummaryIfRequested(opts.out, failSummary);
      return { exitCode: 1, summary: failSummary };
    }
    testStatus = 'PASSED';
  } else {
    // Cross-architecture build (e.g. x86_64-apple-darwin on arm64 runner): compile test targets without executing foreign binaries
    const wsBuildTests = runStep(
      'workspace-compile-tests-cross-arch',
      cargoBin,
      ['build', '--workspace', '--tests', '--target', opts.target, '--locked'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(wsBuildTests);
    if (!wsBuildTests.ok) {
      testStatus = 'FAILED_CROSS_ARCH_TEST_BUILD';
      const failSummary = buildFinalSummary(opts, prereq, isCrossCompile, testStatus, compileStatus, bundleStatus, bundleReason, [], steps);
      writeSummaryIfRequested(opts.out, failSummary);
      return { exitCode: 1, summary: failSummary };
    }

    const tauriBuildTests = runStep(
      'tauri-compile-tests-cross-arch',
      cargoBin,
      ['build', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--tests', '--target', opts.target, '--locked'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(tauriBuildTests);
    if (!tauriBuildTests.ok) {
      testStatus = 'FAILED_CROSS_ARCH_TEST_BUILD';
      const failSummary = buildFinalSummary(opts, prereq, isCrossCompile, testStatus, compileStatus, bundleStatus, bundleReason, [], steps);
      writeSummaryIfRequested(opts.out, failSummary);
      return { exitCode: 1, summary: failSummary };
    }
    testStatus = `COMPILED_TESTS_ONLY_CROSS_ARCH (host=${process.platform}-${process.arch}, target=${opts.target})`;
  }

  // Step 2: Build release binaries (`dim-cli`, `dim-test-fixture`, and `dim-desktop`)
  const wsRelease = runStep(
    'workspace-release-build',
    cargoBin,
    ['build', '--workspace', '--release', '--target', opts.target, '--locked'],
    { env, dryRun: opts.dryRun },
  );
  steps.push(wsRelease);
  if (!wsRelease.ok) {
    compileStatus = 'FAILED';
    const failSummary = buildFinalSummary(opts, prereq, isCrossCompile, testStatus, compileStatus, bundleStatus, bundleReason, [], steps);
    writeSummaryIfRequested(opts.out, failSummary);
    return { exitCode: 1, summary: failSummary };
  }

  const tauriRelease = runStep(
    'tauri-desktop-release-build',
    cargoBin,
    ['build', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--release', '--target', opts.target, '--locked'],
    { env, dryRun: opts.dryRun },
  );
  steps.push(tauriRelease);
  if (!tauriRelease.ok) {
    compileStatus = 'FAILED';
    const failSummary = buildFinalSummary(opts, prereq, isCrossCompile, testStatus, compileStatus, bundleStatus, bundleReason, [], steps);
    writeSummaryIfRequested(opts.out, failSummary);
    return { exitCode: 1, summary: failSummary };
  }
  compileStatus = 'SUCCEEDED';

  // Step 3: OS Installer / Bundle Packaging (`cargo tauri build`) if requested
  if (opts.bundles === 'none') {
    bundleStatus = 'SKIPPED_NONE_REQUESTED';
    bundleReason = 'Caller passed --bundles none; compiled release executables only.';
  } else if (!prereq.toolchain.tauri_cli.available) {
    bundleStatus = 'SKIPPED_MISSING_TOOLING';
    bundleReason =
      'Tauri CLI (cargo-tauri / @tauri-apps/cli) is not installed in this environment; compiled release executables preserved.';
  } else {
    let requestedBundles = opts.bundles === 'all'
      ? defaultOsBundlesForAll(opts.target)
      : opts.bundles.split(',').map((s) => s.trim()).filter(Boolean);

    // Filter Windows bundles if one of WiX or NSIS is unavailable, or skip cleanly if neither is installed
    if (opts.target.includes('-windows-') && prereq.os_checks.windows.applicable_on_host) {
      const winBundlers = prereq.os_checks.windows.bundlers;
      const availableWin = requestedBundles.filter((b) => {
        if (b === 'msi') return winBundlers.wix_msi.available;
        if (b === 'nsis') return winBundlers.nsis_setup.available;
        return true;
      });
      if (availableWin.length === 0) {
        bundleStatus = 'SKIPPED_MISSING_TOOLING';
        bundleReason = `Requested Windows bundle(s) [${requestedBundles.join(', ')}] unavailable: WiX=${winBundlers.wix_msi.available}, NSIS=${winBundlers.nsis_setup.available}. Compiled .exe artifacts preserved.`;
        requestedBundles = [];
      } else {
        if (availableWin.length < requestedBundles.length) {
          bundleReason = `Filtered Windows bundles to installed tooling [${availableWin.join(', ')}] (requested [${requestedBundles.join(', ')}]).`;
        }
        requestedBundles = availableWin;
      }
    }

    if (requestedBundles.length > 0) {
      const tauriInvocation = prereq.toolchain.tauri_cli.invocation === 'cargo tauri'
        ? { bin: cargoBin, prefix: ['tauri'] }
        : { bin: prereq.toolchain.tauri_cli.invocation, prefix: [] };

      const bundleArgs = [
        ...tauriInvocation.prefix,
        'build',
        '--target',
        opts.target,
        '--bundles',
        requestedBundles.join(','),
      ];
      const tauriCwd = path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri');
      const bundleStep = runStep('tauri-bundle-package', tauriInvocation.bin, bundleArgs, {
        cwd: tauriCwd,
        env,
        dryRun: opts.dryRun,
      });
      steps.push(bundleStep);

      if (bundleStep.ok) {
        bundleStatus = 'SUCCEEDED';
        if (!bundleReason) {
          bundleReason = `Bundled [${requestedBundles.join(', ')}] via Tauri CLI (UNSIGNED_DEVELOPMENT_ARTIFACT).`;
        }
      } else {
        // On Linux, if `deb,appimage` failed due to linuxdeploy/AppImage FUSE/network, salvage `.deb` if not yet created
        const debDir = path.join(tauriCwd, 'target', opts.target, 'release', 'bundle', 'deb');
        const hasDeb = fs.existsSync(debDir) && fs.readdirSync(debDir).some((f) => f.endsWith('.deb'));
        if (
          opts.target.includes('-linux-') &&
          requestedBundles.includes('deb') &&
          requestedBundles.includes('appimage') &&
          !hasDeb
        ) {
          const fallbackDebStep = runStep(
            'tauri-bundle-deb-fallback',
            tauriInvocation.bin,
            [...tauriInvocation.prefix, 'build', '--target', opts.target, '--bundles', 'deb'],
            { cwd: tauriCwd, env, dryRun: opts.dryRun },
          );
          steps.push(fallbackDebStep);
        }

        bundleStatus = 'FAILED_BUNDLER';
        bundleReason = `Bundler exited with code ${bundleStep.exit_code} for [${requestedBundles.join(', ')}]. Compiled release binaries preserved. Tail: ${bundleStep.stderr_tail.slice(-500)}`;
      }
    }
  }

  // Step 4: Enforce Zero VNG Binary Policy on all output directories
  const vngViolations = opts.dryRun ? [] : scanTargetDirsForVngBinaries(opts.target);
  const finalSummary = buildFinalSummary(
    opts,
    prereq,
    isCrossCompile,
    testStatus,
    compileStatus,
    bundleStatus,
    bundleReason,
    vngViolations,
    steps,
  );
  writeSummaryIfRequested(opts.out, finalSummary);

  if (vngViolations.length > 0) {
    console.error(`[build-and-package] FATAL ZERO-VNG-BINARY POLICY VIOLATION: ${vngViolations.join(', ')}`);
    return { exitCode: 2, summary: finalSummary };
  }
  if (opts.requireBundler && bundleStatus !== 'SUCCEEDED') {
    return { exitCode: 1, summary: finalSummary };
  }
  return { exitCode: 0, summary: finalSummary };
}

function buildFinalSummary(
  opts,
  prereq,
  isCrossCompile,
  testStatus,
  compileStatus,
  bundleStatus,
  bundleReason,
  vngViolations,
  steps,
) {
  return {
    schema_version: 1,
    completed_at_utc: new Date().toISOString(),
    target: opts.target,
    host: `${process.platform}-${process.arch}`,
    is_cross_compile: isCrossCompile,
    requested_bundles: opts.bundles,
    rustc_version: prereq.toolchain.rustc_vv ? prereq.toolchain.rustc_vv.split(/\r?\n/)[0] : null,
    tauri_cli: prereq.toolchain.tauri_cli,
    test_status: testStatus,
    compile_status: compileStatus,
    bundle_status: bundleStatus,
    bundle_diagnostic_reason: bundleReason,
    signing_status: 'UNSIGNED_DEVELOPMENT_ARTIFACT',
    zero_vng_binary_policy_passed: vngViolations.length === 0,
    vng_binary_violations: vngViolations,
    steps,
  };
}

function cliMain() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    console.log(
      'Usage: node build-and-package.mjs --target <triple> [--bundles <all|msi|nsis|app|dmg|deb|appimage|none>] [--skip-tests] [--require-bundler] [--dry-run] [--out <build-summary-json>]',
    );
    process.exit(0);
  }

  const { exitCode, summary } = runBuildAndPackage(opts);
  console.log(JSON.stringify({
    target: summary.target,
    test_status: summary.test_status,
    compile_status: summary.compile_status,
    bundle_status: summary.bundle_status,
    bundle_diagnostic_reason: summary.bundle_diagnostic_reason,
    zero_vng_binary_policy_passed: summary.zero_vng_binary_policy_passed,
    out_file: opts.out ? path.resolve(opts.out) : null,
  }, null, 2));
  process.exit(exitCode);
}

if (isMainModule(import.meta.url)) {
  try {
    cliMain();
  } catch (err) {
    console.error(`[build-and-package] ERROR: ${err.message}`);
    process.exit(1);
  }
}
