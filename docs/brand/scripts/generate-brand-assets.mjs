import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..', '..', '..');
const assetsDir = path.join(repoRoot, 'docs', 'brand', 'assets');
const conceptsDir = path.join(assetsDir, 'concepts');
const docsIconsDir = path.join(assetsDir, 'icons');
const docsTrayDir = path.join(assetsDir, 'tray');
const marketingDir = path.join(assetsDir, 'marketing');
const tauriIconsDir = path.join(repoRoot, 'desktop-instance-manager', 'apps', 'desktop', 'src-tauri', 'icons');
const tauriTrayDir = path.join(tauriIconsDir, 'tray');

for (const dir of [assetsDir, conceptsDir, docsIconsDir, docsTrayDir, marketingDir, tauriIconsDir, tauriTrayDir]) {
  fs.mkdirSync(dir, { recursive: true });
}

const EDGE_PATH = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const CHROME_PATH = 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const browserBin = fs.existsSync(EDGE_PATH) ? EDGE_PATH : (fs.existsSync(CHROME_PATH) ? CHROME_PATH : null);
if (!browserBin) {
  throw new Error('Headless Edge or Chrome is required to rasterize brand assets.');
}

/* ========================================================================
 * 1. Canonical NexaDock Vector Masters (SVG)
 * ======================================================================== */

const MARK_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" role="img" aria-label="NexaDock mark">
  <rect x="2" y="2" width="20" height="20" rx="5.5" stroke="#182B32" stroke-width="1.75" />
  <rect x="5" y="5" width="3.75" height="14" rx="1.75" fill="#182B32" />
  <rect x="10.75" y="5" width="8.25" height="6.25" rx="1.75" fill="#247D70" />
  <circle cx="16.25" cy="8.125" r="1.15" fill="#FFFFFF" />
  <rect x="10.75" y="12.75" width="8.25" height="6.25" rx="1.75" stroke="#182B32" stroke-width="1.65" />
</svg>
`;

const MARK_SMALL_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="16" height="16" fill="none" role="img" aria-label="NexaDock small mark">
  <rect x="2" y="2" width="20" height="20" rx="5" stroke="#182B32" stroke-width="2.2" />
  <rect x="5" y="5" width="4" height="14" rx="1.5" fill="#182B32" />
  <rect x="11" y="5" width="8" height="6.25" rx="1.5" fill="#247D70" />
  <rect x="11" y="12.75" width="8" height="6.25" rx="1.5" fill="#182B32" />
</svg>
`;

const MARK_DARK_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" role="img" aria-label="NexaDock dark mark">
  <rect x="2" y="2" width="20" height="20" rx="5.5" stroke="#EAF2EF" stroke-width="1.75" />
  <rect x="5" y="5" width="3.75" height="14" rx="1.75" fill="#EAF2EF" />
  <rect x="10.75" y="5" width="8.25" height="6.25" rx="1.75" fill="#2DD4BF" />
  <circle cx="16.25" cy="8.125" r="1.15" fill="#0F171A" />
  <rect x="10.75" y="12.75" width="8.25" height="6.25" rx="1.75" stroke="#EAF2EF" stroke-width="1.65" />
</svg>
`;

const MARK_MONO_BLACK_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" role="img" aria-label="NexaDock monochrome black mark">
  <rect x="2" y="2" width="20" height="20" rx="5.5" stroke="#111111" stroke-width="1.85" />
  <rect x="5" y="5" width="3.75" height="14" rx="1.75" fill="#111111" />
  <rect x="10.75" y="5" width="8.25" height="6.25" rx="1.75" fill="#111111" />
  <circle cx="16.25" cy="8.125" r="1.15" fill="#FFFFFF" />
  <rect x="10.75" y="12.75" width="8.25" height="6.25" rx="1.75" stroke="#111111" stroke-width="1.65" />
</svg>
`;

const MARK_MONO_WHITE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" role="img" aria-label="NexaDock monochrome white mark">
  <rect x="2" y="2" width="20" height="20" rx="5.5" stroke="#FFFFFF" stroke-width="1.85" />
  <rect x="5" y="5" width="3.75" height="14" rx="1.75" fill="#FFFFFF" />
  <rect x="10.75" y="5" width="8.25" height="6.25" rx="1.75" fill="#FFFFFF" />
  <circle cx="16.25" cy="8.125" r="1.15" fill="#111111" />
  <rect x="10.75" y="12.75" width="8.25" height="6.25" rx="1.75" stroke="#FFFFFF" stroke-width="1.65" />
