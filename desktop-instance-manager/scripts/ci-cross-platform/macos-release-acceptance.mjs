#!/usr/bin/env node
/**
 * macOS Release Acceptance & Bundle Diagnostics for NexaDock (.dmg / .app).
 *
 * Separates verification levels honestly:
 *   - Build verified
 *   - Bundle verified (hdiutil, Info.plist, Mach-O arch, otool -L)
 *   - Signature / Gatekeeper verified (codesign --verify --deep --strict, TeamID, Hardened Runtime, stapler, spctl with quarantine)
 *   - Automated launch verified (clean-install copy, 5s survival, SIGTERM, relaunch)
 *   - Interactive GUI / Physical Mac / Official Zalo integration (explicitly NOT_TESTED on headless runners)
 *
 * Modes:
 *   --mode production    Fail-closed: requires Developer ID, Hardened Runtime, Stapler, and Gatekeeper PASS.
 *   --mode dev-unsigned  CI PR mode without Apple secrets: verifies bundle, arch, deps, and launch,
 *                        while still recording exact codesign/spctl/stapler failures honestly.
 */

import { spawn, spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

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

export function parseCodesignMetadata(stderrAndStdout) {
  const text = String(stderrAndStdout || '');
  const identifierMatch = text.match(/^Identifier=(.+)$/m);
  const teamMatch = text.match(/^TeamIdentifier=(.+)$/m);
  const signatureMatch = text.match(/^Signature=(.+)$/m);
  const flagsMatch = text.match(/^CodeDirectory .*?flags=0x[0-9a-fA-F]+\(([^)]+)\)/m);
  const authorities = [...text.matchAll(/^Authority=(.+)$/gm)].map((m) => m[1].trim());

  const teamIdentifier = teamMatch ? teamMatch[1].trim() : null;
  const isAdhoc = signatureMatch ? signatureMatch[1].trim().toLowerCase() === 'adhoc' : false;
  const flagsRaw = flagsMatch ? flagsMatch[1].toLowerCase() : '';
  const hasHardenedRuntime = flagsRaw.split(',').map((s) => s.trim()).includes('runtime');
  const hasDeveloperId =
    !isAdhoc &&
    Boolean(teamIdentifier && teamIdentifier !== 'not set') &&
    authorities.some((a) => a.startsWith('Developer ID Application:'));

  return {
    identifier: identifierMatch ? identifierMatch[1].trim() : null,
    team_identifier: teamIdentifier && teamIdentifier !== 'not set' ? teamIdentifier : null,
    authorities,
    is_adhoc: isAdhoc,
    has_hardened_runtime: hasHardenedRuntime,
    has_developer_id: hasDeveloperId,
  };
}

export function evaluateArchitectureSlices(lipoOutput, expectedArch) {
  const slices = String(lipoOutput || '')
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  const hasExpected = slices.includes(expectedArch);
  return {
    slices,
    expected_arch: expectedArch,
    ok: hasExpected,
    reason: hasExpected
      ? null
      : `Expected Mach-O slice '${expectedArch}' not found in [${slices.join(', ')}]`,
  };
}

