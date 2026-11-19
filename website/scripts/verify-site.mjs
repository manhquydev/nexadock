import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(__dirname, '..');
const distDir = path.join(rootDir, 'dist');
const screenshotDir = path.join(rootDir, '.qa-screenshots');

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.mjs': 'application/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.txt': 'text/plain; charset=utf-8',
  '.xml': 'application/xml; charset=utf-8',
  '.webp': 'image/webp',
  '.woff2': 'font/woff2'
};

function findBrowserExecutable() {
  const candidates = [
    process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
    process.env.CHROME_PATH,
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
    'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    '/usr/bin/google-chrome',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge'
  ].filter(Boolean);

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  throw new Error(`No Chromium/Chrome/Edge executable found. Checked: ${candidates.join(', ')}`);
}

function startStaticServer() {
  if (!fs.existsSync(distDir)) {
    throw new Error(`dist directory not found at ${distDir}. Run npm run build first.`);
  }

  const server = http.createServer((req, res) => {
    const reqUrl = new URL(req.url || '/', 'http://127.0.0.1');
    let pathname = decodeURIComponent(reqUrl.pathname);

    if (!pathname.startsWith('/nexadock')) {
      res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end('Not under /nexadock');
      return;
    }

    let relPath = pathname.slice('/nexadock'.length);
    if (relPath === '' || relPath === '/') {
      relPath = '/index.html';
    } else if (relPath.endsWith('/')) {
      relPath += 'index.html';
    }

    const safeRel = path.normalize(relPath).replace(/^(\.\.(\/|\\|$))+/, '');
    let filePath = path.join(distDir, safeRel);
    if (fs.existsSync(filePath) && fs.statSync(filePath).isDirectory()) {
      filePath = path.join(filePath, 'index.html');
    }

    if (!fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
      const notFoundPath = path.join(distDir, '404.html');
      res.writeHead(404, { 'Content-Type': 'text/html; charset=utf-8' });
      res.end(fs.existsSync(notFoundPath) ? fs.readFileSync(notFoundPath) : '404 Not Found');
      return;
    }

    const ext = path.extname(filePath).toLowerCase();
    res.writeHead(200, { 'Content-Type': MIME_TYPES[ext] || 'application/octet-stream' });
    fs.createReadStream(filePath).pipe(res);
  });

  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address();
      resolve({
        server,
        baseUrl: `http://127.0.0.1:${addr.port}/nexadock`
      });
    });
  });
}