</svg>
`;

const APP_ICON_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256" fill="none" role="img" aria-label="NexaDock App Icon">
  <rect x="12" y="12" width="232" height="232" rx="54" fill="#182B32" />
  <rect x="14" y="14" width="228" height="228" rx="52" stroke="#2D4A54" stroke-width="4" />
  <rect x="46" y="48" width="44" height="160" rx="18" fill="#DCE8E2" />
  <rect x="108" y="48" width="102" height="72" rx="18" fill="#247D70" />
  <circle cx="176" cy="84" r="11" fill="#FFFFFF" />
  <rect x="108" y="136" width="102" height="72" rx="18" fill="#213840" stroke="#9FB3B9" stroke-width="8" />
</svg>
`;

const APP_ICON_SMALL_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64" fill="none" role="img" aria-label="NexaDock Small App Icon">
  <rect x="2" y="2" width="60" height="60" rx="15" fill="#182B32" />
  <rect x="11" y="12" width="12" height="40" rx="4.5" fill="#DCE8E2" />
  <rect x="27" y="12" width="26" height="18" rx="4.5" fill="#247D70" />
  <rect x="27" y="34" width="26" height="18" rx="4.5" stroke="#DCE8E2" stroke-width="3.5" fill="#213840" />
</svg>
`;

const APP_ICON_MICRO_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16" fill="none" role="img" aria-label="NexaDock Micro App Icon">
  <rect x="0" y="0" width="16" height="16" rx="3.5" fill="#182B32" />
  <rect x="2.5" y="2.5" width="3" height="11" rx="1" fill="#DCE8E2" />
  <rect x="7" y="2.5" width="6.5" height="4.5" rx="1" fill="#2DD4BF" />
  <rect x="7" y="9" width="6.5" height="4.5" rx="1" fill="#7B9199" />
</svg>
`;

const TRAY_COLOR_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32" fill="none" role="img" aria-label="NexaDock Tray Icon">
  <rect x="2" y="2" width="28" height="28" rx="7" fill="#182B32" />
  <rect x="6.5" y="6.5" width="5.5" height="19" rx="2.2" fill="#DCE8E2" />
  <rect x="14" y="6.5" width="11.5" height="8.5" rx="2.2" fill="#247D70" />
  <rect x="14" y="17" width="11.5" height="8.5" rx="2.2" fill="#FFFFFF" />
</svg>
`;

const TRAY_ATTENTION_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32" fill="none" role="img" aria-label="NexaDock Tray Attention Icon">
  <rect x="2" y="2" width="28" height="28" rx="7" fill="#182B32" />
  <rect x="6.5" y="6.5" width="5.5" height="19" rx="2.2" fill="#DCE8E2" />
  <rect x="14" y="6.5" width="11.5" height="8.5" rx="2.2" fill="#247D70" />
  <rect x="14" y="17" width="11.5" height="8.5" rx="2.2" fill="#FFFFFF" />
  <circle cx="25.5" cy="6.5" r="5" fill="#F59E0B" stroke="#182B32" stroke-width="2" />
</svg>
`;

const TRAY_MAC_TEMPLATE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" role="img" aria-label="NexaDock macOS Template Icon">
  <rect x="2" y="2" width="20" height="20" rx="5" stroke="#000000" stroke-width="2" />
  <rect x="5" y="5" width="4" height="14" rx="1.5" fill="#000000" />
  <rect x="11" y="5" width="8" height="6.25" rx="1.5" fill="#000000" />
  <rect x="11" y="13" width="8" height="6" rx="1.5" stroke="#000000" stroke-width="1.8" />
</svg>
`;

const WORDMARK_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 156 32" width="156" height="32" fill="none" role="img" aria-label="NexaDock wordmark">
  <text x="2" y="23" fill="#182B32" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="22" font-weight="700" letter-spacing="-0.4">Nexa<tspan fill="#247D70">Dock</tspan></text>
</svg>
`;

const LOCKUP_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 196 36" width="196" height="36" fill="none" role="img" aria-label="NexaDock logo lockup">
  <g transform="translate(2 2) scale(1.3333)">
    <rect x="2" y="2" width="20" height="20" rx="5.5" stroke="#182B32" stroke-width="1.75" />
    <rect x="5" y="5" width="3.75" height="14" rx="1.75" fill="#182B32" />
    <rect x="10.75" y="5" width="8.25" height="6.25" rx="1.75" fill="#247D70" />
    <circle cx="16.25" cy="8.125" r="1.15" fill="#FFFFFF" />
    <rect x="10.75" y="12.75" width="8.25" height="6.25" rx="1.75" stroke="#182B32" stroke-width="1.65" />
  </g>
  <text x="44" y="25" fill="#182B32" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="22" font-weight="700" letter-spacing="-0.4">Nexa<tspan fill="#247D70">Dock</tspan></text>
