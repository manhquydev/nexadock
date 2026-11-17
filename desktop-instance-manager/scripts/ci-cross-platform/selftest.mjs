#!/usr/bin/env node
/**
 * Comprehensive Automated Self-Test Suite for Cross-Platform Build & Provenance Tooling
 * (`desktop-instance-manager/scripts/ci-cross-platform/`).
 *
 * Verifies:
 *   1. `node --check` syntax validation across ALL `.mjs` scripts in `scripts/ci-cross-platform/`.
 *   2. Real generated `assets/icon.icns` (`icns` magic `69636e73`, `ic07` 128x128 + `ic08` 256x256 PNG),
 *      `assets/128x128@2x.png` (256x256 RGBA), and `tauri-ci-bundle-overlay.json` schema & icon resolution.
 *   3. Synthetic PE32+ (`x86_64` & `aarch64`), MSI CFB, Mach-O 64-bit (`arm64` & `x86_64`),
 *      Mach-O Universal Fat binary, `.app` `Info.plist`, `.dmg` UDIF `koly` trailer,
 *      Linux ELF64 (`x86_64`) + `GLIBC_2.xx` floor, Type-2 `.AppImage` (`AI\x02`), and `.deb` `ar` parsers.
 *   4. Honesty & 7-pillar supply-chain invariants:
 *      - Raw `.exe` / Mach-O / ELF binaries are `compiled-executable` (`is_installer === false`), NEVER installers!
 *      - Unsigned artifacts report `UNSIGNED_DEVELOPMENT_ARTIFACT` (never `signed` or `production`).
 *      - Provenance reports `SELF_REPORTED_UNATTESTED` (SBOM != provenance).
 *      - `dependency-audit.mjs` reports real `cargo-deny` / RustSec scan when present, AND strictly reports
 *        `UNVERIFIED_ADVISORY_DB_NOT_SCANNED` (`vulnerabilities_found === null`) when no advisory DB was scanned!
 *   5. `build-and-package.mjs --dry-run` enforces `--locked` on all cargo stages, pins `@tauri-apps/cli@2.12.5`,
 *      and marks `native_runtime_test_possible === false` on non-native cross targets.
 */

import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import {
  classifyArtifact,
  inspectArtifact,
  parseAppImageHeaders,
  parseDebHeaders,
  parseDmgTrailer,
  parseElfHeaders,
  parseIcnsHeaders,
  parseMachOHeaders,
  parseMsiHeaders,
  parsePeHeaders,
} from './inspect-artifact.mjs';
import { SECTION_7_REQUIRED_FIELDS } from './generate-artifact-manifest.mjs';
import { evaluateSpdxExpression } from './dependency-audit.mjs';
import { PINNED_TAURI_CLI_VERSION, PINNED_TAURI_NPM_SPEC } from './check-prerequisites.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const WORKSPACE_ROOT = path.resolve(__dirname, '..', '..');

function buildSyntheticPe64Buffer({
  machine = 0x8664,
  certOffset = 0,
  certSize = 0,
  includeManifest = true,
  includeWebView2Import = true,
} = {}) {
  const buf = Buffer.alloc(2048, 0);
  buf[0] = 0x4d;
  buf[1] = 0x5a;
  const eLfanew = 0x80;
  buf.writeUInt32LE(eLfanew, 0x3c);
  buf.write('PE\0\0', eLfanew, 'ascii');

  const coff = eLfanew + 4;
  buf.writeUInt16LE(machine, coff + 0);
  buf.writeUInt16LE(3, coff + 2);
  buf.writeUInt16LE(240, coff + 16);
  buf.writeUInt16LE(0x0022, coff + 18);

  const opt = coff + 20;
  buf.writeUInt16LE(0x020b, opt + 0);
  buf.writeUInt16LE(2, opt + 68);
  buf.writeUInt32LE(16, opt + 108);
  buf.writeUInt32LE(certOffset, opt + 144);
  buf.writeUInt32LE(certSize, opt + 148);

  let payloadOffset = 512;
  if (includeWebView2Import) {
    payloadOffset += buf.write('WebView2Loader.dll\0', payloadOffset, 'ascii');
  }
  if (includeManifest) {
    const xml = [
      '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>',
      '<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">',
      '  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">',
      '    <security><requestedPrivileges>',
      '      <requestedExecutionLevel level="asInvoker" uiAccess="false"/>',
      '    </requestedPrivileges></security>',
      '  </trustInfo>',
      '  <application xmlns="urn:schemas-microsoft-com:asm.v3">',
      '    <windowsSettings><dpiAwareness>PerMonitorV2</dpiAwareness></windowsSettings>',
      '  </application>',
      '  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">',
      '    <application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/></application>',
      '  </compatibility>',
      '</assembly>',
    ].join('\n');
    buf.write(xml, payloadOffset, 'utf8');
  }
  return buf;
}

