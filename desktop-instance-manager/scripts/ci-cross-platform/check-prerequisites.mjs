#!/usr/bin/env node
/**
 * Cross-platform build & packaging prerequisite checker for Multi-Zalo Desktop
 * (desktop-instance-manager).
 *
 * Strictly read-only: NEVER modifies OS policy, Smart App Control, Cargo.lock,
 * or system packages, and NEVER prompts for signing credentials.
 *
 * CLI:
 *   node check-prerequisites.mjs --target <triple> [--require-bundler] [--require-compile] [--out <json-path>]
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

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

export function buildAugmentedEnv(baseEnv = process.env) {
  const extraDirs = [];
  if (baseEnv.CARGO_HOME) {
    extraDirs.push(path.join(baseEnv.CARGO_HOME, 'bin'));
  }
  const home = os.homedir();
  if (home) {
    extraDirs.push(path.join(home, '.cargo', 'bin'));
  }
  const pathKey = Object.keys(baseEnv).find((k) => k.toLowerCase() === 'path') || 'PATH';
  const currentPath = baseEnv[pathKey] || '';
  const merged = [...extraDirs.filter((d) => fs.existsSync(d)), currentPath]
    .filter(Boolean)
    .join(path.delimiter);
  return { ...baseEnv, [pathKey]: merged };
}

function findExecutable(binName, env = buildAugmentedEnv(), extraCandidatePaths = []) {
  const isWin = process.platform === 'win32';
  const exts = isWin
    ? (path.extname(binName) ? [''] : ['.exe', '.cmd', '.bat', ''])
    : [''];

  for (const candidate of extraCandidatePaths) {
    if (candidate && fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
      return candidate;
    }
  }

  const pathKey = Object.keys(env).find((k) => k.toLowerCase() === 'path') || 'PATH';
  const dirs = (env[pathKey] || '').split(path.delimiter).filter(Boolean);
  for (const dir of dirs) {
    for (const ext of exts) {
      const full = path.join(dir, `${binName}${ext}`);
      try {
        if (fs.existsSync(full) && fs.statSync(full).isFile()) {
          return full;
        }
      } catch {
        // Ignore inaccessible PATH entries
      }
    }
  }
  return null;
}

function probeCommand(bin, args = ['--version'], opts = {}) {
  const env = opts.env || buildAugmentedEnv();
  const resolved = findExecutable(bin, env, opts.extraPaths || []);
  const cmdToRun = resolved || bin;
  const useShell = process.platform === 'win32' && /\.(cmd|bat)$/i.test(cmdToRun);
  const res = spawnSync(cmdToRun, args, {
    cwd: opts.cwd || WORKSPACE_ROOT,
    env,
    encoding: 'utf8',
    timeout: opts.timeoutMs || 15000,
    shell: useShell,
    windowsHide: true,
  });
  return {
    available: !res.error && res.status === 0,
    exit_code: res.status,
    resolved_path: resolved,
    stdout: (res.stdout || '').trim(),
    stderr: (res.stderr || '').trim(),
    error: res.error ? res.error.message : null,
  };
}

function compareSemver(a, b) {
  const pa = String(a).split('.').map((n) => Number.parseInt(n, 10) || 0);
  const pb = String(b).split('.').map((n) => Number.parseInt(n, 10) || 0);
  for (let i = 0; i < 3; i += 1) {
    if ((pa[i] || 0) !== (pb[i] || 0)) {
      return (pa[i] || 0) > (pb[i] || 0) ? 1 : -1;
    }
  }
  return 0;
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

function parseArgs(argv) {
  const opts = {
    target: null,
    requireBundler: false,
    requireCompile: false,
    out: null,
    help: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') {
      opts.help = true;
    } else if (arg === '--require-bundler') {
      opts.requireBundler = true;
    } else if (arg === '--require-compile' || arg === '--strict') {
      opts.requireCompile = true;
    } else if (arg === '--target' && i + 1 < argv.length) {
      opts.target = argv[++i];
    } else if (arg.startsWith('--target=')) {
      opts.target = arg.slice('--target='.length);
    } else if (arg === '--out' && i + 1 < argv.length) {
      opts.out = argv[++i];
    } else if (arg.startsWith('--out=')) {
      opts.out = arg.slice('--out='.length);
    } else {
      throw new Error(`Unknown or incomplete CLI option: ${arg}`);
    }
  }
  if (!opts.target) {
    opts.target = defaultHostTriple();
  }
  return opts;
}

function readCargoMsrv(cargoTomlPath) {
  if (!fs.existsSync(cargoTomlPath)) return null;
  const text = fs.readFileSync(cargoTomlPath, 'utf8');
  const m = text.match(/^\s*rust-version\s*=\s*"([^"]+)"/m);
  return m ? m[1] : null;
}

function inspectIconsAndTauriConf() {
  const confPath = path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'tauri.conf.json');
  const iconsDir = path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'icons');
  if (!fs.existsSync(confPath)) {
    return { tauri_conf_exists: false, error: 'tauri.conf.json not found' };
  }
  const conf = JSON.parse(fs.readFileSync(confPath, 'utf8'));
  const configuredIcons = Array.isArray(conf?.bundle?.icon) ? conf.bundle.icon : [];
  const existingFiles = fs.existsSync(iconsDir)
    ? fs.readdirSync(iconsDir).sort()
    : [];
  const missingConfigured = configuredIcons.filter(
    (rel) => !fs.existsSync(path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', rel)),
  );
  const hasIco = configuredIcons.some((i) => i.endsWith('.ico')) && fs.existsSync(path.join(iconsDir, 'icon.ico'));
  const hasPng = configuredIcons.some((i) => i.endsWith('.png')) && fs.existsSync(path.join(iconsDir, '128x128.png'));
  const hasIcnsInConfig = configuredIcons.some((i) => i.endsWith('.icns'));
  const hasIcnsOnDisk = existingFiles.some((f) => f.endsWith('.icns'));

  return {
    tauri_conf_exists: true,
    product_name: conf.productName || null,
    version: conf.version || null,
    identifier: conf.identifier || null,
    bundle_active: Boolean(conf?.bundle?.active),
    bundle_targets: conf?.bundle?.targets || null,
    configured_icons: configuredIcons,
    icons_on_disk: existingFiles,
    missing_configured_icons: missingConfigured,
    has_ico: hasIco,
    has_png: hasPng,
    has_icns: hasIcnsInConfig && hasIcnsOnDisk,
    macos_icns_warning: (!hasIcnsInConfig || !hasIcnsOnDisk)
      ? 'IR-002: Missing icons/icon.icns in tauri.conf.json bundle.icon and apps/desktop/src-tauri/icons/; macOS .app/.dmg bundling requires a .icns asset.'
      : null,
  };
}

function checkWindowsPrerequisites(env, target) {
  if (process.platform !== 'win32') {
    return {
      applicable_on_host: false,
      note: `Host OS is ${process.platform}; Windows registry/MSVC/WiX/NSIS checks only execute natively on win32.`,
    };
  }

  // 1. MSVC linker / VS Build Tools check (read-only)
  const programFilesX86 = process.env['ProgramFiles(x86)'] || 'C:\\Program Files (x86)';
  const programFiles = process.env.ProgramFiles || 'C:\\Program Files';
  const vswherePath = path.join(programFilesX86, 'Microsoft Visual Studio', 'Installer', 'vswhere.exe');
  let vsInstallPath = null;
  if (fs.existsSync(vswherePath)) {
    const vsw = probeCommand(
      vswherePath,
      ['-latest', '-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath'],
      { env },
    );
    if (vsw.available && vsw.stdout) {
      vsInstallPath = vsw.stdout.split(/\r?\n/)[0].trim();
    }
  }
  const clOnPath = findExecutable('cl', env);
  const linkOnPath = findExecutable('link', env);
  const windowsSdkLib = path.join(programFilesX86, 'Windows Kits', '10', 'Lib');
  const hasWindowsSdk = fs.existsSync(windowsSdkLib);
  const msvcAvailable = Boolean(vsInstallPath || clOnPath);

  // 2. WebView2 Evergreen Runtime check (strictly read-only reg.exe query; never touches SAC)
  const webview2Guid = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  const regKeys = [
    `HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\${webview2Guid}`,
    `HKLM\\SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\${webview2Guid}`,
    `HKCU\\Software\\Microsoft\\EdgeUpdate\\Clients\\${webview2Guid}`,
  ];
  let webview2Version = null;
  let webview2RegSource = null;
  for (const key of regKeys) {
    const regProbe = probeCommand('reg', ['query', key, '/v', 'pv'], { env });
    if (regProbe.available && regProbe.stdout) {
      const m = regProbe.stdout.match(/\bpv\s+REG_SZ\s+([^\r\n]+)/i);
      if (m && m[1].trim() && m[1].trim() !== '0.0.0.0') {
        webview2Version = m[1].trim();
        webview2RegSource = key;
        break;
      }
    }
  }
  const webview2Dir = path.join(programFilesX86, 'Microsoft', 'EdgeWebView', 'Application');
  const webview2DirExists = fs.existsSync(webview2Dir);

  // 3. WiX (v3 candle/light or v4 wix) and NSIS (makensis) bundler checks
  const wixBinDirs = [
    process.env.WIX ? path.join(process.env.WIX, 'bin') : null,
    path.join(programFilesX86, 'WiX Toolset v3.14', 'bin'),
    path.join(programFilesX86, 'WiX Toolset v3.11', 'bin'),
  ].filter(Boolean);
  const candlePath = findExecutable(
    'candle',
    env,
    wixBinDirs.map((d) => path.join(d, 'candle.exe')),
  );
  const lightPath = findExecutable(
    'light',
    env,
    wixBinDirs.map((d) => path.join(d, 'light.exe')),
  );
  const wixV4Path = findExecutable('wix', env);
  const makensisPath = findExecutable('makensis', env, [
    path.join(programFilesX86, 'NSIS', 'makensis.exe'),
    path.join(programFiles, 'NSIS', 'makensis.exe'),
  ]);

  const hasSignCertEnv = Boolean(
    process.env.WINDOWS_CERTIFICATE ||
    process.env.AZURE_TRUSTED_SIGNING_ENDPOINT ||
    process.env.SIGNTOOL_THUMBPRINT,
  );

  return {
    applicable_on_host: true,
    requested_target: target,
    msvc_toolchain: {
      available: msvcAvailable,
      vswhere_exists: fs.existsSync(vswherePath),
      vs_installation_path: vsInstallPath,
      cl_exe_on_path: clOnPath,
      link_exe_on_path: linkOnPath,
      windows_10_11_sdk_present: hasWindowsSdk,
    },
    webview2_evergreen_runtime: {
      installed: Boolean(webview2Version || webview2DirExists),
      version: webview2Version,
      registry_key: webview2RegSource,
      application_dir_exists: webview2DirExists,
    },
    bundlers: {
      wix_msi: {
        available: Boolean((candlePath && lightPath) || wixV4Path),
        candle_path: candlePath,
        light_path: lightPath,
        wix_cli_path: wixV4Path,
      },
      nsis_setup: {
        available: Boolean(makensisPath),
        makensis_path: makensisPath,
      },
    },
    signing_posture: {
      signing_credentials_configured: hasSignCertEnv,
      expected_artifact_signing_status: hasSignCertEnv
        ? 'CREDENTIALS_PRESENT_VERIFY_SIGNATURE'
        : 'UNSIGNED_DEVELOPMENT_ARTIFACT',
      smart_app_control_policy: 'READ_ONLY_RESPECTED (never disabled or bypassed)',
    },
  };
}

function checkMacosPrerequisites(env, target, installedTargets) {
  if (process.platform !== 'darwin') {
    return {
      applicable_on_host: false,
      note: `Host OS is ${process.platform}; Xcode/codesign/hdiutil checks execute natively on darwin.`,
      rustup_darwin_targets_installed: {
        'aarch64-apple-darwin': installedTargets.includes('aarch64-apple-darwin'),
        'x86_64-apple-darwin': installedTargets.includes('x86_64-apple-darwin'),
      },
      expected_artifact_signing_status: 'UNSIGNED_DEVELOPMENT_ARTIFACT',
    };
  }

  const xcrun = probeCommand('xcrun', ['--version'], { env });
  const clang = probeCommand('clang', ['--version'], { env });
  const hdiutilPath = findExecutable('hdiutil', env, ['/usr/bin/hdiutil']);
  const codesignPath = findExecutable('codesign', env, ['/usr/bin/codesign']);
  const lipoPath = findExecutable('lipo', env, ['/usr/bin/lipo']);
  const swVers = probeCommand('sw_vers', [], { env });

  // Passive check ONLY: never prompt keychain or request Apple credentials
  const hasAppleSigningEnv = Boolean(
    process.env.APPLE_SIGNING_IDENTITY &&
    process.env.APPLE_SIGNING_IDENTITY.trim() !== '-' &&
    process.env.APPLE_SIGNING_IDENTITY.trim() !== '',
  );
  const hasNotarizationEnv = Boolean(
    (process.env.APPLE_ID && process.env.APPLE_PASSWORD && process.env.APPLE_TEAM_ID) ||
    (process.env.APPLE_API_KEY && process.env.APPLE_API_ISSUER),
  );

  return {
    applicable_on_host: true,
    requested_target: target,
    macos_version: swVers.available ? swVers.stdout : null,
    tools: {
      xcrun: { available: xcrun.available, version: xcrun.stdout.split(/\r?\n/)[0] || null },
      clang: { available: clang.available, version: clang.stdout.split(/\r?\n/)[0] || null },
      hdiutil: { available: Boolean(hdiutilPath), path: hdiutilPath },
      codesign: { available: Boolean(codesignPath), path: codesignPath },
      lipo: { available: Boolean(lipoPath), path: lipoPath },
    },
    rust_targets: {
      'aarch64-apple-darwin': installedTargets.includes('aarch64-apple-darwin'),
      'x86_64-apple-darwin': installedTargets.includes('x86_64-apple-darwin'),
    },
    signing_posture: {
      apple_signing_identity_configured: hasAppleSigningEnv,
      notarization_credentials_configured: hasNotarizationEnv,
      expected_artifact_signing_status: hasAppleSigningEnv
        ? 'DEVELOPER_ID_CONFIGURED_VERIFY_CODESIGN'
        : 'UNSIGNED_DEVELOPMENT_ARTIFACT',
      expected_notarization_status: hasNotarizationEnv ? 'CONFIGURED' : 'NOT_NOTARIZED',
    },
  };
}

function parseEtcOsRelease() {
  const p = '/etc/os-release';
  if (!fs.existsSync(p)) return null;
  const fields = {};
  const lines = fs.readFileSync(p, 'utf8').split(/\r?\n/);
  for (const line of lines) {
    const m = line.match(/^([A-Z0-9_]+)=(.*)$/);
    if (m) {
      let val = m[2].trim();
      if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
        val = val.slice(1, -1);
      }
      fields[m[1]] = val;
    }
  }
  return {
    id: fields.ID || null,
    version_id: fields.VERSION_ID || null,
    pretty_name: fields.PRETTY_NAME || null,
    version_codename: fields.VERSION_CODENAME || null,
    is_ubuntu_2404: fields.ID === 'ubuntu' && fields.VERSION_ID === '24.04',
  };
}

function checkLinuxPrerequisites(env, target) {
  if (process.platform !== 'linux') {
    return {
      applicable_on_host: false,
      note: `Host OS is ${process.platform}; Ubuntu /etc/os-release, pkg-config, dpkg-deb, and FUSE checks execute natively on linux.`,
    };
  }

  const osRelease = parseEtcOsRelease();
  const pkgConfig = probeCommand('pkg-config', ['--version'], { env });

  const checkPkg = (pkg) => {
    const res = probeCommand('pkg-config', ['--modversion', pkg], { env });
    return { available: res.available, version: res.available ? res.stdout : null };
  };

  const webkit2gtk = checkPkg('webkit2gtk-4.1');
  const gtk3 = checkPkg('gtk+-3.0');
  const ayatana = checkPkg('ayatana-appindicator3-0.1');
  const legacyAppIndicator = checkPkg('appindicator3-0.1');
  const librsvg = checkPkg('librsvg-2.0');

  const patchelf = probeCommand('patchelf', ['--version'], { env });
  const dpkgDeb = probeCommand('dpkg-deb', ['--version'], { env });
  const ldd = probeCommand('ldd', ['--version'], { env });
  let glibcVersion = null;
  if (ldd.stdout) {
    const firstLine = ldd.stdout.split(/\r?\n/)[0] || '';
    const m = firstLine.match(/(\d+\.\d+)\s*$/);
    if (m) glibcVersion = m[1];
  }

  const devFuseExists = fs.existsSync('/dev/fuse');
  const fusermountPath = findExecutable('fusermount3', env) || findExecutable('fusermount', env);
  const libfuse2Candidates = [
    '/usr/lib/x86_64-linux-gnu/libfuse.so.2',
    '/lib/x86_64-linux-gnu/libfuse.so.2',
    '/usr/lib/aarch64-linux-gnu/libfuse.so.2',
  ];
  const libfuse2Present = libfuse2Candidates.some((p) => fs.existsSync(p));

  return {
    applicable_on_host: true,
    requested_target: target,
    os_release: osRelease,
    pkg_config: {
      available: pkgConfig.available,
      version: pkgConfig.available ? pkgConfig.stdout : null,
      packages: {
        'webkit2gtk-4.1': webkit2gtk,
        'gtk+-3.0': gtk3,
        'ayatana-appindicator3-0.1': ayatana,
        'appindicator3-0.1': legacyAppIndicator,
        'librsvg-2.0': librsvg,
      },
      tray_indicator_satisfied: ayatana.available || legacyAppIndicator.available,
    },
    tools: {
      patchelf: { available: patchelf.available, version: patchelf.stdout.split(/\r?\n/)[0] || null },
      dpkg_deb: { available: dpkgDeb.available, version: dpkgDeb.stdout.split(/\r?\n/)[0] || null },
      ldd: { available: ldd.available, glibc_version: glibcVersion },
    },
    appimage_fuse_readiness: {
      dev_fuse_exists: devFuseExists,
      fusermount_path: fusermountPath,
      libfuse2_present: libfuse2Present,
      ci_extract_and_run_supported: true,
      note: (!devFuseExists || !libfuse2Present)
        ? 'FUSE device or libfuse.so.2 absent; AppImage tooling requires APPIMAGE_EXTRACT_AND_RUN=1 in container/CI environments.'
        : 'FUSE device and libfuse.so.2 detected.',
    },
    signing_posture: {
      expected_artifact_signing_status: 'UNSIGNED_DEVELOPMENT_ARTIFACT',
    },
  };
}

export function runPrerequisitesCheck(options = {}) {
  const target = options.target || defaultHostTriple();
  const env = buildAugmentedEnv();

  const rustc = probeCommand('rustc', ['-Vv'], { env });
  const cargo = probeCommand('cargo', ['--version'], { env });
  const rustupTargets = probeCommand('rustup', ['target', 'list', '--installed'], { env });
  const rustupToolchains = probeCommand('rustup', ['toolchain', 'list'], { env });

  let rustcRelease = null;
  let rustcHost = null;
  if (rustc.available) {
    const relMatch = rustc.stdout.match(/^release:\s*(\S+)/m);
    const hostMatch = rustc.stdout.match(/^host:\s*(\S+)/m);
    rustcRelease = relMatch ? relMatch[1] : null;
    rustcHost = hostMatch ? hostMatch[1] : null;
  }

  const installedTargets = rustupTargets.available
    ? rustupTargets.stdout.split(/\r?\n/).map((s) => s.trim()).filter(Boolean)
    : (rustcHost ? [rustcHost] : []);
  const installedToolchains = rustupToolchains.available
    ? rustupToolchains.stdout.split(/\r?\n/).map((s) => s.trim()).filter(Boolean)
    : [];

  const targetInstalled = installedTargets.includes(target) || rustcHost === target;

  // Check Tauri CLI (cargo tauri --version, cargo-tauri --version, or local node_modules/.bin/tauri)
  let tauriCli = { available: false, invocation: null, version: null };
  const cargoTauriProbe = cargo.available
    ? probeCommand('cargo', ['tauri', '--version'], { env })
    : { available: false };
  if (cargoTauriProbe.available) {
    tauriCli = {
      available: true,
      invocation: 'cargo tauri',
      version: cargoTauriProbe.stdout.split(/\r?\n/)[0] || null,
    };
  } else {
    const localTauriBin = findExecutable('tauri', env, [
      path.join(WORKSPACE_ROOT, 'node_modules', '.bin', process.platform === 'win32' ? 'tauri.cmd' : 'tauri'),
      path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'node_modules', '.bin', process.platform === 'win32' ? 'tauri.cmd' : 'tauri'),
    ]);
    if (localTauriBin) {
      const localProbe = probeCommand(localTauriBin, ['--version'], { env });
      if (localProbe.available) {
        tauriCli = {
          available: true,
          invocation: localTauriBin,
          version: localProbe.stdout.split(/\r?\n/)[0] || null,
        };
      }
    }
  }

  const workspaceMsrv = readCargoMsrv(path.join(WORKSPACE_ROOT, 'Cargo.toml'));
  const tauriMsrv = readCargoMsrv(path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri', 'Cargo.toml'));
  const effectiveMsrv = tauriMsrv || workspaceMsrv || '1.90';
  const msrvSatisfied = rustcRelease ? compareSemver(rustcRelease, effectiveMsrv) >= 0 : false;

  const icons = inspectIconsAndTauriConf();
  const windowsChecks = checkWindowsPrerequisites(env, target);
  const macosChecks = checkMacosPrerequisites(env, target, installedTargets);
  const linuxChecks = checkLinuxPrerequisites(env, target);

  const blockingCompileIssues = [];
  const blockingBundlerIssues = [];
  const warnings = [];

  if (!rustc.available) blockingCompileIssues.push('rustc is not available on PATH or ~/.cargo/bin');
  if (!cargo.available) blockingCompileIssues.push('cargo is not available on PATH or ~/.cargo/bin');
  if (rustc.available && !msrvSatisfied) {
    blockingCompileIssues.push(`rustc ${rustcRelease} is older than effective MSRV ${effectiveMsrv}`);
  }
  if (rustc.available && !targetInstalled) {
    blockingCompileIssues.push(
      `Rust target ${target} is not installed on active toolchain (host=${rustcHost}; installed=${installedTargets.join(', ')})`,
    );
  }
  if (workspaceMsrv && tauriMsrv && workspaceMsrv !== tauriMsrv) {
    warnings.push(
      `IR-001: MSRV mismatch between desktop-instance-manager/Cargo.toml (${workspaceMsrv}) and apps/desktop/src-tauri/Cargo.toml (${tauriMsrv}).`,
    );
  }
  if (icons.macos_icns_warning) {
    warnings.push(icons.macos_icns_warning);
  }

  if (!tauriCli.available) {
    blockingBundlerIssues.push('Tauri CLI (cargo-tauri / @tauri-apps/cli) is not installed');
  }

  if (target.includes('-windows-')) {
    if (process.platform !== 'win32') {
      blockingCompileIssues.push(`Target ${target} requires a Windows build runner (current host: ${process.platform})`);
    } else {
      if (target.endsWith('-msvc') && !windowsChecks.msvc_toolchain.available) {
        blockingCompileIssues.push('MSVC C++ Build Tools (cl.exe / link.exe) not detected for x86_64-pc-windows-msvc');
      }
      if (!windowsChecks.bundlers.wix_msi.available && !windowsChecks.bundlers.nsis_setup.available) {
        blockingBundlerIssues.push('Neither WiX (candle/light/wix) nor NSIS (makensis) is installed on this Windows host');
      } else {
        if (!windowsChecks.bundlers.wix_msi.available) {
          warnings.push('WiX Toolset not detected (.msi bundling unavailable unless installed)');
        }
        if (!windowsChecks.bundlers.nsis_setup.available) {
          warnings.push('NSIS makensis not detected (-setup.exe NSIS bundling unavailable unless installed)');
        }
      }
    }
  } else if (target.includes('-apple-darwin')) {
    if (process.platform !== 'darwin') {
      blockingCompileIssues.push(`Target ${target} requires a macOS Xcode runner (current host: ${process.platform})`);
    } else {
      if (!macosChecks.tools.xcrun.available || !macosChecks.tools.clang.available) {
        blockingCompileIssues.push('Xcode Command Line Tools (xcrun / clang) missing on macOS host');
      }
      if (!macosChecks.tools.hdiutil.available) {
        blockingBundlerIssues.push('hdiutil missing on macOS host (.dmg creation unavailable)');
      }
      if (!icons.has_icns) {
        blockingBundlerIssues.push('icons/icon.icns missing from tauri.conf.json and icons/ (IR-002)');
      }
    }
  } else if (target.includes('-unknown-linux-')) {
    if (process.platform !== 'linux') {
      blockingCompileIssues.push(`Target ${target} requires an Ubuntu/Linux build host (current host: ${process.platform})`);
    } else {
      const pkgs = linuxChecks.pkg_config.packages;
      if (!pkgs['webkit2gtk-4.1'].available) {
        blockingCompileIssues.push('pkg-config webkit2gtk-4.1 missing (install libwebkit2gtk-4.1-dev)');
      }
      if (!pkgs['gtk+-3.0'].available) {
        blockingCompileIssues.push('pkg-config gtk+-3.0 missing');
      }
      if (!linuxChecks.pkg_config.tray_indicator_satisfied) {
        blockingCompileIssues.push('Neither ayatana-appindicator3-0.1 nor appindicator3-0.1 found via pkg-config');
      }
      if (!pkgs['librsvg-2.0'].available) {
        blockingCompileIssues.push('pkg-config librsvg-2.0 missing (install librsvg2-dev)');
      }
      if (!linuxChecks.tools.dpkg_deb.available) {
        blockingBundlerIssues.push('dpkg-deb missing (.deb packaging unavailable)');
      }
      if (!linuxChecks.tools.patchelf.available) {
        blockingBundlerIssues.push('patchelf missing (AppImage bundling unavailable)');
      }
      if (linuxChecks.os_release && !linuxChecks.os_release.is_ubuntu_2404) {
        warnings.push(
          `Linux host is ${linuxChecks.os_release.pretty_name || linuxChecks.os_release.id}, not primary Ubuntu 24.04 LTS baseline.`,
        );
      }
    }
  }

  const compileReady = blockingCompileIssues.length === 0;
  const bundlerReady = compileReady && blockingBundlerIssues.length === 0;

  let status = 'READY';
  if (!compileReady) {
    status = 'MISSING_COMPILE_PREREQUISITES';
  } else if (!bundlerReady) {
    status = 'COMPILE_READY_BUNDLER_INCOMPLETE';
  }

  return {
    schema_version: 1,
    checked_at_utc: new Date().toISOString(),
    host: {
      platform: process.platform,
      arch: process.arch,
      release: os.release(),
      node_version: process.version,
    },
    requested_target: target,
    status,
    compile_ready: compileReady,
    bundler_ready: bundlerReady,
    toolchain: {
      rustc_available: rustc.available,
      rustc_path: rustc.resolved_path,
      rustc_release: rustcRelease,
      rustc_host: rustcHost,
      rustc_vv: rustc.available ? rustc.stdout : null,
      cargo_available: cargo.available,
      cargo_path: cargo.resolved_path,
      cargo_version: cargo.available ? cargo.stdout : null,
      installed_toolchains: installedToolchains,
      installed_targets: installedTargets,
      requested_target_installed: targetInstalled,
      workspace_msrv: workspaceMsrv,
      tauri_crate_msrv: tauriMsrv,
      effective_msrv: effectiveMsrv,
      msrv_satisfied: msrvSatisfied,
      tauri_cli: tauriCli,
    },
    icons,
    os_checks: {
      windows: windowsChecks,
      macos: macosChecks,
      linux: linuxChecks,
    },
    blocking_compile_issues: blockingCompileIssues,
    blocking_bundler_issues: blockingBundlerIssues,
    warnings,
  };
}

function cliMain() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    console.log(
      'Usage: node check-prerequisites.mjs --target <triple> [--require-bundler] [--require-compile] [--out <json-path>]',
    );
    process.exit(0);
  }

  const report = runPrerequisitesCheck(opts);

  if (opts.out) {
    const outAbs = path.resolve(opts.out);
    fs.mkdirSync(path.dirname(outAbs), { recursive: true });
    fs.writeFileSync(outAbs, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
  }

  console.log(JSON.stringify({
    requested_target: report.requested_target,
    status: report.status,
    compile_ready: report.compile_ready,
    bundler_ready: report.bundler_ready,
    rustc_release: report.toolchain.rustc_release,
    rustc_host: report.toolchain.rustc_host,
    tauri_cli_available: report.toolchain.tauri_cli.available,
    blocking_compile_issues: report.blocking_compile_issues,
    blocking_bundler_issues: report.blocking_bundler_issues,
    warnings_count: report.warnings.length,
    out_file: opts.out ? path.resolve(opts.out) : null,
  }, null, 2));

  if (!report.toolchain.rustc_available || !report.toolchain.cargo_available) {
    process.exit(1);
  }
  if (opts.requireBundler && !report.bundler_ready) {
    process.exit(1);
  }
  if (opts.requireCompile && !report.compile_ready) {
    process.exit(1);
  }
}

if (isMainModule(import.meta.url)) {
  try {
    cliMain();
  } catch (err) {
    console.error(`[check-prerequisites] ERROR: ${err.message}`);
    process.exit(1);
  }
}