</svg>
`;

const LOCKUP_COMPACT_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 148 28" width="148" height="28" fill="none" role="img" aria-label="NexaDock compact lockup">
  <g transform="translate(2 2)">
    <rect x="2" y="2" width="20" height="20" rx="5.5" stroke="#182B32" stroke-width="1.75" />
    <rect x="5" y="5" width="3.75" height="14" rx="1.75" fill="#182B32" />
    <rect x="10.75" y="5" width="8.25" height="6.25" rx="1.75" fill="#247D70" />
    <circle cx="16.25" cy="8.125" r="1.15" fill="#FFFFFF" />
    <rect x="10.75" y="12.75" width="8.25" height="6.25" rx="1.75" stroke="#182B32" stroke-width="1.65" />
  </g>
  <text x="34" y="20" fill="#182B32" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="17" font-weight="700" letter-spacing="-0.3">Nexa<tspan fill="#247D70">Dock</tspan></text>
</svg>
`;

const GITHUB_HERO_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 400" width="1200" height="400" fill="none" role="img" aria-label="NexaDock — Independent Desktop Account Manager">
  <rect width="1200" height="400" rx="20" fill="#F6F7F5" />
  <rect x="1" y="1" width="1198" height="398" rx="19" stroke="#D7E2DC" stroke-width="2" />

  <!-- Right architectural workspace illustration -->
  <rect x="780" y="48" width="368" height="304" rx="18" fill="#FFFFFF" stroke="#D7E2DC" stroke-width="1.5" />
  <!-- Dock Rail -->
  <rect x="802" y="70" width="68" height="260" rx="12" fill="#182B32" />
  <circle cx="836" cy="106" r="14" fill="#DCE8E2" />
  <text x="836" y="111" text-anchor="middle" fill="#182B32" font-family="'Inter', 'Segoe UI', sans-serif" font-size="11" font-weight="700">Z</text>
  <circle cx="836" cy="154" r="14" fill="#247D70" />
  <text x="836" y="159" text-anchor="middle" fill="#FFFFFF" font-family="'Inter', 'Segoe UI', sans-serif" font-size="11" font-weight="700">CV</text>
  <circle cx="836" cy="202" r="14" fill="#1E56A0" />
  <text x="836" y="207" text-anchor="middle" fill="#FFFFFF" font-family="'Inter', 'Segoe UI', sans-serif" font-size="11" font-weight="700">CN</text>
  <circle cx="836" cy="250" r="14" fill="#A14A0A" />
  <text x="836" y="255" text-anchor="middle" fill="#FFFFFF" font-family="'Inter', 'Segoe UI', sans-serif" font-size="11" font-weight="700">BH</text>

  <!-- Active isolated bay card -->
  <rect x="888" y="70" width="238" height="122" rx="12" fill="#FFFFFF" stroke="#247D70" stroke-width="2" />
  <rect x="896" y="82" width="4" height="98" rx="2" fill="#247D70" />
  <rect x="912" y="90" width="96" height="12" rx="6" fill="#182B32" />
  <rect x="912" y="112" width="136" height="8" rx="4" fill="#B6C7BE" />
  <rect x="912" y="144" width="94" height="28" rx="7" fill="#247D70" />
  <rect x="1016" y="144" width="66" height="28" rx="7" fill="#EDF2EF" stroke="#D7E2DC" stroke-width="1" />

  <!-- Secondary isolated bay card -->
  <rect x="888" y="208" width="238" height="122" rx="12" fill="#F6F7F5" stroke="#D7E2DC" stroke-width="1.5" />
  <rect x="910" y="228" width="84" height="12" rx="6" fill="#41565E" />
  <rect x="910" y="250" width="124" height="8" rx="4" fill="#B6C7BE" />
  <rect x="910" y="282" width="112" height="28" rx="7" fill="#FFFFFF" stroke="#B6C7BE" stroke-width="1" />

  <!-- Left Brand Identity -->
  <g transform="translate(64 68) scale(2.4)">
    <rect x="2" y="2" width="20" height="20" rx="5.5" stroke="#182B32" stroke-width="1.75" />
    <rect x="5" y="5" width="3.75" height="14" rx="1.75" fill="#182B32" />
    <rect x="10.75" y="5" width="8.25" height="6.25" rx="1.75" fill="#247D70" />
    <circle cx="16.25" cy="8.125" r="1.15" fill="#FFFFFF" />
    <rect x="10.75" y="12.75" width="8.25" height="6.25" rx="1.75" stroke="#182B32" stroke-width="1.65" />
  </g>
  <text x="140" y="112" fill="#182B32" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="44" font-weight="700" letter-spacing="-0.8">Nexa<tspan fill="#247D70">Dock</tspan></text>

  <rect x="64" y="148" width="348" height="30" rx="15" fill="#DCE8E2" />
  <text x="84" y="167.5" fill="#182B32" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="12" font-weight="600" letter-spacing="0.4">INDEPENDENT DESKTOP ACCOUNT MANAGER</text>

  <text x="64" y="226" fill="#182B32" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="27" font-weight="600">Your accounts. Their own isolated space.</text>
  <text x="64" y="264" fill="#41565E" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="19" font-weight="500">Một máy. Nhiều tài khoản Zalo Desktop. Không trộn dữ liệu.</text>

  <text x="64" y="338" fill="#5A7078" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="14" font-weight="500">Windows · macOS · Ubuntu (Experimental) · Rust + Tauri 2 · MIT OR Apache-2.0 · Zero VNG Binaries</text>