function buildSyntheticMachO64Buffer({ cpuType = 0x0100000c, withCodeSigCmd = false } = {}) {
  const buf = Buffer.alloc(256, 0);
  buf.writeUInt32LE(0xfeedfacf, 0);
  buf.writeUInt32LE(cpuType, 4);
  buf.writeUInt32LE(0, 8);
  buf.writeUInt32LE(2, 12);
  buf.writeUInt32LE(withCodeSigCmd ? 1 : 0, 16);
  buf.writeUInt32LE(withCodeSigCmd ? 16 : 0, 20);
  if (withCodeSigCmd) {
    buf.writeUInt32LE(0x1d, 32);
    buf.writeUInt32LE(16, 36);
  }
  return buf;
}

function buildSyntheticMachOFatBuffer() {
  const buf = Buffer.alloc(128, 0);
  buf.writeUInt32BE(0xcafebabe, 0);
  buf.writeUInt32BE(2, 4);
  buf.writeUInt32BE(0x01000007, 8);
  buf.writeUInt32BE(0x0100000c, 28);
  return buf;
}

function buildSyntheticElf64Buffer({ isAppImage = false, machine = 0x3e } = {}) {
  const buf = Buffer.alloc(512, 0);
  buf[0] = 0x7f;
  buf[1] = 0x45;
  buf[2] = 0x4c;
  buf[3] = 0x46;
  buf[4] = 2;
  buf[5] = 1;
  buf[6] = 1;
  if (isAppImage) {
    buf[8] = 0x41;
    buf[9] = 0x49;
    buf[10] = 0x02;
  }
  buf.writeUInt16LE(3, 16);
  buf.writeUInt16LE(machine, 18);
  buf.write('GLIBC_2.2.5\0GLIBC_2.34\0GLIBC_2.39\0', 128, 'ascii');
  return buf;
}

function buildSyntheticDebBuffer() {
  const buf = Buffer.alloc(128, 0x20);
  buf.write('!<arch>\n', 0, 'ascii');
  buf.write('debian-binary/  ', 8, 'ascii');
  buf.write('2.0\n', 68, 'ascii');
  return buf;
}

function buildSyntheticMsiBuffer() {
  const buf = Buffer.alloc(512, 0);
  Buffer.from([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]).copy(buf, 0);
  return buf;
}

function buildSyntheticDmgBuffer() {
  const buf = Buffer.alloc(1024, 0);
  buf.write('koly', 512, 'ascii');
  return buf;
}