export function evaluateOtoolDependencies(otoolOutput) {
  const lines = String(otoolOutput || '')
    .split(/\r?\n/)
    .slice(1)
    .map((l) => l.trim())
    .filter(Boolean);
  const nonSystem = [];
  for (const line of lines) {
    const depPath = line.split(/\s+\(compatibility/)[0].trim();
    if (
      !depPath.startsWith('/System/Library/') &&
      !depPath.startsWith('/usr/lib/') &&
      !depPath.startsWith('@executable_path/') &&
      !depPath.startsWith('@loader_path/')
    ) {
      nonSystem.push(depPath);
    }
  }
  return {
    dependency_count: lines.length,
    non_system_dependencies: nonSystem,
    ok: nonSystem.length === 0,
  };
}

export function evaluateSigningVerdict(meta, codesignVerifyOk, spctlOk, dmgStapled, appStapled) {
  const reasons = [];
  if (!codesignVerifyOk) reasons.push('codesign --verify --deep --strict failed');
  if (meta.is_adhoc) reasons.push('bundle is ad-hoc signed (Signature=adhoc)');
  if (!meta.has_developer_id) reasons.push('missing Developer ID Application certificate');
  if (!meta.team_identifier) reasons.push('missing Apple TeamIdentifier');
  if (!meta.has_hardened_runtime) reasons.push('missing Hardened Runtime (flags=runtime)');
  if (!appStapled) reasons.push('.app notarization ticket not stapled');
  if (!dmgStapled) reasons.push('.dmg notarization ticket not stapled');
  if (!spctlOk) reasons.push('Gatekeeper (spctl --assess --type execute) rejected quarantined .app');

  return {
    passed: reasons.length === 0,
    signing_status: reasons.length === 0 ? 'SIGNED_AND_NOTARIZED' : 'UNSIGNED_DEVELOPMENT_ARTIFACT',
    reasons,
  };
}

function runCmd(bin, args, timeoutMs = 30000) {
  const res = spawnSync(bin, args, {
    encoding: 'utf8',
    timeout: timeoutMs,
  });
  return {
    ok: !res.error && res.status === 0,
    exit_code: res.status,
    stdout: (res.stdout || '').trim(),
    stderr: (res.stderr || '').trim(),
    combined: `${res.stdout || ''}\n${res.stderr || ''}`.trim(),
    error: res.error ? res.error.message : null,
  };
}

function computeSha256(filePath) {
  const hash = crypto.createHash('sha256');
  const fd = fs.openSync(filePath, 'r');
  try {
    const buf = Buffer.allocUnsafe(1024 * 1024);
    let n = 0;
    while ((n = fs.readSync(fd, buf, 0, buf.length, null)) > 0) {
      hash.update(buf.subarray(0, n));
    }
  } finally {
    fs.closeSync(fd);
  }
  return hash.digest('hex');
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function testCleanLaunch(executablePath, timeoutMs = 5000) {
  const isolatedHome = fs.mkdtempSync(path.join(os.tmpdir(), 'nexadock-launch-home-'));
  const isolatedTmp = path.join(isolatedHome, 'tmp');
  fs.mkdirSync(isolatedTmp, { recursive: true });

  const runOnce = async (holdMs) => {
    const child = spawn(executablePath, [], {
      env: {
        ...process.env,
        HOME: isolatedHome,
        TMPDIR: isolatedTmp,
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout?.on('data', (d) => { stdout += d.toString(); });
    child.stderr?.on('data', (d) => { stderr += d.toString(); });

    let exitedEarly = false;
    let exitCode = null;
    let exitSignal = null;
    child.on('exit', (code, signal) => {
      exitedEarly = true;
      exitCode = code;
      exitSignal = signal;
    });

    await sleep(holdMs);

    const aliveAfterHold = !exitedEarly;
    if (aliveAfterHold) {
      child.kill('SIGTERM');
      const deadline = Date.now() + 5000;
      while (!exitedEarly && Date.now() < deadline) {
        await sleep(100);
      }
      if (!exitedEarly) {
        child.kill('SIGKILL');
        await sleep(200);
      }
    }

    const hasPanic = /panicked at|fatal error|RUST_BACKTRACE/i.test(stderr + stdout);
    return {
      alive_after_hold: aliveAfterHold,
      clean_exit: exitCode === 0 || exitSignal === 'SIGTERM',
      exit_code: exitCode,
      exit_signal: exitSignal,
      has_panic: hasPanic,
      stdout: stdout.slice(0, 2000),
      stderr: stderr.slice(0, 2000),
    };
  };

  try {
    const first = await runOnce(timeoutMs);
    const relaunch = await runOnce(3000);
    return {
      ok: first.alive_after_hold && !first.has_panic && relaunch.alive_after_hold && !relaunch.has_panic,
      first_launch: first,
      relaunch,
    };
  } finally {
    fs.rmSync(isolatedHome, { recursive: true, force: true });
  }
}

function parseArgs(argv) {
  const args = {
    dmg: null,
    arch: 'arm64',
    mode: 'production',
    expectedVersion: null,
    expectedBundleId: 'com.multizalodesktop.manager',
    expectedProduct: 'NexaDock',
    skipLaunch: false,
    out: null,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const k = argv[i];
    if (k === '--dmg') args.dmg = argv[++i];
    else if (k === '--arch') args.arch = argv[++i];
    else if (k === '--mode') args.mode = argv[++i];
    else if (k === '--expected-version') args.expectedVersion = argv[++i];
    else if (k === '--expected-bundle-id') args.expectedBundleId = argv[++i];
    else if (k === '--expected-product') args.expectedProduct = argv[++i];
    else if (k === '--skip-launch') args.skipLaunch = true;
    else if (k === '--out') args.out = argv[++i];
  }
  return args;
}

async function main() {
  if (process.platform !== 'darwin') {
    console.error('macos-release-acceptance.mjs must be run on macOS (darwin)');
    process.exit(2);
  }
  const opts = parseArgs(process.argv.slice(2));
  if (!opts.dmg || !fs.existsSync(opts.dmg)) {
    console.error(`Missing DMG file: ${opts.dmg}`);
    process.exit(2);
  }
  if (!['production', 'dev-unsigned'].includes(opts.mode)) {
    console.error(`Invalid --mode: ${opts.mode}`);
    process.exit(2);
  }

  const dmgPath = path.resolve(opts.dmg);
  const sha256 = computeSha256(dmgPath);
  const dmgSize = fs.statSync(dmgPath).size;
  const hdiutilVerify = runCmd('hdiutil', ['verify', dmgPath], 60000);

  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nexadock-accept-'));
  const mountPoint = path.join(workDir, 'mnt');
  const installDir = path.join(workDir, 'Applications');
  fs.mkdirSync(mountPoint, { recursive: true });
  fs.mkdirSync(installDir, { recursive: true });

  let mounted = false;
  try {
    const attach = runCmd(
      'hdiutil',
      ['attach', dmgPath, '-nobrowse', '-readonly', '-mountpoint', mountPoint],
      60000,
    );
    if (!attach.ok) {
      throw new Error(`hdiutil attach failed: ${attach.combined}`);
    }
    mounted = true;

    const appEntries = fs.readdirSync(mountPoint).filter((e) => e.endsWith('.app'));
    if (appEntries.length === 0) {
      throw new Error(`No .app bundle found inside mounted DMG at ${mountPoint}`);
    }
    const appName = appEntries[0];
    const mountedApp = path.join(mountPoint, appName);
    const installedApp = path.join(installDir, appName);

    const ditto = runCmd('ditto', [mountedApp, installedApp], 60000);
    if (!ditto.ok) {
      throw new Error(`ditto copy to clean install dir failed: ${ditto.combined}`);
    }

    runCmd('hdiutil', ['detach', mountPoint, '-force'], 30000);
    mounted = false;

    const plistPath = path.join(installedApp, 'Contents', 'Info.plist');
    const plistLint = runCmd('plutil', ['-lint', plistPath]);
    const plistJsonRes = runCmd('plutil', ['-convert', 'json', '-o', '-', plistPath]);
    const plist = plistJsonRes.ok ? JSON.parse(plistJsonRes.stdout) : {};

    const execName = plist.CFBundleExecutable || opts.expectedProduct;
    const execPath = path.join(installedApp, 'Contents', 'MacOS', execName);
    const icnsPath = path.join(installedApp, 'Contents', 'Resources', 'icon.icns');
    const bundleStructureOk =
      fs.existsSync(plistPath) &&
      fs.existsSync(execPath) &&
      fs.existsSync(icnsPath) &&
      plistLint.ok &&
      plist.CFBundleIdentifier === opts.expectedBundleId &&
      (!opts.expectedVersion || plist.CFBundleShortVersionString === opts.expectedVersion);

    const lipoRes = runCmd('lipo', ['-archs', execPath]);
    const fileRes = runCmd('file', [execPath]);
    const archEval = evaluateArchitectureSlices(lipoRes.stdout, opts.arch);

    const otoolRes = runCmd('otool', ['-L', execPath]);
    const otoolEval = evaluateOtoolDependencies(otoolRes.stdout);

    const codesignVerify = runCmd('codesign', ['--verify', '--deep', '--strict', '--verbose=4', installedApp]);
    const codesignDisplay = runCmd('codesign', ['-dvv', installedApp]);
    const codesignMeta = parseCodesignMetadata(codesignDisplay.combined);
    const entitlementsRes = runCmd('codesign', ['-d', '--entitlements', ':-', installedApp]);

    const dmgStaple = runCmd('xcrun', ['stapler', 'validate', dmgPath]);
    const appStaple = runCmd('xcrun', ['stapler', 'validate', installedApp]);

    // Simulate downloaded browser quarantine on the clean-installed copy (never stripping quarantine)
    const hexEpoch = Math.floor(Date.now() / 1000).toString(16);
    runCmd('xattr', ['-w', 'com.apple.quarantine', `0081;${hexEpoch};Safari;`, installedApp]);
    const spctlRes = runCmd('spctl', ['--assess', '--type', 'execute', '--verbose=4', installedApp]);

    const signingVerdict = evaluateSigningVerdict(
      codesignMeta,
      codesignVerify.ok,
      spctlRes.ok,
      dmgStaple.ok,
      appStaple.ok,
    );

    const launchResult = opts.skipLaunch
      ? { ok: null, skipped: true }
      : await testCleanLaunch(execPath);

    const bundleVerified = hdiutilVerify.ok && bundleStructureOk && archEval.ok && otoolEval.ok;
    const overallPass =
      opts.mode === 'production'
        ? bundleVerified && signingVerdict.passed && launchResult.ok === true
        : bundleVerified && (opts.skipLaunch || launchResult.ok === true);

    const report = {
      schema_version: 1,
      mode: opts.mode,
      dmg_path: dmgPath,
      dmg_size_bytes: dmgSize,
      dmg_sha256: sha256,
      hdiutil_verify_ok: hdiutilVerify.ok,
      app_name: appName,
      info_plist: {
        lint_ok: plistLint.ok,
        CFBundleIdentifier: plist.CFBundleIdentifier || null,
        CFBundleName: plist.CFBundleName || null,
        CFBundleShortVersionString: plist.CFBundleShortVersionString || null,
        CFBundleExecutable: plist.CFBundleExecutable || null,
        LSMinimumSystemVersion: plist.LSMinimumSystemVersion || null,
      },
      architecture: {
        file_output: fileRes.stdout,
        ...archEval,
      },
      dependencies: otoolEval,
      codesign: {
        verify_ok: codesignVerify.ok,
        verify_detail: codesignVerify.combined,
        ...codesignMeta,
        entitlements_raw: entitlementsRes.combined,
      },
      notarization: {
        dmg_stapler_ok: dmgStaple.ok,
        dmg_stapler_detail: dmgStaple.combined,
        app_stapler_ok: appStaple.ok,
        app_stapler_detail: appStaple.combined,
      },
      gatekeeper: {
        spctl_ok: spctlRes.ok,
        spctl_detail: spctlRes.combined,
      },
      signing_verdict: signingVerdict,
      automated_launch: launchResult,
      verification_levels: {
        build_verified: 'PASS',
        bundle_verified: bundleVerified ? 'PASS' : 'FAIL',
        signature_gatekeeper_verified: signingVerdict.passed ? 'PASS' : 'FAIL',
        automated_launch_verified: opts.skipLaunch
          ? 'NOT_TESTED'
          : launchResult.ok
            ? 'PASS'
            : 'FAIL',
        interactive_gui_verified: 'NOT_TESTED',
        physical_mac_verified: 'NOT_TESTED',
        official_zalo_integration_verified: 'NOT_TESTED',
      },
      overall_pass: overallPass,
    };

    const jsonText = JSON.stringify(report, null, 2);
    if (opts.out) {
      fs.mkdirSync(path.dirname(path.resolve(opts.out)), { recursive: true });
      fs.writeFileSync(opts.out, `${jsonText}\n`, 'utf8');
    }
    console.log(jsonText);

    if (!overallPass) {
      process.exit(1);
    }
  } finally {
    if (mounted) {
      runCmd('hdiutil', ['detach', mountPoint, '-force'], 30000);
    }
    fs.rmSync(workDir, { recursive: true, force: true });
  }
}

if (isMainModule(import.meta.url)) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