</svg>
`;

const RELEASE_BANNER_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 320" width="1200" height="320" fill="none" role="img" aria-label="NexaDock Release Banner">
  <rect width="1200" height="320" rx="16" fill="#0F171A" />
  <rect x="1" y="1" width="1198" height="318" rx="15" stroke="#273940" stroke-width="2" />

  <g transform="translate(64 60) scale(2.2)">
    <rect x="2" y="2" width="20" height="20" rx="5.5" stroke="#EAF2EF" stroke-width="1.75" />
    <rect x="5" y="5" width="3.75" height="14" rx="1.75" fill="#DCE8E2" />
    <rect x="10.75" y="5" width="8.25" height="6.25" rx="1.75" fill="#247D70" />
    <circle cx="16.25" cy="8.125" r="1.15" fill="#FFFFFF" />
    <rect x="10.75" y="12.75" width="8.25" height="6.25" rx="1.75" stroke="#9FB3B9" stroke-width="1.65" />
  </g>
  <text x="136" y="102" fill="#EAF2EF" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="40" font-weight="700" letter-spacing="-0.6">Nexa<tspan fill="#2DD4BF">Dock</tspan></text>

  <rect x="64" y="136" width="276" height="28" rx="14" fill="#173632" stroke="#247D70" stroke-width="1" />
  <text x="84" y="154.5" fill="#DCE8E2" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="12" font-weight="600">ISOLATED ACCOUNT WORKSPACE</text>

  <text x="64" y="210" fill="#EAF2EF" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="24" font-weight="600">Your accounts. Their own isolated space.</text>
  <text x="64" y="246" fill="#9FB3B9" font-family="'Inter', 'Segoe UI', system-ui, sans-serif" font-size="18" font-weight="500">Một máy. Nhiều tài khoản Zalo Desktop. Không trộn dữ liệu.</text>

  <rect x="884" y="52" width="88" height="216" rx="14" fill="#162126" stroke="#273940" stroke-width="1.5" />
  <rect x="990" y="52" width="146" height="100" rx="14" fill="#247D70" />
  <circle cx="1102" cy="86" r="10" fill="#FFFFFF" />
  <rect x="990" y="168" width="146" height="100" rx="14" fill="#162126" stroke="#DCE8E2" stroke-width="1.75" />
</svg>
`;

fs.writeFileSync(path.join(assetsDir, 'nexadock-mark.svg'), MARK_SVG);
fs.writeFileSync(path.join(assetsDir, 'nexadock-mark-small.svg'), MARK_SMALL_SVG);
fs.writeFileSync(path.join(assetsDir, 'nexadock-light.svg'), MARK_SVG);
fs.writeFileSync(path.join(assetsDir, 'nexadock-dark.svg'), MARK_DARK_SVG);
fs.writeFileSync(path.join(assetsDir, 'nexadock-mono-black.svg'), MARK_MONO_BLACK_SVG);
fs.writeFileSync(path.join(assetsDir, 'nexadock-mono-white.svg'), MARK_MONO_WHITE_SVG);
fs.writeFileSync(path.join(assetsDir, 'nexadock-wordmark.svg'), WORDMARK_SVG);
fs.writeFileSync(path.join(assetsDir, 'nexadock-lockup.svg'), LOCKUP_SVG);
fs.writeFileSync(path.join(assetsDir, 'nexadock-lockup-compact.svg'), LOCKUP_COMPACT_SVG);
fs.writeFileSync(path.join(assetsDir, 'nexadock-app-icon.svg'), APP_ICON_SVG);