function sha256Hex(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function main() {
  const results = [];
  const runTest = (name, fn) => {
    fn();
    results.push({ name, status: 'PASS' });
  };

  // 1. Syntax check (`node --check`) on every `.mjs` file in `scripts/ci-cross-platform/`
  const mjsFiles = fs
    .readdirSync(__dirname)
    .filter((f) => f.endsWith('.mjs'))
    .sort();
  assert.ok(mjsFiles.length >= 6, `Expected >= 6 .mjs scripts, found ${mjsFiles.length}`);

  for (const scriptFile of mjsFiles) {
    runTest(`syntax-check:${scriptFile}`, () => {
      execFileSync(process.execPath, ['--check', path.join(__dirname, scriptFile)], { stdio: 'pipe' });
    });
  }

  // 2. Validate generated Apple `.icns`, Retina `128x128@2x.png`, and `tauri-ci-bundle-overlay.json`
  runTest('icns-retina-assets-and-tauri-bundle-overlay', () => {
    const icnsPath = path.join(__dirname, 'assets', 'icon.icns');
    const retinaPath = path.join(__dirname, 'assets', '128x128@2x.png');
    const overlayPath = path.join(__dirname, 'tauri-ci-bundle-overlay.json');

    assert.ok(fs.existsSync(icnsPath), 'Missing assets/icon.icns');
    assert.ok(fs.existsSync(retinaPath), 'Missing assets/128x128@2x.png');
    assert.ok(fs.existsSync(overlayPath), 'Missing tauri-ci-bundle-overlay.json');

    const icnsParsed = parseIcnsHeaders(fs.readFileSync(icnsPath));
    assert.equal(icnsParsed.valid_icns, true, 'icon.icns failed ICNS structural validation');
    assert.equal(icnsParsed.magic_hex, '69636e73');
    assert.equal(icnsParsed.declared_total_bytes, icnsParsed.actual_total_bytes);
    assert.equal(icnsParsed.has_ic07_128x128, true, 'icon.icns missing ic07 128x128 PNG chunk');
    assert.equal(icnsParsed.has_ic08_256x256, true, 'icon.icns missing ic08 256x256 PNG chunk');

    const icnsInspected = inspectArtifact(icnsPath);
    assert.equal(icnsInspected.artifact_kind, 'macos-icns-icon');
    assert.equal(icnsInspected.is_installer, false);
    assert.equal(icnsInspected.details.icns.valid_icns, true);

    const retinaBuf = fs.readFileSync(retinaPath);
    assert.equal(retinaBuf.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
    assert.equal(retinaBuf.readUInt32BE(16), 256);
    assert.equal(retinaBuf.readUInt32BE(20), 256);
    assert.equal(retinaBuf[24], 8);
    assert.equal(retinaBuf[25], 6); // RGBA

    const overlay = JSON.parse(fs.readFileSync(overlayPath, 'utf8'));
    assert.equal(overlay.bundle.windows.webviewInstallMode.type, 'downloadBootstrapper');
    assert.equal(overlay.bundle.macOS.minimumSystemVersion, '11.0');
    assert.ok(Array.isArray(overlay.bundle.linux.deb.depends));
    assert.ok(overlay.bundle.linux.deb.depends.includes('libwebkit2gtk-4.1-0'));

    const srcTauriDir = path.join(WORKSPACE_ROOT, 'apps', 'desktop', 'src-tauri');
    for (const relIcon of overlay.bundle.icon) {
      const resolvedIcon = path.resolve(srcTauriDir, relIcon);
      assert.ok(fs.existsSync(resolvedIcon), `Overlay icon path does not resolve: ${relIcon}`);
    }
  });

  // 3. SPDX expression evaluator unit tests
  runTest('spdx-license-expression-evaluator', () => {
    const allowed = new Set(['MIT', 'Apache-2.0', 'Apache-2.0 WITH LLVM-exception', 'BSD-3-Clause', 'Zlib']);
    assert.equal(evaluateSpdxExpression('MIT OR Apache-2.0', allowed), true);
    assert.equal(evaluateSpdxExpression('(MIT OR CustomProprietary) AND BSD-3-Clause', allowed), true);
    assert.equal(evaluateSpdxExpression('MIT AND GPL-3.0-only', allowed), false);
    assert.equal(evaluateSpdxExpression('Apache-2.0 WITH LLVM-exception', allowed), true);
    assert.equal(evaluateSpdxExpression('MIT/Apache-2.0', allowed), true);
    assert.equal(evaluateSpdxExpression('AGPL-3.0-only', allowed), false);
  });

  // 4. Synthetic binary inspection, classification, and manifest tests in `os.tmpdir()`
  const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'dim-ci-xplat-selftest-'));
  try {
    const winRelDir = path.join(tmpRoot, 'target', 'x86_64-pc-windows-msvc', 'release');
    fs.mkdirSync(winRelDir, { recursive: true });
    const rawExePath = path.join(winRelDir, 'dim-desktop.exe');
    fs.writeFileSync(rawExePath, buildSyntheticPe64Buffer());
    fs.writeFileSync(path.join(winRelDir, 'WebView2Loader.dll'), Buffer.from('MZ-sidecar-placeholder'));

    const nsisDir = path.join(winRelDir, 'bundle', 'nsis');
    const msiDir = path.join(winRelDir, 'bundle', 'msi');
    fs.mkdirSync(nsisDir, { recursive: true });
    fs.mkdirSync(msiDir, { recursive: true });
    const nsisExePath = path.join(nsisDir, 'Multi-Zalo Desktop_0.1.0_x64-setup.exe');
    const msiPath = path.join(msiDir, 'Multi-Zalo Desktop_0.1.0_x64_en-US.msi');
    fs.writeFileSync(nsisExePath, buildSyntheticPe64Buffer({ machine: 0x014c, includeWebView2Import: false }));
    fs.writeFileSync(msiPath, buildSyntheticMsiBuffer());

    runTest('windows-pe-and-installer-classification', () => {
      const rawInsp = inspectArtifact(rawExePath);
      assert.equal(rawInsp.artifact_kind, 'compiled-executable');
      assert.equal(rawInsp.is_installer, false, 'Raw .exe MUST NEVER be classified as an installer');
      assert.equal(rawInsp.cpu_architecture, 'x86_64');
      assert.equal(rawInsp.signing_status, 'UNSIGNED_DEVELOPMENT_ARTIFACT');
      assert.equal(rawInsp.details.pe.authenticode.has_certificate_table, false);
      assert.equal(rawInsp.details.pe.embedded_manifest.requested_execution_level, 'asInvoker');
      assert.equal(rawInsp.details.pe.embedded_manifest.dpi_awareness, 'PerMonitorV2');
      assert.equal(rawInsp.details.pe.imports_webview2_loader, true);
      assert.equal(rawInsp.details.pe.sidecar_webview2_loader.present, true);

      const nsisInsp = inspectArtifact(nsisExePath);
      assert.equal(nsisInsp.artifact_kind, 'installer-nsis');
      assert.equal(nsisInsp.is_installer, true);
      assert.equal(nsisInsp.signing_status, 'UNSIGNED_DEVELOPMENT_ARTIFACT');

      const msiInsp = inspectArtifact(msiPath);
      assert.equal(msiInsp.artifact_kind, 'installer-msi');
      assert.equal(msiInsp.is_installer, true);
      assert.equal(msiInsp.details.msi.valid_cfb_header, true);

      const armPe = parsePeHeaders(buildSyntheticPe64Buffer({ machine: 0xaa64 }));
      assert.equal(armPe.cpu_architecture, 'aarch64');
    });

    const macRelDir = path.join(tmpRoot, 'target', 'aarch64-apple-darwin', 'release');
    const appDir = path.join(macRelDir, 'bundle', 'macos', 'Multi-Zalo Desktop.app');
    const appMacosBinDir = path.join(appDir, 'Contents', 'MacOS');
    const dmgDir = path.join(macRelDir, 'bundle', 'dmg');
    fs.mkdirSync(appMacosBinDir, { recursive: true });
    fs.mkdirSync(dmgDir, { recursive: true });

    const rawMachOPath = path.join(macRelDir, 'dim-desktop');
    fs.writeFileSync(rawMachOPath, buildSyntheticMachO64Buffer({ cpuType: 0x0100000c, withCodeSigCmd: true }));
    fs.writeFileSync(
      path.join(appDir, 'Contents', 'Info.plist'),
      [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<plist version="1.0"><dict>',
        '  <key>CFBundleIdentifier</key><string>com.multizalodesktop.manager</string>',
        '  <key>CFBundleName</key><string>Multi-Zalo Desktop</string>',
        '  <key>CFBundleShortVersionString</key><string>0.1.0</string>',
        '  <key>CFBundleExecutable</key><string>dim-desktop</string>',
        '</dict></plist>',
      ].join('\n'),
      'utf8',
    );
    fs.writeFileSync(path.join(appMacosBinDir, 'dim-desktop'), buildSyntheticMachO64Buffer({ cpuType: 0x0100000c }));
    const dmgPath = path.join(dmgDir, 'Multi-Zalo Desktop_0.1.0_aarch64.dmg');
    fs.writeFileSync(dmgPath, buildSyntheticDmgBuffer());

    runTest('macos-macho-app-bundle-and-dmg-inspection', () => {
      const machoInsp = inspectArtifact(rawMachOPath);
      assert.equal(machoInsp.artifact_kind, 'compiled-executable');
      assert.equal(machoInsp.is_installer, false);
      assert.equal(machoInsp.os, 'macos');
      assert.equal(machoInsp.cpu_architecture, 'aarch64');
      assert.equal(machoInsp.signing_status, 'UNSIGNED_DEVELOPMENT_ARTIFACT');

      const fatParsed = parseMachOHeaders(buildSyntheticMachOFatBuffer());
      assert.equal(fatParsed.valid_macho, true);
      assert.equal(fatParsed.is_universal, true);
      assert.deepEqual(fatParsed.architectures, ['x86_64', 'aarch64']);

      const appInsp = inspectArtifact(appDir);
      assert.equal(appInsp.artifact_kind, 'macos-app-bundle');
      assert.equal(appInsp.is_installer, false);
      assert.equal(appInsp.cpu_architecture, 'aarch64');
      assert.equal(appInsp.details.info_plist.bundle_identifier, 'com.multizalodesktop.manager');
      assert.equal(appInsp.signing_status, 'UNSIGNED_DEVELOPMENT_ARTIFACT');

      const dmgInsp = inspectArtifact(dmgPath);
      assert.equal(dmgInsp.artifact_kind, 'macos-dmg-installer');
      assert.equal(dmgInsp.is_installer, true);
      assert.equal(dmgInsp.details.dmg.valid_udif_koly_trailer, true);
      assert.equal(parseDmgTrailer(buildSyntheticDmgBuffer()).valid_udif_koly_trailer, true);
    });

    const linuxRelDir = path.join(tmpRoot, 'target', 'x86_64-unknown-linux-gnu', 'release');
    const debDir = path.join(linuxRelDir, 'bundle', 'deb');
    const appImageDir = path.join(linuxRelDir, 'bundle', 'appimage');
    fs.mkdirSync(debDir, { recursive: true });
    fs.mkdirSync(appImageDir, { recursive: true });

    const elfPath = path.join(linuxRelDir, 'dim-desktop');
    const appImagePath = path.join(appImageDir, 'multi-zalo-desktop_0.1.0_amd64.AppImage');
    const debPath = path.join(debDir, 'multi-zalo-desktop_0.1.0_amd64.deb');
    fs.writeFileSync(elfPath, buildSyntheticElf64Buffer({ isAppImage: false }));
    fs.writeFileSync(appImagePath, buildSyntheticElf64Buffer({ isAppImage: true }));
    fs.writeFileSync(debPath, buildSyntheticDebBuffer());

    runTest('linux-elf-appimage-and-deb-inspection', () => {
      const elfParsed = parseElfHeaders(fs.readFileSync(elfPath));
      assert.equal(elfParsed.valid_elf, true);
      assert.equal(elfParsed.elf_class, 'ELF64');
      assert.equal(elfParsed.cpu_architecture, 'x86_64');
      assert.equal(elfParsed.max_glibc_version, '2.39');

      const elfInsp = inspectArtifact(elfPath);
      assert.equal(elfInsp.artifact_kind, 'compiled-executable');
      assert.equal(elfInsp.is_installer, false);
      assert.equal(elfInsp.os, 'linux');

      const aiParsed = parseAppImageHeaders(fs.readFileSync(appImagePath));
      assert.equal(aiParsed.is_type2_appimage, true);
      assert.equal(aiParsed.appimage_magic_offset_8, '414902');

      const aiInsp = inspectArtifact(appImagePath);
      assert.equal(aiInsp.artifact_kind, 'linux-appimage');
      assert.equal(aiInsp.is_installer, false);

      const debParsed = parseDebHeaders(fs.readFileSync(debPath), debPath);
      assert.equal(debParsed.valid_ar_header, true);
      assert.equal(debParsed.has_debian_binary_member, true);
      assert.equal(classifyArtifact(debPath).artifact_kind, 'debian-package');
      assert.equal(classifyArtifact(debPath).is_installer, true);
      assert.equal(parseMsiHeaders(buildSyntheticMsiBuffer()).valid_cfb_header, true);
    });

    // 5. End-to-end Section 7 manifest + `SELF_REPORTED_UNATTESTED` provenance verification
    const manifestsDir = path.join(tmpRoot, 'manifests');
    runTest('generate-and-merge-section7-manifests', () => {
      const winOutDir = path.join(manifestsDir, 'manifest-windows-x86_64');
      const macOutDir = path.join(manifestsDir, 'manifest-macos-arm64');
      const linOutDir = path.join(manifestsDir, 'manifest-ubuntu-2404');
      const mergedDir = path.join(manifestsDir, 'merged');

      execFileSync(process.execPath, [
        path.join(__dirname, 'generate-artifact-manifest.mjs'),
        '--artifacts-dir', winRelDir,
        '--target', 'x86_64-pc-windows-msvc',
        '--os-label', 'windows-2022',
        '--arch', 'x86_64',
        '--test-status', 'PASSED_NATIVE',
        '--build-log-url', 'https://example.invalid/runs/1',
        '--out-manifest', path.join(winOutDir, 'artifact-manifest.json'),
        '--out-sha256', path.join(winOutDir, 'SHA256SUMS.txt'),
        '--out-md', path.join(winOutDir, 'artifact-summary.md'),
      ], { stdio: 'pipe' });

      fs.writeFileSync(path.join(winOutDir, 'prereq.json'), JSON.stringify({ status: 'READY' }), 'utf8');

      execFileSync(process.execPath, [
        path.join(__dirname, 'generate-artifact-manifest.mjs'),
        '--artifacts-dir', macRelDir,
        '--target', 'aarch64-apple-darwin',
        '--os-label', 'macos-15',
        '--arch', 'arm64',
        '--test-status', 'PASSED_NATIVE',
        '--build-log-url', 'https://example.invalid/runs/1',
        '--out-manifest', path.join(macOutDir, 'artifact-manifest.json'),
        '--out-sha256', path.join(macOutDir, 'SHA256SUMS.txt'),
      ], { stdio: 'pipe' });

      execFileSync(process.execPath, [
        path.join(__dirname, 'generate-artifact-manifest.mjs'),
        '--artifacts-dir', linuxRelDir,
        '--target', 'x86_64-unknown-linux-gnu',
        '--os-label', 'ubuntu-24.04',
        '--arch', 'x86_64',
        '--test-status', 'PASSED_NATIVE',
        '--build-log-url', 'https://example.invalid/runs/1',
        '--out-manifest', path.join(linOutDir, 'artifact-manifest.json'),
        '--out-sha256', path.join(linOutDir, 'SHA256SUMS.txt'),
      ], { stdio: 'pipe' });

      const winManifest = JSON.parse(fs.readFileSync(path.join(winOutDir, 'artifact-manifest.json'), 'utf8'));
      assert.equal(winManifest.provenance_trust_level, 'SELF_REPORTED_UNATTESTED');
      assert.equal(winManifest.artifacts.length, 3);
      const rawEntry = winManifest.artifacts.find((a) => a.name === 'dim-desktop.exe');
      assert.ok(rawEntry);
      assert.equal(rawEntry.sha256, sha256Hex(rawExePath));
      assert.equal(rawEntry.is_installer, false);
      assert.equal(rawEntry.signing_status, 'UNSIGNED_DEVELOPMENT_ARTIFACT');
      assert.equal(rawEntry.provenance_trust_level, 'SELF_REPORTED_UNATTESTED');

      execFileSync(process.execPath, [
        path.join(__dirname, 'generate-artifact-manifest.mjs'),
        '--merge-manifests', manifestsDir,
        '--out-manifest', path.join(mergedDir, 'cross-platform-manifest.json'),
        '--out-sha256', path.join(mergedDir, 'SHA256SUMS.txt'),
        '--out-md', path.join(mergedDir, 'cross-platform-summary.md'),
      ], { stdio: 'pipe' });

      const merged = JSON.parse(fs.readFileSync(path.join(mergedDir, 'cross-platform-manifest.json'), 'utf8'));
      assert.equal(merged.merged_manifests_count, 3);
      assert.equal(merged.artifacts.length, 9);

      for (const entry of merged.artifacts) {
        for (const field of SECTION_7_REQUIRED_FIELDS) {
          assert.notEqual(entry[field], undefined, `Missing field ${field}`);
          assert.notEqual(entry[field], null, `Null field ${field}`);
          assert.notEqual(entry[field], '', `Empty field ${field}`);
        }
        assert.equal(entry.signing_status, 'UNSIGNED_DEVELOPMENT_ARTIFACT');
        assert.equal(entry.provenance_trust_level, 'SELF_REPORTED_UNATTESTED');
        assert.equal(entry.vng_binary_embedded, false);
      }
    });

    // 6. Zero-VNG-Binary Policy rejection
    runTest('zero-vng-binary-policy-rejection', () => {
      const poisonedDir = path.join(tmpRoot, 'poisoned');
      fs.mkdirSync(poisonedDir, { recursive: true });
      fs.writeFileSync(path.join(poisonedDir, 'Zalo.exe'), buildSyntheticPe64Buffer());

      const res = spawnSync(process.execPath, [
        path.join(__dirname, 'generate-artifact-manifest.mjs'),
        '--artifacts-dir', poisonedDir,
        '--out-manifest', path.join(tmpRoot, 'should-not-exist.json'),
      ], { encoding: 'utf8' });
      assert.notEqual(res.status, 0, 'generate-artifact-manifest.mjs MUST fail non-zero on Zalo.exe');
      assert.match(`${res.stdout}\n${res.stderr}`, /Zero-VNG-Binary/i);
    });

    // 7. `check-prerequisites.mjs` & `build-and-package.mjs --dry-run` native vs non-native verification
    runTest('check-prerequisites-and-build-dry-run-contract', () => {
      assert.equal(PINNED_TAURI_CLI_VERSION, '2.12.5');
      assert.equal(PINNED_TAURI_NPM_SPEC, '@tauri-apps/cli@2.12.5');

      const prereqJsonPath = path.join(tmpRoot, 'prereq-out.json');
      execFileSync(process.execPath, [
        path.join(__dirname, 'check-prerequisites.mjs'),
        '--target', 'x86_64-pc-windows-msvc',
        '--out', prereqJsonPath,
      ], { stdio: 'pipe' });
      const prereq = JSON.parse(fs.readFileSync(prereqJsonPath, 'utf8'));
      assert.equal(prereq.requested_target, 'x86_64-pc-windows-msvc');
      assert.equal(prereq.icons.has_icns, false);
      assert.equal(prereq.icons.ci_bundle_overlay.mitigates_ir002_ir003_in_ci, true);

      // Dry-run cross-target check: non-native target MUST set native_runtime_test_possible === false and enforce `--locked`
      const foreignTriple = process.platform === 'darwin' ? 'x86_64-unknown-linux-gnu' : 'aarch64-apple-darwin';
      const buildSummaryPath = path.join(tmpRoot, 'dry-build-summary.json');
      execFileSync(process.execPath, [
        path.join(__dirname, 'build-and-package.mjs'),
        '--target', foreignTriple,
        '--bundles', 'none',
        '--dry-run',
        '--out', buildSummaryPath,
      ], { stdio: 'pipe' });
      const buildSummary = JSON.parse(fs.readFileSync(buildSummaryPath, 'utf8'));
      assert.equal(buildSummary.native_runtime_test_possible, false);
      assert.equal(buildSummary.cargo_locked_enforced, true);
      assert.equal(buildSummary.pinned_tauri_cli_spec, '@tauri-apps/cli@2.12.5');
      assert.match(buildSummary.runtime_fixture_status, /BLOCKED_NON_NATIVE_TARGET/);
      for (const step of buildSummary.steps) {
        assert.ok(step.command.includes('--locked'), `Cargo step missing --locked: ${step.command}`);
      }
    });

    // 8. `dependency-audit.mjs` 7-pillar separation + honesty when advisory DB is unscanned vs real `cargo-deny`
    runTest('dependency-audit-7-pillars-and-advisory-honesty', () => {
      // 8a. Simulated unscanned advisory DB MUST report UNVERIFIED_ADVISORY_DB_NOT_SCANNED & null vulnerabilities!
      const unverifiedJsonPath = path.join(tmpRoot, 'dep-audit-unverified.json');
      execFileSync(process.execPath, [
        path.join(__dirname, 'dependency-audit.mjs'),
        '--simulate-no-advisory-db',
        '--out-json', unverifiedJsonPath,
      ], { stdio: 'pipe' });
      const unverified = JSON.parse(fs.readFileSync(unverifiedJsonPath, 'utf8'));
      assert.equal(unverified.checks.advisory_scan.status, 'UNVERIFIED_ADVISORY_DB_NOT_SCANNED');
      assert.equal(unverified.checks.advisory_scan.vulnerabilities_found, null);
      assert.equal(unverified.supply_chain_pillars.sbom.is_independent_build_provenance, false);
      assert.equal(unverified.supply_chain_pillars.provenance.status, 'SELF_REPORTED_UNATTESTED');

      // 8b. Live `dependency-audit.mjs` run
      const auditJsonPath = path.join(tmpRoot, 'dep-audit.json');
      const auditMdPath = path.join(tmpRoot, 'dep-audit.md');
      execFileSync(process.execPath, [
        path.join(__dirname, 'dependency-audit.mjs'),
        '--out-json', auditJsonPath,
        '--out-md', auditMdPath,
      ], { stdio: 'pipe' });
      const audit = JSON.parse(fs.readFileSync(auditJsonPath, 'utf8'));
      assert.equal(audit.policy_check_passed, true);
      assert.equal(audit.checks.banned_crates_found.length, 0);
      assert.equal(audit.checks.disallowed_sources.length, 0);
      assert.equal(audit.checks.missing_or_invalid_checksums.length, 0);
      assert.equal(audit.checks.vng_proprietary_binaries_found.length, 0);
      assert.equal(audit.checks.msrv_alignment.mismatch_detected, true);
      assert.deepEqual(Object.keys(audit.supply_chain_pillars), [
        'lockfile_checksum_verification',
        'license_policy',
        'known_vulnerability_advisory_scan',
        'sbom',
        'provenance',
        'artifact_signature',
        'installer_integrity',
      ]);
      assert.ok(fs.existsSync(auditMdPath));
    });
  } finally {
    fs.rmSync(tmpRoot, { recursive: true, force: true });
  }

  console.log(JSON.stringify({
    suite: 'ci-cross-platform-selftest',
    passed: results.length,
    failed: 0,
    tests: results,
  }, null, 2));
}

try {
  main();
} catch (err) {
  console.error(`[selftest] FAILED: ${err.stack || err.message}`);
  process.exit(1);
}