async function main() {
  const liveBase = process.env.SITE_BASE_URL ? process.env.SITE_BASE_URL.replace(/\/$/, '') : null;
  let serverInfo = null;
  const baseUrl = liveBase || (serverInfo = await startStaticServer()).baseUrl;

  fs.mkdirSync(screenshotDir, { recursive: true });
  const executablePath = findBrowserExecutable();
  const browser = await chromium.launch({
    executablePath,
    headless: true,
    args: ['--no-sandbox', '--disable-gpu']
  });

  try {
    // 1. Verify static metadata files
    for (const staticPath of ['/robots.txt', '/sitemap-index.xml', '/favicon.svg', '/og-preview.png']) {
      const res = await fetch(`${baseUrl}${staticPath}`);
      assert.equal(res.status, 200, `Expected HTTP 200 for ${baseUrl}${staticPath}, got ${res.status}`);
    }

    // 2. Verify all pages at multiple viewports
    const routes = ['/', '/en/', '/docs/', '/en/docs/'];
    const viewports = [
      { name: 'mobile-390', width: 390, height: 844 },
      { name: 'tablet-768', width: 768, height: 1024 },
      { name: 'laptop-1024', width: 1024, height: 768 },
      { name: 'desktop-1440', width: 1440, height: 900 }
    ];

    for (const route of routes) {
      for (const vp of viewports) {
        const context = await browser.newContext({ viewport: { width: vp.width, height: vp.height } });
        const page = await context.newPage();
        const consoleErrors = [];
        const pageErrors = [];
        const failedAssets = [];

        page.on('console', (msg) => {
          if (msg.type() === 'error') consoleErrors.push(msg.text());
        });
        page.on('pageerror', (err) => pageErrors.push(err.message));
        page.on('requestfailed', (req) => {
          failedAssets.push(`${req.url()} (${req.failure()?.errorText || 'failed'})`);
        });
        page.on('response', (res) => {
          if (res.status() >= 400 && !res.url().includes('404')) {
            failedAssets.push(`${res.url()} (HTTP ${res.status()})`);
          }
        });

        const url = `${baseUrl}${route}`;
        const response = await page.goto(url, { waitUntil: 'networkidle' });
        assert.equal(response?.status(), 200, `Expected HTTP 200 for ${url}`);

        // Scroll each image into view so loading="lazy" images are fetched and decoded
        for (const img of await page.locator('img').all()) {
          await img.scrollIntoViewIfNeeded();
        }
        await page.evaluate(() => window.scrollTo(0, 0));
        await page.waitForLoadState('networkidle');
        await page.waitForTimeout(300);

        const audit = await page.evaluate(() => {
          const docEl = document.documentElement;
          const imgs = Array.from(document.querySelectorAll('img'));
          const brokenImgs = imgs
            .filter((img) => !img.complete || img.naturalWidth === 0)
            .map((img) => img.getAttribute('src'));
          const h1 = document.querySelector('h1');
          const h1Style = h1 ? getComputedStyle(h1) : null;
          const bodyStyle = getComputedStyle(document.body);
          const links = Array.from(document.querySelectorAll('a[href]')).map((a) => a.getAttribute('href'));
          return {
            title: document.title,
            lang: docEl.lang,
            scrollWidth: docEl.scrollWidth,
            innerWidth: window.innerWidth,
            imgCount: imgs.length,
            brokenImgs,
            h1Text: h1?.textContent?.trim() || '',
            h1FontSize: h1Style?.fontSize || '',
            bodyFontSize: bodyStyle.fontSize,
            hasReleaseLink: links.some((href) => href && href.includes('v0.1.1-beta.2')),
            textSample: document.body.innerText.slice(0, 400)
          };
        });

        assert.equal(consoleErrors.length, 0, `${url} [${vp.name}] console errors: ${consoleErrors.join('; ')}`);
        assert.equal(pageErrors.length, 0, `${url} [${vp.name}] page errors: ${pageErrors.join('; ')}`);
        assert.equal(failedAssets.length, 0, `${url} [${vp.name}] failed assets: ${failedAssets.join('; ')}`);
        assert.equal(audit.brokenImgs.length, 0, `${url} [${vp.name}] broken images: ${audit.brokenImgs.join(', ')}`);
        assert.ok(
          audit.scrollWidth <= audit.innerWidth,
          `${url} [${vp.name}] horizontal overflow: scrollWidth=${audit.scrollWidth} > innerWidth=${audit.innerWidth}`
        );
        assert.ok(audit.title.includes('NexaDock'), `${url} missing NexaDock in title: ${audit.title}`);
        assert.ok(audit.h1Text.length > 0, `${url} missing h1`);
        if (route === '/' || route === '/en/') {
          assert.ok(audit.hasReleaseLink, `${url} missing v0.1.1-beta.2 release link`);
        }

        if (vp.name === 'desktop-1440' || (vp.name === 'mobile-390' && route === '/')) {
          const slug = route === '/' ? 'home-vi' : route.replace(/^\/|\/$/g, '').replace(/\//g, '-');
          await page.screenshot({
            path: path.join(screenshotDir, `${slug}-${vp.name}.png`),
            fullPage: true
          });
        }

        await context.close();
      }
    }

    // 3. Interactive checks on `/` (showcase tabs, download platform tabs, FAQ, mobile menu, reduced-motion)
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      reducedMotion: 'reduce'
    });
    const page = await context.newPage();
    await page.goto(`${baseUrl}/`, { waitUntil: 'networkidle' });

    // Showcase tab switch
    const personalTab = page.locator('[data-showcase-tab="personal"]');
    await personalTab.click();
    assert.equal(await personalTab.getAttribute('aria-selected'), 'true');
    const personalPanel = page.locator('[data-showcase-panel="personal"]');
    assert.equal(await personalPanel.isVisible(), true);

    // Download platform tab switch
    const macTab = page.locator('[data-dl-tab="macos"]');
    await macTab.click();
    assert.equal(await macTab.getAttribute('aria-selected'), 'true');
    assert.equal(await page.locator('[data-dl-panel="macos"]').isVisible(), true);
    assert.equal(await page.locator('[data-dl-panel="windows"]').isVisible(), false);

    // FAQ details toggle
    const firstFaq = page.locator('.faq-item').first();
    await firstFaq.locator('summary').click();
    assert.equal(await firstFaq.getAttribute('open'), '');

    await context.close();

    // Mobile drawer toggle at 390px
    const mobileContext = await browser.newContext({ viewport: { width: 390, height: 844 } });
    const mobilePage = await mobileContext.newPage();
    await mobilePage.goto(`${baseUrl}/`, { waitUntil: 'networkidle' });
    const menuBtn = mobilePage.locator('#mobile-menu-toggle');
    await menuBtn.click();
    assert.equal(await menuBtn.getAttribute('aria-expanded'), 'true');
    assert.equal(await mobilePage.locator('#mobile-drawer').isVisible(), true);
    await menuBtn.click();
    assert.equal(await menuBtn.getAttribute('aria-expanded'), 'false');
    await mobileContext.close();

    // 4. Verify 404 page
    const notFoundContext = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const notFoundPage = await notFoundContext.newPage();
    const notFoundRes = await notFoundPage.goto(`${baseUrl}/non-existent-route/`, { waitUntil: 'networkidle' });
    assert.equal(notFoundRes?.status(), 404);
    await notFoundContext.close();

    console.log(`ALL WEBSITE VERIFICATION CHECKS PASSED (${liveBase ? 'LIVE' : 'LOCAL'}: ${baseUrl})`);
  } finally {
    await browser.close();
    if (serverInfo) {
      await new Promise((resolve) => serverInfo.server.close(resolve));
    }
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