fs.writeFileSync(path.join(marketingDir, 'favicon.svg'), MARK_SMALL_SVG);
fs.writeFileSync(path.join(marketingDir, 'github-readme-hero.svg'), GITHUB_HERO_SVG);
fs.writeFileSync(path.join(marketingDir, 'release-banner.svg'), RELEASE_BANNER_SVG);

/* ========================================================================
 * 2. ICO & ICNS Binary Builders (Zero External Dependencies)
 * ======================================================================== */

function buildIcoFromPngs(entries) {
  // entries: Array<{ size: number, data: Buffer }>
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // Reserved
  header.writeUInt16LE(1, 2); // Icon type = 1
  header.writeUInt16LE(entries.length, 4);

  const dirEntries = [];
  let offset = 6 + entries.length * 16;
  for (const entry of entries) {
    const dir = Buffer.alloc(16);
    dir.writeUInt8(entry.size >= 256 ? 0 : entry.size, 0);
    dir.writeUInt8(entry.size >= 256 ? 0 : entry.size, 1);
    dir.writeUInt8(0, 2); // Palette count
    dir.writeUInt8(0, 3); // Reserved
    dir.writeUInt16LE(1, 4); // Color planes
    dir.writeUInt16LE(32, 6); // Bits per pixel
    dir.writeUInt32LE(entry.data.length, 8);
    dir.writeUInt32LE(offset, 12);
    dirEntries.push(dir);
    offset += entry.data.length;
  }
  return Buffer.concat([header, ...dirEntries, ...entries.map((e) => e.data)]);
}

function buildIcnsFromPngs(entries) {
  // entries: Array<{ type: string, data: Buffer }>
  const chunks = [];
  let totalLen = 8;
  for (const entry of entries) {
    const chunkHeader = Buffer.alloc(8);
    chunkHeader.write(entry.type, 0, 4, 'ascii');
    chunkHeader.writeUInt32BE(8 + entry.data.length, 4);
    chunks.push(chunkHeader, entry.data);
    totalLen += 8 + entry.data.length;
  }
  const fileHeader = Buffer.alloc(8);
  fileHeader.write('icns', 0, 4, 'ascii');
  fileHeader.writeUInt32BE(totalLen, 4);
  return Buffer.concat([fileHeader, ...chunks]);
}

/* ========================================================================
 * 3. Headless Browser SVG -> PNG Rasterizer via CDP
 * ======================================================================== */

