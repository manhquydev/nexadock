import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const dimRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(dimRoot, '..');
const tauriDir = path.join(dimRoot, 'apps', 'desktop', 'src-tauri');
const iconsDir = path.join(tauriDir, 'icons');
const trayDir = path.join(iconsDir, 'tray');

function assertMagic(filePath, expectedHex) {
  assert.ok(fs.existsSync(filePath), `Missing required asset: ${filePath}`);
  const buf = fs.readFileSync(filePath);
  assert.ok(buf.length > 16, `Asset is unexpectedly empty: ${filePath}`);
  const actualHex = buf.subarray(0, expectedHex.length / 2).toString('hex');
  assert.equal(actualHex, expectedHex, `Invalid magic header in ${filePath}: got ${actualHex}`);
}

test('Tauri app icons exist in all required sizes with valid headers', () => {
  const pngs = [
    '16x16.png',
    '20x20.png',
    '24x24.png',
    '32x32.png',
    '48x48.png',
    '64x64.png',
    '128x128.png',
    '128x128@2x.png',
    '256x256.png',
    '512x512.png',
    'icon.png',
    'icon-1024.png',
  ];
  for (const name of pngs) {
    assertMagic(path.join(iconsDir, name), '89504e470d0a1a0a');
  }
  assertMagic(path.join(iconsDir, 'icon.ico'), '00000100');
  assertMagic(path.join(iconsDir, 'icon.icns'), '69636e73');
});

test('System tray icons exist for Windows, macOS template, Linux, and attention states', () => {
  assertMagic(path.join(trayDir, 'tray-windows.ico'), '00000100');
  assertMagic(path.join(trayDir, 'tray-windows.png'), '89504e470d0a1a0a');
  assertMagic(path.join(trayDir, 'tray-macos-template.png'), '89504e470d0a1a0a');
  assertMagic(path.join(trayDir, 'tray-macos-template@2x.png'), '89504e470d0a1a0a');
  assertMagic(path.join(trayDir, 'tray-linux.png'), '89504e470d0a1a0a');
  assertMagic(path.join(trayDir, 'tray-attention.png'), '89504e470d0a1a0a');
});

test('tauri.conf.json enforces NexaDock branding and preserved window/CSP/compatibility contracts', () => {
  const conf = JSON.parse(fs.readFileSync(path.join(tauriDir, 'tauri.conf.json'), 'utf8'));
  assert.equal(conf.productName, 'NexaDock');
  assert.equal(conf.identifier, 'io.github.manhquydev.nexadock');
  assert.equal(conf.app?.withGlobalTauri, true);
  assert.equal(conf.app?.windows?.[0]?.title, 'NexaDock');
  assert.equal(conf.app?.windows?.[0]?.width, 1160);
  assert.equal(conf.app?.windows?.[0]?.height, 780);
  assert.equal(conf.app?.windows?.[0]?.minWidth, 420);
  assert.equal(conf.app?.windows?.[0]?.minHeight, 540);
  assert.ok(conf.app?.security?.csp?.includes("default-src 'self'"), 'Strict CSP must be preserved');
});

test('src-tauri Cargo.toml, main.rs, and UI dictionaries contain no leftover KHOANG or Multi-Zalo user-facing strings', () => {
  const cargoToml = fs.readFileSync(path.join(tauriDir, 'Cargo.toml'), 'utf8');
  const mainRs = fs.readFileSync(path.join(tauriDir, 'src', 'main.rs'), 'utf8');
  const viJson = fs.readFileSync(path.join(dimRoot, 'apps', 'desktop', 'ui', 'i18n', 'vi-VN.json'), 'utf8');
  const enJson = fs.readFileSync(path.join(dimRoot, 'apps', 'desktop', 'ui', 'i18n', 'en-US.json'), 'utf8');
  assert.doesNotMatch(cargoToml, /Multi-Zalo|\bKHOANG\b/i);
  assert.doesNotMatch(mainRs, /Multi-Zalo|\bKHOANG\b/i);
  assert.doesNotMatch(viJson, /\bKHOANG\b/);
  assert.doesNotMatch(enJson, /\bKHOANG\b/);
});

test('README.md and README.en.md feature NexaDock branding, manhquydev/nexadock URLs, taglines, and VNG non-affiliation disclaimer', () => {
  const readmeVi = fs.readFileSync(path.join(repoRoot, 'README.md'), 'utf8');
  const readmeEn = fs.readFileSync(path.join(repoRoot, 'README.en.md'), 'utf8');

  assert.ok(readmeVi.includes('NexaDock'));
  assert.ok(readmeVi.includes('manhquydev/nexadock'));
  assert.doesNotMatch(readmeVi, /https:\/\/github\.com\/manhquydev\/zalo/);
  assert.ok(readmeVi.includes('github-readme-hero.svg'));
  assert.ok(readmeVi.includes('Một máy. Nhiều tài khoản. Không trộn dữ liệu.'));
  assert.ok(readmeVi.includes('VNG Corporation'));

  assert.ok(readmeEn.includes('NexaDock'));
  assert.ok(readmeEn.includes('manhquydev/nexadock'));
  assert.doesNotMatch(readmeEn, /https:\/\/github\.com\/manhquydev\/zalo/);
  assert.ok(readmeEn.includes('github-readme-hero.svg'));
  assert.ok(readmeEn.includes('Your accounts. Their own space.'));
  assert.ok(readmeEn.includes('VNG Corporation'));
});

test('Canonical NexaDock identifiers and filenames are enforced across Rust and UI sources', () => {
  const modelsRs = fs.readFileSync(path.join(dimRoot, 'crates', 'core', 'src', 'models.rs'), 'utf8');
  const prefsRs = fs.readFileSync(path.join(tauriDir, 'src', 'preferences.rs'), 'utf8');
  const trayRs = fs.readFileSync(path.join(tauriDir, 'src', 'tray.rs'), 'utf8');
  const autostartRs = fs.readFileSync(path.join(tauriDir, 'src', 'autostart.rs'), 'utf8');
  const webWorkspaceRs = fs.readFileSync(path.join(tauriDir, 'src', 'web_workspace.rs'), 'utf8');

  assert.match(modelsRs, /pub const APP_DIR_NAME:\s*&str\s*=\s*"NexaDock"/);
  assert.match(prefsRs, /pub const PREFERENCES_FILE_NAME:\s*&str\s*=\s*"nexadock-preferences\.json"/);
  assert.match(trayRs, /pub const TRAY_ID:\s*&str\s*=\s*"nexadock-tray"/);
  assert.match(autostartRs, /io\.github\.manhquydev\.nexadock/);
  assert.match(autostartRs, /nexadock\.desktop/);
  assert.match(webWorkspaceRs, /"nexadock_web_"/);
  assert.match(webWorkspaceRs, /"khoang_web_"/);
});
