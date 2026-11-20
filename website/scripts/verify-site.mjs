import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(__dirname, '..');
const distDir = path.join(rootDir, 'dist');
const releasePath = path.join(rootDir, 'src', 'data', 'release.json');
const axeSourcePath = path.join(rootDir, 'node_modules', 'axe-core', 'axe.min.js');
const screenshotDir = process.env.QA_SCREENSHOT_DIR || path.join(rootDir, '.qa-screenshots');

const release = JSON.parse(fs.readFileSync(releasePath, 'utf8'));
const axeSource = fs.readFileSync(axeSourcePath, 'utf8');

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

async function runContrastAudit(page) {
  return page.evaluate(() => {
    const parseColor = (css) => {
      const m = css.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\)/);
      if (!m) return null;
      return {
        r: Number(m[1]),
        g: Number(m[2]),
        b: Number(m[3]),
        a: m[4] === undefined ? 1 : Number(m[4])
      };
    };

    const blend = (fg, bg) => {
      const a = fg.a;
      return {
        r: fg.r * a + bg.r * (1 - a),
        g: fg.g * a + bg.g * (1 - a),
        b: fg.b * a + bg.b * (1 - a),
        a: 1
      };
    };

    const luminance = (c) => {
      const toLinear = (v) => {
        const s = v / 255;
        return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * toLinear(c.r) + 0.7152 * toLinear(c.g) + 0.0722 * toLinear(c.b);
    };

    const contrastRatio = (c1, c2) => {
      const l1 = luminance(c1);
      const l2 = luminance(c2);
      const hi = Math.max(l1, l2);
      const lo = Math.min(l1, l2);
      return (hi + 0.05) / (lo + 0.05);
    };

    const getEffectiveBg = (el) => {
      const stack = [];
      let cur = el;
      while (cur && cur.nodeType === Node.ELEMENT_NODE) {
        const bg = parseColor(getComputedStyle(cur).backgroundColor);
        if (bg && bg.a > 0) stack.unshift(bg);
        cur = cur.parentElement;
      }
      let out = { r: 246, g: 245, b: 242, a: 1 };
      for (const layer of stack) {
        out = layer.a >= 1 ? layer : blend(layer, out);
      }
      return out;
    };

    const failures = [];
    const checked = new Set();
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    let node;
    while ((node = walker.nextNode())) {
      if (!node.textContent || !node.textContent.trim()) continue;
      const el = node.parentElement;
      if (!el || checked.has(el)) continue;
      checked.add(el);

      const style = getComputedStyle(el);
      if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) continue;
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      if (el.closest('.sr-only, .skip-link') || el.closest('[hidden]')) continue;

      const fg = parseColor(style.color);
      if (!fg) continue;
      const bg = getEffectiveBg(el);
      const effectiveFg = fg.a < 1 ? blend(fg, bg) : fg;
      const ratio = contrastRatio(effectiveFg, bg);
      const fontSize = parseFloat(style.fontSize);
      const fontWeight = parseInt(style.fontWeight, 10) || 400;
      const isLarge = fontSize >= 24 || (fontSize >= 18.66 && fontWeight >= 700);
      const minRatio = isLarge ? 3.0 : 4.5;

      if (ratio < minRatio) {
        failures.push({
          tag: el.tagName.toLowerCase(),
          text: node.textContent.trim().slice(0, 60),
          ratio: Number(ratio.toFixed(2)),
          minRatio,
          fg: style.color,
          bg: `rgb(${Math.round(bg.r)}, ${Math.round(bg.g)}, ${Math.round(bg.b)})`
        });
      }
    }

    return { checkedCount: checked.size, failures };
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

  const legacyForbidden = [
    'KHOANG',
    'Multi-Zalo Desktop',
    'MultiZaloDesktop',
    'com.multizalodesktop.manager',
    'khoang-preferences.json',
    'account-dock-'
  ];

  const a11ySummary = [];

  try {
    // 1. Verify static metadata files
    for (const staticPath of ['/robots.txt', '/sitemap-index.xml', '/favicon.svg', '/og-preview.png']) {
      const res = await fetch(`${baseUrl}${staticPath}`);
      assert.equal(res.status, 200, `Expected HTTP 200 for ${baseUrl}${staticPath}, got ${res.status}`);
    }

    // 2. Verify all pages at multiple viewports
    const routes = ['/', '/en/', '/docs/', '/en/docs/'];
    const viewports = [
      { name: 'mobile-320', width: 320, height: 568 },
      { name: 'mobile-360', width: 360, height: 800 },
      { name: 'mobile-390', width: 390, height: 844 },
      { name: 'tablet-768', width: 768, height: 1024 },
      { name: 'laptop-1024', width: 1024, height: 768 },
      { name: 'desktop-1440', width: 1440, height: 900 },
      { name: 'wide-1920', width: 1920, height: 1080 }
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
        await page.waitForTimeout(200);

        const audit = await page.evaluate(() => {
          const docEl = document.documentElement;
          const imgs = Array.from(document.querySelectorAll('img'));
          const brokenImgs = imgs
            .filter((img) => !img.complete || img.naturalWidth === 0)
            .map((img) => img.getAttribute('src'));
          const h1 = document.querySelector('h1');
          const links = Array.from(document.querySelectorAll('a[href]')).map((a) => a.getAttribute('href'));
          const hreflangs = Array.from(document.querySelectorAll('link[rel="alternate"][hreflang]')).map((el) =>
            el.getAttribute('hreflang')
          );
          const ogLocale = document.querySelector('meta[property="og:locale"]')?.getAttribute('content') || '';
          const ogSiteName = document.querySelector('meta[property="og:site_name"]')?.getAttribute('content') || '';
          return {
            title: document.title,
            lang: docEl.lang,
            scrollWidth: docEl.scrollWidth,
            innerWidth: window.innerWidth,
            imgCount: imgs.length,
            brokenImgs,
            h1Text: h1?.textContent?.trim() || '',
            links,
            hreflangs,
            ogLocale,
            ogSiteName,
            html: document.documentElement.outerHTML
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
        assert.ok(audit.hreflangs.includes('vi') && audit.hreflangs.includes('en') && audit.hreflangs.includes('x-default'), `${url} missing hreflang links`);
        assert.equal(audit.ogSiteName, 'NexaDock', `${url} missing og:site_name`);
        assert.ok(audit.ogLocale === 'vi_VN' || audit.ogLocale === 'en_US', `${url} invalid og:locale: ${audit.ogLocale}`);

        for (const term of legacyForbidden) {
          if (term === 'MultiZaloDesktop' && (route === '/docs/' || route === '/en/docs/')) {
            continue;
          }
          assert.equal(
            audit.html.includes(term),
            false,
            `${url} [${vp.name}] still contains legacy reference: ${term}`
          );
        }

        if (route === '/' || route === '/en/') {
          const hasReleaseLink = audit.links.some((href) => href && href.includes(`/releases/tag/${release.tag}`));
          assert.ok(hasReleaseLink, `${url} missing /releases/tag/${release.tag} release link`);

          const bpAudit = await page.evaluate(() => {
            const card = document.getElementById('isolation-blueprint');
            const desktopSvg = card?.querySelector('.bp-svg-desktop');
            const mobileSvg = card?.querySelector('.bp-svg-mobile');
            const cardRect = card?.getBoundingClientRect();
            const dRect = desktopSvg?.getBoundingClientRect();
            const mRect = mobileSvg?.getBoundingClientRect();
            return {
              hasCard: Boolean(card),
              cardRight: cardRect?.right || 0,
              cardScrollW: card?.scrollWidth || 0,
              cardClientW: card?.clientWidth || 0,
              desktopWidth: dRect?.width || 0,
              desktopRight: dRect?.right || 0,
              mobileWidth: mRect?.width || 0,
              mobileRight: mRect?.right || 0,
              innerWidth: window.innerWidth
            };
          });
          assert.ok(bpAudit.hasCard, `${url} [${vp.name}] missing #isolation-blueprint`);
          assert.ok(
            bpAudit.cardRight <= bpAudit.innerWidth + 1 && bpAudit.cardScrollW <= bpAudit.cardClientW + 1,
            `${url} [${vp.name}] #isolation-blueprint overflows viewport: ${JSON.stringify(bpAudit)}`
          );
          if (vp.width <= 767) {
            assert.ok(bpAudit.mobileWidth > 0 && bpAudit.mobileRight <= bpAudit.innerWidth + 1, `${url} [${vp.name}] mobile blueprint SVG invalid: ${JSON.stringify(bpAudit)}`);
            assert.equal(bpAudit.desktopWidth, 0, `${url} [${vp.name}] desktop blueprint SVG must be hidden on <=767px`);
          } else {
            assert.ok(bpAudit.desktopWidth > 0 && bpAudit.desktopRight <= bpAudit.innerWidth + 1, `${url} [${vp.name}] desktop blueprint SVG invalid: ${JSON.stringify(bpAudit)}`);
            assert.equal(bpAudit.mobileWidth, 0, `${url} [${vp.name}] mobile blueprint SVG must be hidden on >=768px`);
          }
        }

        // Axe-core + computed contrast check on mobile-320, mobile-390 and desktop-1440
        if (vp.name === 'mobile-320' || vp.name === 'mobile-390' || vp.name === 'desktop-1440') {
          await page.addScriptTag({ content: axeSource });
          const axeResults = await page.evaluate(async () => {
            return window.axe.run(document, {
              runOnly: {
                type: 'tag',
                values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa']
              }
            });
          });
          assert.equal(
            axeResults.violations.length,
            0,
            `${url} [${vp.name}] axe-core violations: ${JSON.stringify(
              axeResults.violations.map((v) => ({ id: v.id, impact: v.impact, nodes: v.nodes.length }))
            )}`
          );

          const contrast = await runContrastAudit(page);
          assert.equal(
            contrast.failures.length,
            0,
            `${url} [${vp.name}] contrast failures: ${JSON.stringify(contrast.failures)}`
          );

          a11ySummary.push({
            route,
            viewport: vp.name,
            axePasses: axeResults.passes.length,
            axeViolations: axeResults.violations.length,
            contrastChecked: contrast.checkedCount,
            contrastFailures: contrast.failures.length
          });
        }

        if (vp.name === 'desktop-1440' || (route === '/' && vp.name !== 'mobile-360')) {
          const slug = route === '/' ? 'home-vi' : route.replace(/^\/|\/$/g, '').replace(/\//g, '-');
          await page.screenshot({
            path: path.join(screenshotDir, `${slug}-${vp.name}.png`),
            fullPage: true
          });
        }

        await context.close();
      }
    }

    // 3. Interactive & keyboard checks on `/`
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      reducedMotion: 'reduce'
    });
    const page = await context.newPage();
    await page.goto(`${baseUrl}/`, { waitUntil: 'networkidle' });

    // Showcase tab click + keyboard navigation + Open/Close toggle parity
    const primaryPanel = page.locator('[data-showcase-panel="primary"]');
    assert.equal(await primaryPanel.locator('[data-toggle-session]').count(), 0, 'Primary Zalo must not expose Open/Close toggle');
    const primaryDotBefore = await page.locator('[data-rail-dot="primary"]').getAttribute('class');
    const primaryTextBefore = await page.locator('[data-stage-state-text="primary"]').textContent();

    const personalTab = page.locator('[data-showcase-tab="personal"]');
    await personalTab.click();
    assert.equal(await personalTab.getAttribute('aria-selected'), 'true');
    assert.equal(await page.locator('[data-showcase-panel="personal"]').isVisible(), true);

    const personalToggle = page.locator('[data-toggle-session="personal"]');
    assert.equal(await personalToggle.getAttribute('aria-pressed'), 'false');
    assert.ok((await page.locator('[data-rail-dot="personal"]').getAttribute('class'))?.includes('is-stopped'));

    await personalToggle.click();
    assert.equal(await personalToggle.getAttribute('aria-pressed'), 'true');
    assert.ok((await personalToggle.textContent())?.includes('Đóng Zalo'));
    assert.ok((await page.locator('[data-rail-dot="personal"]').getAttribute('class'))?.includes('is-running'));
    assert.ok((await page.locator('[data-stage-dot="personal"]').getAttribute('class'))?.includes('is-running'));
    assert.equal(await page.locator('[data-stage-state-text="personal"]').textContent(), 'Cửa sổ đang mở');

    await personalToggle.click();
    assert.equal(await personalToggle.getAttribute('aria-pressed'), 'false');
    assert.ok((await personalToggle.textContent())?.includes('Mở Zalo'));
    assert.ok((await page.locator('[data-rail-dot="personal"]').getAttribute('class'))?.includes('is-stopped'));
    assert.equal(await page.locator('[data-stage-state-text="personal"]').textContent(), 'Đã đóng');

    assert.equal(await page.locator('[data-rail-dot="primary"]').getAttribute('class'), primaryDotBefore);
    assert.equal(await page.locator('[data-stage-state-text="primary"]').textContent(), primaryTextBefore);

    await personalTab.focus();
    await page.keyboard.press('ArrowDown');
    assert.equal(await page.locator('[data-showcase-tab="sales"]').getAttribute('aria-selected'), 'true');

    // Download platform tab click + keyboard navigation
    const macTab = page.locator('[data-dl-tab="macos"]');
    await macTab.click();
    assert.equal(await macTab.getAttribute('aria-selected'), 'true');
    assert.equal(await page.locator('[data-dl-panel="macos"]').isVisible(), true);
    assert.equal(await page.locator('[data-dl-panel="windows"]').isVisible(), false);
    await macTab.focus();
    await page.keyboard.press('ArrowRight');
    assert.equal(await page.locator('[data-dl-tab="ubuntu"]').getAttribute('aria-selected'), 'true');

    // FAQ details toggle
    const firstFaq = page.locator('.faq-item').first();
    await firstFaq.locator('summary').click();
    assert.equal(await firstFaq.getAttribute('open'), '');

    await context.close();

    // Mobile drawer toggle + Escape key dismissal + focus return at 390px
    const mobileContext = await browser.newContext({ viewport: { width: 390, height: 844 } });
    const mobilePage = await mobileContext.newPage();
    await mobilePage.goto(`${baseUrl}/`, { waitUntil: 'networkidle' });
    const menuBtn = mobilePage.locator('#mobile-menu-toggle');
    await menuBtn.click();
    assert.equal(await menuBtn.getAttribute('aria-expanded'), 'true');
    assert.equal(await mobilePage.locator('#mobile-drawer').isVisible(), true);

    await mobilePage.keyboard.press('Escape');
    assert.equal(await menuBtn.getAttribute('aria-expanded'), 'false');
    assert.equal(await mobilePage.locator('#mobile-drawer').isVisible(), false);
    const activeId = await mobilePage.evaluate(() => document.activeElement?.id || '');
    assert.equal(activeId, 'mobile-menu-toggle', 'Escape must return focus to #mobile-menu-toggle');
    await mobileContext.close();

    // 4. Verify 404 page + noindex meta
    const notFoundContext = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const notFoundPage = await notFoundContext.newPage();
    const notFoundRes = await notFoundPage.goto(`${baseUrl}/non-existent-route/`, { waitUntil: 'networkidle' });
    assert.equal(notFoundRes?.status(), 404);
    const robotsMeta = await notFoundPage.locator('meta[name="robots"]').getAttribute('content');
    assert.equal(robotsMeta, 'noindex', '404 page must include <meta name="robots" content="noindex" />');
    await notFoundContext.close();

    console.log('AXE_AND_CONTRAST_SUMMARY:', JSON.stringify(a11ySummary));
    console.log(`ALL WEBSITE VERIFICATION CHECKS PASSED (${liveBase ? 'LIVE' : 'LOCAL'}: ${baseUrl}, release: ${release.tag})`);
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