const server = http.createServer((req, res) => {
  res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
  res.end('<!DOCTYPE html><html><body style="margin:0;padding:0;background:transparent;"></body></html>');
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const port = server.address().port;

const cdpPort = 9350 + Math.floor(Math.random() * 400);
const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nexadock-assets-'));
const proc = spawn(browserBin, [
  '--headless=new',
  '--disable-gpu',
  `--remote-debugging-port=${cdpPort}`,
  `--user-data-dir=${userDataDir}`,
  `http://127.0.0.1:${port}/`
], { stdio: 'ignore' });

async function getWsUrl() {
  for (let i = 0; i < 40; i += 1) {
    try {
      const resp = await fetch(`http://127.0.0.1:${cdpPort}/json/list`);
      const list = await resp.json();
      const page = list.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // Wait for browser
    }
    await new Promise((r) => setTimeout(r, 150));
  }
  throw new Error('Timed out waiting for headless CDP websocket');
}

try {
  const wsUrl = await getWsUrl();
  const ws = new WebSocket(wsUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });

  let nextId = 1;
  const pending = new Map();
  ws.addEventListener('message', (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(msg.error.message));
      else resolve(msg.result);
    }
  });

  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params }));
  });

  await send('Page.enable');
  await send('Runtime.enable');
  await send('Emulation.setDefaultBackgroundColorOverride', { color: { r: 0, g: 0, b: 0, a: 0 } });

  async function renderHtmlToPng(htmlContent, width, height) {
    await send('Emulation.setDeviceMetricsOverride', {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: false
    });
    const payload = JSON.stringify(htmlContent);
    await send('Runtime.evaluate', {
      expression: `(() => {
        document.open();
        document.write(${payload});
        document.close();
      })()`,
      awaitPromise: true
    });
    await new Promise((r) => setTimeout(r, 60));
    const { data } = await send('Page.captureScreenshot', {
      format: 'png',
      omitBackground: true,
      clip: { x: 0, y: 0, width, height, scale: 1 }
    });
    return Buffer.from(data, 'base64');
  }

  async function rasterizeSvg(svgText, width, height) {
    const html = `<!DOCTYPE html><html><head><style>
      html, body { margin: 0; padding: 0; width: ${width}px; height: ${height}px; overflow: hidden; background: transparent; }
      svg { display: block; width: ${width}px; height: ${height}px; }
    </style></head><body>${svgText}</body></html>`;
    return renderHtmlToPng(html, width, height);
  }

  // App icon PNG ladder
  const appIconSizes = [16, 20, 24, 32, 48, 64, 128, 256, 512, 1024];
  const appPngBySize = new Map();
  for (const size of appIconSizes) {
    const svgSource = size <= 24 ? APP_ICON_MICRO_SVG : (size <= 48 ? APP_ICON_SMALL_SVG : APP_ICON_SVG);
    const pngBuf = await rasterizeSvg(svgSource, size, size);
    appPngBySize.set(size, pngBuf);
    const filename = size === 1024 ? 'icon-1024.png' : `${size}x${size}.png`;
    fs.writeFileSync(path.join(tauriIconsDir, filename), pngBuf);
    fs.writeFileSync(path.join(docsIconsDir, filename), pngBuf);
  }

  // Tauri required aliases
  fs.writeFileSync(path.join(tauriIconsDir, '128x128@2x.png'), appPngBySize.get(256));
  fs.writeFileSync(path.join(docsIconsDir, '128x128@2x.png'), appPngBySize.get(256));
  fs.writeFileSync(path.join(tauriIconsDir, 'icon.png'), appPngBySize.get(512));
  fs.writeFileSync(path.join(docsIconsDir, 'icon.png'), appPngBySize.get(512));

  // Windows multi-res ICO
  const icoBuf = buildIcoFromPngs([16, 24, 32, 48, 64, 128, 256].map((size) => ({
    size,
    data: appPngBySize.get(size)
  })));
  fs.writeFileSync(path.join(tauriIconsDir, 'icon.ico'), icoBuf);
  fs.writeFileSync(path.join(docsIconsDir, 'icon.ico'), icoBuf);

  // macOS ICNS
  const icnsBuf = buildIcnsFromPngs([
    { type: 'icp4', data: appPngBySize.get(16) },
    { type: 'icp5', data: appPngBySize.get(32) },
    { type: 'icp6', data: appPngBySize.get(64) },
    { type: 'ic07', data: appPngBySize.get(128) },
    { type: 'ic08', data: appPngBySize.get(256) },
    { type: 'ic09', data: appPngBySize.get(512) },
    { type: 'ic10', data: appPngBySize.get(1024) }
  ]);
  fs.writeFileSync(path.join(tauriIconsDir, 'icon.icns'), icnsBuf);
  fs.writeFileSync(path.join(docsIconsDir, 'icon.icns'), icnsBuf);

  // Tray icons
  const trayWin16 = await rasterizeSvg(TRAY_COLOR_SVG, 16, 16);
  const trayWin32 = await rasterizeSvg(TRAY_COLOR_SVG, 32, 32);
  const trayWinIco = buildIcoFromPngs([
    { size: 16, data: trayWin16 },
    { size: 32, data: trayWin32 }
  ]);
  const trayAttention = await rasterizeSvg(TRAY_ATTENTION_SVG, 32, 32);
  const trayMac1x = await rasterizeSvg(TRAY_MAC_TEMPLATE_SVG, 22, 22);
  const trayMac2x = await rasterizeSvg(TRAY_MAC_TEMPLATE_SVG, 44, 44);

  fs.writeFileSync(path.join(tauriTrayDir, 'tray-windows.png'), trayWin32);
  fs.writeFileSync(path.join(docsTrayDir, 'tray-windows.png'), trayWin32);
  fs.writeFileSync(path.join(tauriTrayDir, 'tray-windows.ico'), trayWinIco);
  fs.writeFileSync(path.join(docsTrayDir, 'tray-windows.ico'), trayWinIco);
  fs.writeFileSync(path.join(tauriTrayDir, 'tray-linux.png'), trayWin32);
  fs.writeFileSync(path.join(docsTrayDir, 'tray-linux.png'), trayWin32);
  fs.writeFileSync(path.join(tauriTrayDir, 'tray-attention.png'), trayAttention);
  fs.writeFileSync(path.join(docsTrayDir, 'tray-attention.png'), trayAttention);
  fs.writeFileSync(path.join(tauriTrayDir, 'tray-macos-template.png'), trayMac1x);
  fs.writeFileSync(path.join(docsTrayDir, 'tray-macos-template.png'), trayMac1x);
  fs.writeFileSync(path.join(tauriTrayDir, 'tray-macos-template@2x.png'), trayMac2x);
  fs.writeFileSync(path.join(docsTrayDir, 'tray-macos-template@2x.png'), trayMac2x);

  // Favicon ICO + Marketing PNGs
  const fav16 = await rasterizeSvg(MARK_SMALL_SVG, 16, 16);
  const fav32 = await rasterizeSvg(MARK_SMALL_SVG, 32, 32);
  fs.writeFileSync(path.join(marketingDir, 'favicon.ico'), buildIcoFromPngs([
    { size: 16, data: fav16 },
    { size: 32, data: fav32 }
  ]));
  fs.writeFileSync(path.join(marketingDir, 'github-readme-hero.png'), await rasterizeSvg(GITHUB_HERO_SVG, 1200, 400));
  fs.writeFileSync(path.join(marketingDir, 'release-banner.png'), await rasterizeSvg(RELEASE_BANNER_SVG, 1200, 320));
  const socialHtml = `<!DOCTYPE html><html><head><style>
    html, body { margin: 0; padding: 0; width: 1280px; height: 640px; background: #F6F7F5; display: grid; place-items: center; }
    svg { width: 1200px; height: 400px; }
  </style></head><body>${GITHUB_HERO_SVG}</body></html>`;
  fs.writeFileSync(path.join(marketingDir, 'github-social-preview.png'), await renderHtmlToPng(socialHtml, 1280, 640));

  // Concept evaluation & size-ladder verification sheet (16px..256px, Light/Dark/Mono/Grayscale)
  const conceptA = fs.readFileSync(path.join(conceptsDir, 'concept-a-docked-bays.svg'), 'utf8');
  const conceptB = fs.readFileSync(path.join(conceptsDir, 'concept-b-nexus-bridge.svg'), 'utf8');
  const conceptC = fs.readFileSync(path.join(conceptsDir, 'concept-c-shielded-stack.svg'), 'utf8');
  const iconDataUri = `data:image/png;base64,${appPngBySize.get(256).toString('base64')}`;

  const sheetHtml = `<!DOCTYPE html>
  <html lang="en">
  <head>
    <meta charset="UTF-8" />
    <style>
      * { box-sizing: border-box; }
      body {
        margin: 0;
        padding: 32px;
        width: 1280px;
        font-family: "Inter", "Segoe UI", system-ui, sans-serif;
        background: #F6F7F5;
        color: #182B32;
      }
      h1 { margin: 0 0 6px; font-size: 24px; font-weight: 700; }
      p.sub { margin: 0 0 24px; font-size: 14px; color: #41565E; }
      .grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-bottom: 24px; }
      .card {
        background: #FFFFFF;
        border: 1px solid #D7E2DC;
        border-radius: 12px;
        padding: 16px;
      }
      .card.selected { border: 2px solid #247D70; }
      .tag {
        display: inline-block;
        padding: 2px 8px;
        border-radius: 999px;
        font-size: 11px;
        font-weight: 600;
        background: #DCE8E2;
        color: #182B32;
        margin-bottom: 8px;
      }
      .card h2 { margin: 0 0 6px; font-size: 16px; }
      .card p { margin: 0 0 12px; font-size: 12px; color: #41565E; line-height: 1.4; }
      .ladder { display: flex; align-items: flex-end; gap: 12px; flex-wrap: wrap; padding: 12px; background: #F6F7F5; border-radius: 8px; }
      .ladder-item { display: flex; flex-direction: column; align-items: center; gap: 4px; font-size: 10px; color: #5A7078; }
      .surfaces { display: grid; grid-template-columns: repeat(4, 1fr); gap: 16px; }
      .surface-box { padding: 16px; border-radius: 12px; border: 1px solid #D7E2DC; display: flex; align-items: center; gap: 16px; }
      .s-light { background: #F6F7F5; color: #182B32; }
      .s-dark { background: #0F171A; color: #EAF2EF; border-color: #273940; }
      .s-mono { background: #FFFFFF; color: #111111; }
      .s-gray { background: #ECECEC; color: #111111; filter: grayscale(100%); }
      .surface-title { font-size: 12px; font-weight: 600; margin-bottom: 4px; }
      .surface-sub { font-size: 11px; opacity: 0.8; }
    </style>
  </head>
  <body>
    <h1>NexaDock — Logo Concept Evaluation &amp; Scale Ladder</h1>
    <p class="sub">Vector construction evaluated at 16px, 24px, 32px, 64px, 128px, and 256px across light, dark, monochrome, and grayscale backgrounds.</p>
    <div class="grid">
      <div class="card selected">
        <span class="tag">SELECTED CONCEPT A</span>
        <h2>The Docked Bays</h2>
        <p>Architectural dock chassis with persistent left rail spine + isolated active &amp; standby workspace bays.</p>
        <div class="ladder">
          ${[16, 24, 32, 64, 128].map((s) => `<div class="ladder-item"><div style="width:${s}px;height:${s}px;">${(s <= 24 ? MARK_SMALL_SVG : conceptA).replace(/width="\d+"/, `width="${s}"`).replace(/height="\d+"/, `height="${s}"`)}</div><span>${s}px</span></div>`).join('')}
        </div>
      </div>
      <div class="card">
        <span class="tag">ALTERNATIVE CONCEPT B</span>
        <h2>The Nexus Bridge</h2>
        <p>Monogram N formed by dual workspace columns joined by a diagonal routing bridge.</p>
        <div class="ladder">
          ${[16, 24, 32, 64, 128].map((s) => `<div class="ladder-item"><div style="width:${s}px;height:${s}px;">${conceptB.replace(/width="\d+"/, `width="${s}"`).replace(/height="\d+"/, `height="${s}"`)}</div><span>${s}px</span></div>`).join('')}
        </div>
      </div>
      <div class="card">
        <span class="tag">ALTERNATIVE CONCEPT C</span>
        <h2>The Shielded Compartment</h2>
        <p>Hexagonal safety enclosure containing stacked account bars. Rejected: shield cliché.</p>
        <div class="ladder">
          ${[16, 24, 32, 64, 128].map((s) => `<div class="ladder-item"><div style="width:${s}px;height:${s}px;">${conceptC.replace(/width="\d+"/, `width="${s}"`).replace(/height="\d+"/, `height="${s}"`)}</div><span>${s}px</span></div>`).join('')}
        </div>
      </div>
    </div>

    <div class="surfaces">
      <div class="surface-box s-light">
        <img src="${iconDataUri}" width="64" height="64" alt="App icon on light" />
        <div>
          <div class="surface-title">App Icon (Light)</div>
          <div class="surface-sub">Taskbar / Explorer</div>
          <div style="margin-top:8px;">${LOCKUP_COMPACT_SVG}</div>
        </div>
      </div>
      <div class="surface-box s-dark">
        <img src="${iconDataUri}" width="64" height="64" alt="App icon on dark" />
        <div>
          <div class="surface-title">App Icon (Dark)</div>
          <div class="surface-sub">Deep Harbor #0F171A</div>
          <div style="margin-top:8px;">${MARK_DARK_SVG}</div>
        </div>
      </div>
      <div class="surface-box s-mono">
        <div style="width:48px;height:48px;">${MARK_MONO_BLACK_SVG.replace('width="24"', 'width="48"').replace('height="24"', 'height="48"')}</div>
        <div>
          <div class="surface-title">Monochrome (Black)</div>
          <div class="surface-sub">Forced-colors / Print</div>
        </div>
      </div>
      <div class="surface-box s-gray">
        <img src="${iconDataUri}" width="48" height="48" alt="Grayscale test" />
        <div>
          <div class="surface-title">Grayscale 100%</div>
          <div class="surface-sub">Luminance separation</div>
        </div>
      </div>
    </div>
  </body>
  </html>`;

  const sheetPng = await renderHtmlToPng(sheetHtml, 1280, 740);
  fs.writeFileSync(path.join(conceptsDir, 'concept-comparison-sheet.png'), sheetPng);

  console.log('Generated NexaDock SVG, PNG (16..1024), ICO, ICNS, tray icons, marketing banners, and concept-comparison-sheet.png.');
  ws.close();
} finally {
  proc.kill();
  server.close();
  try {
    fs.rmSync(userDataDir, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
  } catch {
    // Ignore Windows temp lock
  }
}
