#!/usr/bin/env node
/**
 * Cross-platform native verification, `--locked` Clippy/test/release build,
 * runtime fixture smoke runner, Tauri overlay bundler, and Zero-VNG-Binary gate
 * for NexaDock (desktop-instance-manager).
 *
 * Enforces:
 *   - `--locked` on ALL `cargo` invocations (clippy, test, build, and `tauri build -- --locked`)
 *   - Exact pinned `@tauri-apps/cli@2.12.5` reference (never floating `@latest`)
 *   - Non-destructive `--config <tauri-ci-bundle-overlay.json>` for `.icns`, `@2x.png`,
 *     WebView2 `downloadBootstrapper`, macOS `minimumSystemVersion`, and Ubuntu 24.04 `.deb` `depends`
 *   - Native hardware (`uname -m` / `PROCESSOR_ARCHITECTURE`) + `rustc -vV` host verification:
 *     cross-compilation is NEVER reported as a native runtime test!
 *
 * CLI:
 *   node build-and-package.mjs --target <triple> [--bundles <all|msi|nsis|app|dmg|deb|appimage|none>]
 *     [--skip-tests] [--run-clippy|--no-clippy] [--run-runtime-fixture|--no-runtime-fixture]
 *     [--tauri-config-overlay <json>] [--require-bundler] [--dry-run] [--out <build-summary-json>]
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import {
  buildAugmentedEnv,
  DEFAULT_TAURI_CI_OVERLAY_PATH,
  PINNED_TAURI_CLI_VERSION,
  PINNED_TAURI_NPM_SPEC,
  runPrerequisitesCheck,
} from './check-prerequisites.mjs';

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
  const augmented = buildAugmentedEnv(baseEnv);
  const extraDirs = [];

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

  const pathKey = Object.keys(augmented).find((k) => k.toLowerCase() === 'path') || 'PATH';
  const mergedPath = [...extraDirs.filter((d) => fs.existsSync(d)), augmented[pathKey] || '']
    .filter(Boolean)
    .join(path.delimiter);

  const env = { ...augmented, [pathKey]: mergedPath };
  if (process.platform === 'linux') {
    if (!env.APPIMAGE_EXTRACT_AND_RUN) env.APPIMAGE_EXTRACT_AND_RUN = '1';
    if (!env.NO_STRIP) env.NO_STRIP = 'true';
  }
  return env;
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
      stdout_tail: '',
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

  if (!opts.quietStdout && res.stdout) process.stderr.write(res.stdout);
  if (res.stderr) process.stderr.write(res.stderr);

  const durationMs = Date.now() - startedAt;
  const stdoutTail = (res.stdout || '').trim().split(/\r?\n/).slice(-25).join('\n');
  const stderrTail = (res.stderr || res.stdout || '').trim().split(/\r?\n/).slice(-25).join('\n');

  return {
    label,
    command: `${bin} ${args.join(' ')}`,
    exit_code: res.status,
    duration_ms: durationMs,
    ok: !res.error && res.status === 0,
    error: res.error ? res.error.message : null,
    stdout_tail: stdoutTail,
    stderr_tail: stderrTail,
    raw_stdout: opts.captureRawStdout ? (res.stdout || '') : undefined,
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
    runClippy: true,
    runRuntimeFixture: true,
    tauriConfigOverlay: DEFAULT_TAURI_CI_OVERLAY_PATH,
    requireBundler: false,
    dryRun: false,
    out: null,
    help: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') opts.help = true;
    else if (arg === '--skip-tests') {
      opts.skipTests = true;
    } else if (arg === '--run-clippy') opts.runClippy = true;
    else if (arg === '--no-clippy') opts.runClippy = false;
    else if (arg === '--run-runtime-fixture') opts.runRuntimeFixture = true;
    else if (arg === '--no-runtime-fixture') opts.runRuntimeFixture = false;
    else if (arg === '--require-bundler') opts.requireBundler = true;
    else if (arg === '--dry-run') opts.dryRun = true;
    else if (arg === '--target' && i + 1 < argv.length) opts.target = argv[++i];
    else if (arg.startsWith('--target=')) opts.target = arg.slice('--target='.length);
    else if (arg === '--bundles' && i + 1 < argv.length) opts.bundles = argv[++i].toLowerCase();
    else if (arg.startsWith('--bundles=')) opts.bundles = arg.slice('--bundles='.length).toLowerCase();
    else if (arg === '--tauri-config-overlay' && i + 1 < argv.length) opts.tauriConfigOverlay = argv[++i];
    else if (arg.startsWith('--tauri-config-overlay=')) {
      opts.tauriConfigOverlay = arg.slice('--tauri-config-overlay='.length);
    } else if (arg === '--out' && i + 1 < argv.length) opts.out = argv[++i];
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

  const nativeExec = prereq.native_execution_verification;
  const nativeRuntimeTestPossible = nativeExec.native_runtime_test_possible;

  let clippyStatus = 'NOT_RUN';
  let testStatus = 'NOT_RUN';
  let compileStatus = 'NOT_RUN';
  let runtimeFixtureStatus = 'NOT_RUN';
  let bundleStatus = 'NOT_RUN';
  let bundleReason = null;

  const abortWithSummary = (exitCode = 1) => {
    const failSummary = buildFinalSummary({
      opts,
      prereq,
      nativeRuntimeTestPossible,
      clippyStatus,
      testStatus,
      compileStatus,
      runtimeFixtureStatus,
      bundleStatus,
      bundleReason,
      vngViolations: [],
      steps,
    });
    writeSummaryIfRequested(opts.out, failSummary);
    return { exitCode, summary: failSummary };
  };

  // Stage 1: Clippy (`-D warnings --locked`) unless skipped
  if (!opts.skipTests && opts.runClippy) {
    const wsClippy = runStep(
      'workspace-clippy-locked',
      cargoBin,
      ['clippy', '--workspace', '--all-targets', '--target', opts.target, '--locked', '--', '-D', 'warnings'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(wsClippy);
    if (!wsClippy.ok) {
      clippyStatus = 'FAILED';
      return abortWithSummary(1);
    }

    const tauriClippy = runStep(
      'tauri-crate-clippy-locked',
      cargoBin,
      ['clippy', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--target', opts.target, '--locked', '--', '-D', 'warnings'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(tauriClippy);
    if (!tauriClippy.ok) {
      clippyStatus = 'FAILED';
      return abortWithSummary(1);
    }
    clippyStatus = 'PASSED';
  } else {
    clippyStatus = 'SKIPPED_BY_FLAG';
  }

  // Stage 2: Workspace + Tauri unit tests (`--locked`)
  if (opts.skipTests) {
    testStatus = 'SKIPPED_BY_FLAG';
  } else if (nativeRuntimeTestPossible) {
    const wsTest = runStep(
      'workspace-tests-locked',
      cargoBin,
      ['test', '--workspace', '--all-targets', '--target', opts.target, '--locked'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(wsTest);
    if (!wsTest.ok) {
      testStatus = 'FAILED';
      return abortWithSummary(1);
    }

    const tauriTest = runStep(
      'tauri-crate-tests-locked',
      cargoBin,
      ['test', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--target', opts.target, '--locked'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(tauriTest);
    if (!tauriTest.ok) {
      testStatus = 'FAILED';
      return abortWithSummary(1);
    }
    testStatus = 'PASSED_NATIVE';
  } else {
    // Non-native target: compile test harnesses ONLY (`--locked`) and NEVER claim native runtime execution
    const wsBuildTests = runStep(
      'workspace-compile-tests-non-native-locked',
      cargoBin,
      ['build', '--workspace', '--tests', '--target', opts.target, '--locked'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(wsBuildTests);
    if (!wsBuildTests.ok) {
      testStatus = 'FAILED_NON_NATIVE_TEST_COMPILE';
      return abortWithSummary(1);
    }

    const tauriBuildTests = runStep(
      'tauri-compile-tests-non-native-locked',
      cargoBin,
      ['build', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--tests', '--target', opts.target, '--locked'],
      { env, dryRun: opts.dryRun },
    );
    steps.push(tauriBuildTests);
    if (!tauriBuildTests.ok) {
      testStatus = 'FAILED_NON_NATIVE_TEST_COMPILE';
      return abortWithSummary(1);
    }
    testStatus = `CROSS_COMPILE_ONLY_NOT_NATIVE_RUNTIME_VERIFIED (host=${prereq.toolchain.rustc_host}, target=${opts.target})`;
  }

  // Stage 3: Build release binaries (`dim-cli`, `dim-test-fixture`, and `dim-desktop`) with `--locked`
  const wsRelease = runStep(
    'workspace-release-build-locked',
    cargoBin,
    ['build', '--workspace', '--release', '--target', opts.target, '--locked'],
    { env, dryRun: opts.dryRun },
  );
  steps.push(wsRelease);
  if (!wsRelease.ok) {
    compileStatus = 'FAILED';
    return abortWithSummary(1);
  }

  const tauriRelease = runStep(
    'tauri-desktop-release-build-locked',
    cargoBin,
    ['build', '--manifest-path', 'apps/desktop/src-tauri/Cargo.toml', '--release', '--target', opts.target, '--locked'],
    { env, dryRun: opts.dryRun },
  );
  steps.push(tauriRelease);
  if (!tauriRelease.ok) {
    compileStatus = 'FAILED';
    return abortWithSummary(1);
  }
  compileStatus = 'SUCCEEDED';

  // Stage 4: Native Runtime Fixture Smoke Check (`fixture-app.mjs` + native `dim-cli`)
  if (opts.skipTests || !opts.runRuntimeFixture) {
    runtimeFixtureStatus = 'SKIPPED_BY_FLAG';
  } else if (!nativeRuntimeTestPossible) {
    runtimeFixtureStatus = 'BLOCKED_NON_NATIVE_TARGET (cross-compilation is never treated as a native runtime test)';
  } else if (opts.dryRun) {
    runtimeFixtureStatus = 'PASSED_DRY_RUN';
  } else {
    const tmpSandbox = fs.mkdtempSync(path.join(os.tmpdir(), 'dim-native-runtime-smoke-'));
    try {
      const sandboxEnv = {
        ...env,
        APPDATA: tmpSandbox,
        LOCALAPPDATA: tmpSandbox,
        USERPROFILE: tmpSandbox,
        HOME: tmpSandbox,
        XDG_CONFIG_HOME: tmpSandbox,
        XDG_DATA_HOME: tmpSandbox,
      };

      const nodeFixtureStep = runStep(
        'runtime-fixture-node-smoke',
        process.execPath,
        [path.join('fixtures', 'test-desktop-app', 'fixture-app.mjs'), '--stay-ms=100'],
        { env: sandboxEnv, quietStdout: true },
      );
      steps.push(nodeFixtureStep);
      if (!nodeFixtureStep.ok) {
        runtimeFixtureStatus = 'FAILED_NODE_FIXTURE';
        return abortWithSummary(1);
      }

      const ext = opts.target.includes('-windows-') ? '.exe' : '';
      const dimCliPath = path.join(WORKSPACE_ROOT, 'target', opts.target, 'release', `dim-cli${ext}`);
      const cliSmokeStep = runStep(
        'runtime-fixture-native-dim-cli-status',
        dimCliPath,
        ['--cli', '--json', '--data-dir', path.join(tmpSandbox, 'dim-data'), 'status'],
        { env: sandboxEnv, quietStdout: true, captureRawStdout: true },
      );
      const rawOut = cliSmokeStep.raw_stdout || '';
      delete cliSmokeStep.raw_stdout;
      steps.push(cliSmokeStep);

      let cliJsonOk = false;
      if (cliSmokeStep.ok && rawOut) {
        try {
          cliJsonOk = JSON.parse(rawOut).ok === true;
        } catch {
          cliJsonOk = false;
        }
      }
      if (!cliSmokeStep.ok || !cliJsonOk) {
        runtimeFixtureStatus = 'FAILED_NATIVE_DIM_CLI';
        return abortWithSummary(1);
      }
      runtimeFixtureStatus = 'PASSED_NATIVE_FIXTURE_AND_DIM_CLI';
    } finally {
      fs.rmSync(tmpSandbox, { recursive: true, force: true });
    }
  }

  // Stage 5: OS Installer / Bundle Packaging (`tauri build --config <overlay> -- --locked`)
  const overlayAbs = opts.tauriConfigOverlay ? path.resolve(opts.tauriConfigOverlay) : null;
  const hasOverlay = Boolean(overlayAbs && fs.existsSync(overlayAbs));

  if (opts.bundles === 'none') {
    bundleStatus = 'SKIPPED_NONE_REQUESTED';
    bundleReason = 'Caller passed --bundles none; compiled release executables only.';
  } else if (!prereq.toolchain.tauri_cli.available) {
    bundleStatus = 'SKIPPED_MISSING_TOOLING';
    bundleReason =
      `Tauri CLI (${PINNED_TAURI_NPM_SPEC} / cargo-tauri) is not installed in this environment; compiled release executables preserved.`;
  } else {
    let requestedBundles = opts.bundles === 'all'
      ? defaultOsBundlesForAll(opts.target)
      : opts.bundles.split(',').map((s) => s.trim()).filter(Boolean);

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
      if (hasOverlay) {
        bundleArgs.push('--config', overlayAbs);
      }
      bundleArgs.push('--', '--locked');

      const tauriCwd = path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri');
      const bundleStep = runStep('tauri-bundle-package-locked', tauriInvocation.bin, bundleArgs, {
        cwd: tauriCwd,
        env,
        dryRun: opts.dryRun,
      });
      steps.push(bundleStep);

      if (bundleStep.ok) {
        bundleStatus = 'SUCCEEDED';
        if (!bundleReason) {
          bundleReason = `Bundled [${requestedBundles.join(', ')}] via ${prereq.toolchain.tauri_cli.version} with overlay=${hasOverlay} (UNSIGNED_DEVELOPMENT_ARTIFACT).`;
        }
      } else {
        const debDir = path.join(tauriCwd, 'target', opts.target, 'release', 'bundle', 'deb');
        const hasDeb = fs.existsSync(debDir) && fs.readdirSync(debDir).some((f) => f.endsWith('.deb'));
        if (
          opts.target.includes('-linux-') &&
          requestedBundles.includes('deb') &&
          requestedBundles.includes('appimage') &&
          !hasDeb
        ) {
          const fallbackArgs = [
            ...tauriInvocation.prefix,
            'build',
            '--target',
            opts.target,
            '--bundles',
            'deb',
          ];
          if (hasOverlay) fallbackArgs.push('--config', overlayAbs);
          fallbackArgs.push('--', '--locked');
          const fallbackDebStep = runStep('tauri-bundle-deb-fallback-locked', tauriInvocation.bin, fallbackArgs, {
            cwd: tauriCwd,
            env,
            dryRun: opts.dryRun,
          });
          steps.push(fallbackDebStep);
        }

        bundleStatus = 'FAILED_BUNDLER';
        bundleReason = `Bundler exited with code ${bundleStep.exit_code} for [${requestedBundles.join(', ')}]. Compiled release binaries preserved. Tail: ${bundleStep.stderr_tail.slice(-500)}`;
      }
    }
  }

  // Stage 6: Enforce Zero VNG Binary Policy on all output directories
  const vngViolations = opts.dryRun ? [] : scanTargetDirsForVngBinaries(opts.target);
  const finalSummary = buildFinalSummary({
    opts,
    prereq,
    nativeRuntimeTestPossible,
    clippyStatus,
    testStatus,
    compileStatus,
    runtimeFixtureStatus,
    bundleStatus,
    bundleReason,
    vngViolations,
    steps,
  });
  writeSummaryIfRequested(opts.out, finalSummary);

  if (vngViolations.length > 0) {
    console.error(`[build-and-package] FATAL ZERO-VNG-BINARY POLICY VIOLATION: ${vngViolations.join(', ')}`);
    return { exitCode: 2, summary: finalSummary };
  }
  if (opts.requireBundler && opts.bundles !== 'none' && bundleStatus !== 'SUCCEEDED') {
    return { exitCode: 1, summary: finalSummary };
  }
  return { exitCode: 0, summary: finalSummary };
}

function buildFinalSummary({
  opts,
  prereq,
  nativeRuntimeTestPossible,
  clippyStatus,
  testStatus,
  compileStatus,
  runtimeFixtureStatus,
  bundleStatus,
  bundleReason,
  vngViolations,
  steps,
}) {
  return {
    schema_version: 2,
    completed_at_utc: new Date().toISOString(),
    target: opts.target,
    host: `${process.platform}-${process.arch}`,
    native_execution_verification: prereq.native_execution_verification,
    native_runtime_test_possible: nativeRuntimeTestPossible,
    cargo_locked_enforced: true,
    pinned_tauri_cli_spec: PINNED_TAURI_NPM_SPEC,
    pinned_tauri_cli_version: PINNED_TAURI_CLI_VERSION,
    tauri_config_overlay_used: opts.tauriConfigOverlay && fs.existsSync(path.resolve(opts.tauriConfigOverlay))
      ? path.relative(WORKSPACE_ROOT, path.resolve(opts.tauriConfigOverlay)).replace(/\\/g, '/')
      : null,
    requested_bundles: opts.bundles,
    rustc_version: prereq.toolchain.rustc_vv ? prereq.toolchain.rustc_vv.split(/\r?\n/)[0] : null,
    tauri_cli: prereq.toolchain.tauri_cli,
    clippy_status: clippyStatus,
    test_status: testStatus,
    compile_status: compileStatus,
    runtime_fixture_status: runtimeFixtureStatus,
    bundle_status: bundleStatus,
    bundle_diagnostic_reason: bundleReason,
    signing_status: 'UNSIGNED_DEVELOPMENT_ARTIFACT',
    provenance_trust_level: 'SELF_REPORTED_UNATTESTED',
    zero_vng_binary_policy_passed: vngViolations.length === 0,
    vng_binary_violations: vngViolations,
    steps,
  };
}

function cliMain() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    console.log(
      'Usage: node build-and-package.mjs --target <triple> [--bundles <all|msi|nsis|app|dmg|deb|appimage|none>] [--skip-tests] [--run-clippy|--no-clippy] [--run-runtime-fixture|--no-runtime-fixture] [--tauri-config-overlay <json>] [--require-bundler] [--dry-run] [--out <build-summary-json>]',
    );
    process.exit(0);
  }

  const { exitCode, summary } = runBuildAndPackage(opts);
  console.log(JSON.stringify({
    target: summary.target,
    native_runtime_test_possible: summary.native_runtime_test_possible,
    cargo_locked_enforced: summary.cargo_locked_enforced,
    pinned_tauri_cli_spec: summary.pinned_tauri_cli_spec,
    clippy_status: summary.clippy_status,
    test_status: summary.test_status,
    compile_status: summary.compile_status,
    runtime_fixture_status: summary.runtime_fixture_status,
    bundle_status: summary.bundle_status,
    bundle_diagnostic_reason: summary.bundle_diagnostic_reason,
    provenance_trust_level: summary.provenance_trust_level,
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
