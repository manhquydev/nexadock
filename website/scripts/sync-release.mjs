#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const websiteDir = path.resolve(__dirname, '..');
const outPath = path.join(websiteDir, 'src', 'data', 'release.json');
const REPO = 'manhquydev/nexadock';

function parseSha256Sums(text) {
  const map = {};
  for (const line of String(text || '').split(/\r?\n/)) {
    const m = line.trim().match(/^([a-fA-F0-9]{64})\s+\*?(.+)$/);
    if (m) map[m[2].trim()] = m[1].toLowerCase();
  }
  return map;
}

function parseProvenance(text) {
  const meta = {};
  for (const line of String(text || '').split(/\r?\n/)) {
    const idx = line.indexOf('=');
    if (idx > 0) meta[line.slice(0, idx).trim()] = line.slice(idx + 1).trim();
  }
  return meta;
}

async function fetchJson(url) {
  const headers = {
    Accept: 'application/vnd.github+json',
    'User-Agent': 'nexadock-website-sync'
  };
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }
  const res = await fetch(url, { headers, signal: AbortSignal.timeout(12000) });
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  return res.json();
}

async function fetchText(url) {
  const headers = { 'User-Agent': 'nexadock-website-sync' };
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }
  const res = await fetch(url, { headers, signal: AbortSignal.timeout(12000) });
  if (!res.ok) return '';
  return res.text();
}

async function run() {
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  try {
    const releases = await fetchJson(`https://api.github.com/repos/${REPO}/releases?per_page=10`);
    const published = releases.filter((r) => !r.draft);
    const nexadockReleases = published.filter((r) =>
      Array.isArray(r.assets) && r.assets.some((a) => /^NexaDock-/i.test(a.name))
    );
    const target = nexadockReleases[0] || published[0];
    if (!target) throw new Error('No published GitHub Releases found');

    const shaAsset = target.assets.find((a) => a.name === 'SHA256SUMS.txt');
    const provAsset = target.assets.find((a) => a.name === 'PROVENANCE.txt');
    const [shaText, provText] = await Promise.all([
      shaAsset ? fetchText(shaAsset.browser_download_url) : Promise.resolve(''),
      provAsset ? fetchText(provAsset.browser_download_url) : Promise.resolve('')
    ]);

    const shaMap = parseSha256Sums(shaText);
    const provMap = parseProvenance(provText);
    const version = target.tag_name.replace(/^v/, '');

    const manifest = {
      repository: REPO,
      tag: target.tag_name,
      version,
      name: target.name,
      isPrerelease: Boolean(target.prerelease),
      publishedAt: target.published_at,
      htmlUrl: target.html_url,
      signingStatus: provMap.signing_status || 'UNSIGNED_DEVELOPMENT_ARTIFACT',
      commitSha: provMap.commit || null,
      workflowRunUrl: provMap.workflow_run || null,
      syncStatus: 'live-github-api',
      syncedAt: new Date().toISOString(),
      sha256SumsUrl: shaAsset?.browser_download_url || null,
      provenanceUrl: provAsset?.browser_download_url || null,
      assets: target.assets
        .filter((a) => a.name !== 'SHA256SUMS.txt' && a.name !== 'PROVENANCE.txt')
        .map((a) => ({
          name: a.name,
          size: a.size,
          downloadUrl: a.browser_download_url,
          sha256: shaMap[a.name] || null
        }))
    };

    fs.writeFileSync(outPath, JSON.stringify(manifest, null, 2) + '\n', 'utf8');
    console.log(`Synced release ${manifest.tag} (${manifest.assets.length} assets) -> src/data/release.json`);
  } catch (err) {
    if (fs.existsSync(outPath)) {
      const cached = JSON.parse(fs.readFileSync(outPath, 'utf8'));
      cached.syncStatus = 'cached-fallback';
      fs.writeFileSync(outPath, JSON.stringify(cached, null, 2) + '\n', 'utf8');
      console.warn(`Release API unavailable (${err.message}); using cached manifest for ${cached.tag}`);
      return;
    }
    throw err;
  }
}

await run();
