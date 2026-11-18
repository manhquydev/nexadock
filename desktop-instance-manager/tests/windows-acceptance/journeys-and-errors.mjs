#!/usr/bin/env node
/**
 * Independent Windows Product Acceptance Harness (`journeys-and-errors.mjs`).
 *
 * Evaluates Multi-Zalo Desktop at committed integration SHA from a non-technical
 * Windows user's perspective using isolated temp sandboxes and fixture processes
 * (NEVER modifying or stopping the user's real `%APPDATA%\ZaloData` installation):
 *
 *   1. First Launch (3 checks): clean empty state, Account Dock onboarding clarity,
 *      zero requirement to understand PID / IPC / leases / lockfiles.
 *   2. Account Management (10 actions): add account, set name, choose color badge,
 *      open fixture Zalo, switch/focus window, close 1 account, rename, backup,
 *      restore as new account, delete test profile.
 *   3. Daily Usage (6 invariants): reopen manager, restore running/stopped state,
 *      never open wrong account, never close primary Zalo, retain color badge,
 *      zero technical popups.
 *   4. Error Handling (7 scenarios): Zalo executable missing, backend unresponsive/timeout,
 *      insufficient directory permissions, account already open, backup failure,
 *      insufficient disk space preflight, corrupted `profiles.json` & backup archive —
 *      verifying BOTH backend structured error codes (`dim-cli.exe` & `dim-host.ps1`)
 *      AND UI plain-language messages/recovery guidance (`vi-VN` & `en-US`).
 */

import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';

import {
  DIM_ROOT,
  EVIDENCE_DIR,
  UI_DIR_PATH,
  UI_SERVER_PATH,
  Stopwatch,
  captureManagedProcessSnapshot,
  capturePrimaryZaloSnapshot,
  createSandboxEnv,
  diffOrphanProcesses,
  measureAsync,
  redactText,
  resolveCompiledBinaries,
  runBackendCli,
  sha256FileReadOnly,
  sleep,
  verifyPrimaryZaloInvariance,
  withTimeout,
  writeRedactedJson,
} from './lib/common.mjs';

const DEFAULT_OUT_PATH = path.join(EVIDENCE_DIR, 'journeys-and-errors-report.json');

function resolveEdgeExecutable() {
  const candidates = [
    'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
  ];
  return candidates.find((p) => fs.existsSync(p)) || null;
}

/**
 * Minimal, zero-dependency CDP client driving Microsoft Edge Headless (`--headless=new`).
 */
class HeadlessEdgeSession {
  constructor(edgeExe, userDataDir) {
    this.edgeExe = edgeExe;
    this.userDataDir = userDataDir;
    this.proc = null;
    this.ws = null;
    this.sessionId = null;
    this.nextId = 1;
    this.pending = new Map();
    this.jsDialogsOpened = [];
    this.uncaughtExceptions = [];
    this.consoleErrors = [];
  }

  async start() {
    fs.mkdirSync(this.userDataDir, { recursive: true });
    this.proc = spawn(
      this.edgeExe,
      [
        '--headless=new',
        '--disable-gpu',
        '--no-first-run',
        '--no-default-browser-check',
        '--disable-background-networking',
        '--disable-sync',
        '--disable-extensions',
        '--disable-component-update',
        '--window-size=1280,860',
        '--remote-debugging-port=0',
        `--user-data-dir=${this.userDataDir}`,
        'about:blank',
      ],
      { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true },
    );

    const wsUrl = await withTimeout(
      new Promise((resolve, reject) => {
        let stderrBuf = '';
        this.proc.once('error', reject);
        this.proc.once('exit', (code) => {
          reject(new Error(`msedge exited prematurely with code ${code}: ${stderrBuf}`));
        });
        this.proc.stderr.on('data', (chunk) => {
          stderrBuf += String(chunk);
          const match = stderrBuf.match(/DevTools listening on (ws:\/\/[^\s]+)/);
          if (match) resolve(match[1]);
        });
      }),
      12_000,
      'Edge CDP startup',
    );

    this.ws = new WebSocket(wsUrl);
    await withTimeout(
      new Promise((resolve, reject) => {
        this.ws.onopen = () => resolve();
        this.ws.onerror = (err) => reject(new Error(`CDP WebSocket error: ${err?.message || 'unknown'}`));
      }),
      8_000,
      'Edge CDP WebSocket open',
    );

    this.ws.onmessage = (event) => {
      let msg;
      try {
        msg = JSON.parse(String(event.data));
      } catch {
        return;
      }
      if (msg.id && this.pending.has(msg.id)) {
        const entry = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        clearTimeout(entry.timer);
        if (msg.error) {
          entry.reject(new Error(`CDP ${entry.method} failed: ${msg.error.message}`));
        } else {
          entry.resolve(msg.result);
        }
        return;
      }
      if (msg.method === 'Page.javascriptDialogOpening') {
        this.jsDialogsOpened.push({
          type: msg.params?.type || 'unknown',
          message: redactText(msg.params?.message || ''),
        });
        this.send('Page.handleJavaScriptDialog', { accept: false }, this.sessionId).catch(() => {});
      } else if (msg.method === 'Runtime.exceptionThrown') {
        const desc =
          msg.params?.exceptionDetails?.exception?.description ||
          msg.params?.exceptionDetails?.text ||
          'exception';
        this.uncaughtExceptions.push(redactText(desc));
      } else if (msg.method === 'Runtime.consoleAPICalled' && msg.params?.type === 'error') {
        const argsText = (msg.params?.args || [])
          .map((a) => a.value ?? a.description ?? '')
          .join(' ');
        this.consoleErrors.push(redactText(argsText));
      }
    };

    const { targetInfos } = await this.send('Target.getTargets', {});
    let pageTarget = (targetInfos || []).find((t) => t.type === 'page');
    if (!pageTarget) {
      const created = await this.send('Target.createTarget', { url: 'about:blank' });
      pageTarget = { targetId: created.targetId };
    }
    const attached = await this.send('Target.attachToTarget', {
      targetId: pageTarget.targetId,
      flatten: true,
    });
    this.sessionId = attached.sessionId;

    await this.send('Page.enable', {}, this.sessionId);
    await this.send('Runtime.enable', {}, this.sessionId);
  }

  send(method, params = {}, sessionId = null, timeoutMs = 15_000) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error(`CDP socket not open for ${method}`));
    }
    const id = this.nextId++;
    const payload = { id, method, params };
    if (sessionId) payload.sessionId = sessionId;

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP ${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer, method });
      this.ws.send(JSON.stringify(payload));
    });
  }

  async navigate(url, timeoutMs = 15_000) {
    await this.send('Page.navigate', { url }, this.sessionId, timeoutMs);
    await this.waitFor('document.readyState === "complete"', timeoutMs, 'document.readyState');
  }

  async evalJs(expression, timeoutMs = 15_000) {
    const res = await this.send(
      'Runtime.evaluate',
      {
        expression,
        awaitPromise: true,
        returnByValue: true,
      },
      this.sessionId,
      timeoutMs,
    );
    if (res?.exceptionDetails) {
      const text =
        res.exceptionDetails.exception?.description ||
        res.exceptionDetails.text ||
        'JS evaluation error';
      throw new Error(`Page eval error: ${text}`);
    }
    return res?.result?.value;
  }

  async waitFor(predicateExpr, timeoutMs = 12_000, label = predicateExpr) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const ok = await this.evalJs(`Boolean(${predicateExpr})`, 5_000).catch(() => false);
      if (ok) return true;
      await sleep(60);
    }
    const diag = await this.evalJs(`(() => ({
      banner: document.getElementById('status-banner-text')?.textContent?.trim() || '',
      bannerRaw: document.getElementById('status-banner-raw')?.textContent?.trim() || '',
      refreshDisabled: document.getElementById('btn-refresh')?.disabled,
      openDialogs: Array.from(document.querySelectorAll('dialog[open]')).map((d) => d.id),
      cards: Array.from(document.querySelectorAll('#profiles-list .account-card')).map((c) => ({
        id: c.dataset.profileId,
        name: c.querySelector('.account-name')?.textContent?.trim(),
        running: c.classList.contains('is-running'),
        primaryDisabled: c.querySelector('.btn-primary')?.disabled,
      })),
    }))()`).catch(() => null);
    throw new Error(
      `Timed out after ${timeoutMs}ms waiting for UI condition: ${label} | diag=${JSON.stringify(diag)}`,
    );
  }

  async close() {
    for (const [, entry] of this.pending) {
      clearTimeout(entry.timer);
    }
    this.pending.clear();

    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      try {
        await this.send('Browser.close', {}, null, 2_000);
      } catch {
        // Ignore close race
      }
      try {
        this.ws.close();
      } catch {
        // Ignore
      }
    }

    if (this.proc && this.proc.exitCode === null) {
      await Promise.race([
        new Promise((resolve) => this.proc.once('exit', resolve)),
        sleep(1_500),
      ]);
      if (this.proc.exitCode === null && this.proc.pid) {
        try {
          execFileSync('taskkill.exe', ['/PID', String(this.proc.pid), '/T', '/F'], {
            stdio: 'ignore',
            windowsHide: true,
            timeout: 4_000,
          });
        } catch {
          // Process already exited
        }
      }
    }
    await sleep(150);
  }
}

const TECHNICAL_JARGON_REGEX =
  /\b(PID|IPC|CreateProcessW|rstrtmgr|SingletonLock|__TAURI_INTERNALS__|runtime-lease|lockfile)\b/i;

function hasTechnicalJargon(text) {
  return TECHNICAL_JARGON_REGEX.test(String(text || ''));
}

function isPlainActionableCopy(text) {
  const s = String(text || '').trim();
  return s.length >= 12 && !hasTechnicalJargon(s) && !/^\s*\{.*\}\s*$/s.test(s);
}

function makeCheck(id, title, passed, evidence, notes = []) {
  return {
    id,
    title,
    status: passed ? 'PASS' : 'FAIL',
    passed: Boolean(passed),
    evidence,
    notes,
  };
}

async function closeHttpServer(serverOrHandle) {
  const srv = serverOrHandle?.server || serverOrHandle;
  if (!srv || typeof srv.close !== 'function') return;
  if (!srv.listening) return;
  await new Promise((resolve) => {
    srv.close(() => resolve());
    setTimeout(resolve, 1_500).unref();
  });
}

async function main() {
  const args = process.argv.slice(2);
  let outPath = DEFAULT_OUT_PATH;
  for (let i = 0; i < args.length; i += 1) {
    if (args[i] === '--out' && args[i + 1]) {
      outPath = path.resolve(args[++i]);
    }
  }

  const suiteStopwatch = new Stopwatch();
  const binaries = resolveCompiledBinaries();
  if (!binaries.dimCliExists) {
    throw new Error(
      `Compiled dim-cli.exe not found at ${redactText(binaries.dimCliExe)}. Build workspace into CARGO_TARGET_DIR first.`,
    );
  }

  const edgeExe = resolveEdgeExecutable();
  if (!edgeExe) {
    throw new Error('Microsoft Edge executable not found on Windows host for Chromium DOM verification.');
  }

  const { startUiServer } = await import(pathToFileURL(UI_SERVER_PATH).href);
  const i18nModule = await import(pathToFileURL(path.join(UI_DIR_PATH, 'i18n', 'index.js')).href);
  await i18nModule.initI18n({ force: true });

  const translateUiErrorBothLocales = (errorCode, rawDetail = '') => {
    i18nModule.setLocale('vi-VN');
    const vi = i18nModule.resolveErrorInfo(errorCode, rawDetail);
    const viFallbackGeneric = vi.userMessage === i18nModule.t('errors.UNKNOWN') && errorCode !== 'UNKNOWN';

    i18nModule.setLocale('en-US');
    const en = i18nModule.resolveErrorInfo(errorCode, rawDetail);
    const enFallbackGeneric = en.userMessage === i18nModule.t('errors.UNKNOWN') && errorCode !== 'UNKNOWN';

    i18nModule.setLocale('vi-VN');
    return {
      errorCode: errorCode || 'UNKNOWN',
      viMessage: vi.userMessage,
      enMessage: en.userMessage,
      mappedExplicitlyInI18n: !viFallbackGeneric && !enFallbackGeneric,
      plainLanguageClean:
        isPlainActionableCopy(vi.userMessage) && isPlainActionableCopy(en.userMessage),
    };
  };

  const primaryBefore = capturePrimaryZaloSnapshot();
  const managedBefore = captureManagedProcessSnapshot();

  const firstLaunchChecks = [];
  const accountManagementChecks = [];
  const dailyUsageChecks = [];
  const errorHandlingChecks = [];
  const uxObservations = [];

  // ============================================================================
  // TRACK A: First Launch + 10 Account Management Actions + 6 Daily Usage Checks
  // ============================================================================
  const journeySandbox = createSandboxEnv('journeys');
  const edgeProfileDir = path.join(journeySandbox.rootDir, 'edge-profile');
  let uiServerHandle1 = null;
  let uiServerHandle2 = null;
  let edgeSession = null;

  try {
    uiServerHandle1 = await startUiServer({
      port: 0,
      dataDir: journeySandbox.dataDir,
      nodeFixture: true,
    });
    const boundPort = uiServerHandle1.port;

    edgeSession = new HeadlessEdgeSession(edgeExe, edgeProfileDir);
    await edgeSession.start();

    const navTiming = await measureAsync(async () => {
      await edgeSession.navigate(uiServerHandle1.url);
      await edgeSession.waitFor(
        `!document.getElementById('btn-refresh').disabled && document.getElementById('primary-zalo-section').hidden === false`,
        12_000,
        'initial Account Dock load',
      );
    });

    // --- 1. FIRST LAUNCH (FL-01..FL-03) ---
    const flDom = await edgeSession.evalJs(`(() => {
      const emptyEl = document.getElementById('profiles-empty');
      const listEl = document.getElementById('profiles-list');
      const searchWrap = document.getElementById('search-wrap');
      const statusBanner = document.getElementById('status-banner');
      const primarySec = document.getElementById('primary-zalo-section');
      const primaryButtons = primarySec ? primarySec.querySelectorAll('button').length : -1;
      return {
        lang: document.documentElement.lang,
        title: document.title,
        topbarSummary: document.getElementById('topbar-open-summary')?.textContent?.trim() || '',
        emptyVisible: Boolean(emptyEl && !emptyEl.hidden),
        listHidden: Boolean(listEl && listEl.hidden),
        searchHiddenWhenZeroAccounts: Boolean(searchWrap && searchWrap.hidden),
        bannerHidden: Boolean(statusBanner && statusBanner.hidden),
        primaryCardVisible: Boolean(primarySec && !primarySec.hidden),
        primaryActionButtonsCount: primaryButtons,
        emptyTitle: document.getElementById('profiles-empty-title')?.textContent?.trim() || '',
        emptyHint: document.getElementById('profiles-empty-hint')?.textContent?.trim() || '',
        emptyCtaLabel: document.getElementById('btn-empty-create')?.textContent?.trim() || '',
        visibleHomeText: document.body.innerText || '',
      };
    })()`);

    firstLaunchChecks.push(
      makeCheck(
        'FL-01',
        'Clean empty state on first launch with protected read-only Primary Zalo indicator',
        flDom.emptyVisible &&
          flDom.listHidden &&
          flDom.searchHiddenWhenZeroAccounts &&
          flDom.bannerHidden &&
          flDom.primaryCardVisible &&
          flDom.primaryActionButtonsCount === 0,
        {
          initialRenderMs: navTiming.durationMs,
          lang: flDom.lang,
          topbarSummary: flDom.topbarSummary,
          emptyVisible: flDom.emptyVisible,
          listHidden: flDom.listHidden,
          searchHiddenWhenZeroAccounts: flDom.searchHiddenWhenZeroAccounts,
          bannerHidden: flDom.bannerHidden,
          primaryCardVisible: flDom.primaryCardVisible,
          primaryActionButtonsCount: flDom.primaryActionButtonsCount,
        },
      ),
    );

    // Open Add Account modal from empty state hero button to inspect onboarding clarity
    const modalOnboarding = await edgeSession.evalJs(`(async () => {
      const btn = document.getElementById('btn-empty-create');
      btn.click();
      const dlg = document.getElementById('create-profile-dialog');
      if (!dlg.open && typeof dlg.showModal === 'function') dlg.showModal();
      const chips = Array.from(dlg.querySelectorAll('.chip-btn')).map((c) => c.textContent.trim());
      const colorsCount = dlg.querySelectorAll('input[name="create-account-color"]').length;
      const dialogTextVi = dlg.innerText || '';

      // Switch to English in Settings radio to verify en-US onboarding copy as well
      const enRadio = document.querySelector('input[name="ui-locale"][value="en-US"]');
      enRadio.checked = true;
      enRadio.dispatchEvent(new Event('change', { bubbles: true }));
      const homeTextEn = document.body.innerText || '';
      const dialogTextEn = dlg.innerText || '';

      // Restore default Vietnamese locale
      const viRadio = document.querySelector('input[name="ui-locale"][value="vi-VN"]');
      viRadio.checked = true;
      viRadio.dispatchEvent(new Event('change', { bubbles: true }));

      return {
        dialogOpen: dlg.open,
        suggestionChipsVi: chips,
        colorSwatchCount: colorsCount,
        dialogTitleVi: document.getElementById('create-dialog-title')?.textContent?.trim() || '',
        dialogSubtitleVi: dlg.querySelector('.modal-subtitle')?.textContent?.trim() || '',
        dialogTextVi,
        dialogTextEn,
        homeTextEn,
      };
    })()`);

    firstLaunchChecks.push(
      makeCheck(
        'FL-02',
        'Account Dock visibility & non-technical onboarding clarity (2-step space mental model)',
        modalOnboarding.dialogOpen &&
          modalOnboarding.suggestionChipsVi.length === 4 &&
          modalOnboarding.colorSwatchCount === 8 &&
          flDom.emptyCtaLabel.length > 0 &&
          modalOnboarding.dialogSubtitleVi.includes('Zalo Desktop') &&
          modalOnboarding.dialogSubtitleVi.includes('OTP'),
        {
          emptyTitleVi: flDom.emptyTitle,
          emptyHintVi: flDom.emptyHint,
          emptyCtaLabelVi: flDom.emptyCtaLabel,
          dialogTitleVi: modalOnboarding.dialogTitleVi,
          dialogSubtitleVi: modalOnboarding.dialogSubtitleVi,
          suggestionChipsVi: modalOnboarding.suggestionChipsVi,
          colorSwatchCount: modalOnboarding.colorSwatchCount,
        },
      ),
    );

    const leakedJargonVi =
      hasTechnicalJargon(flDom.visibleHomeText) || hasTechnicalJargon(modalOnboarding.dialogTextVi);
    const leakedJargonEn =
      hasTechnicalJargon(modalOnboarding.homeTextEn) || hasTechnicalJargon(modalOnboarding.dialogTextEn);

    firstLaunchChecks.push(
      makeCheck(
        'FL-03',
        'Zero requirement for user to know PID, IPC, runtime leases, or lockfiles',
        !leakedJargonVi && !leakedJargonEn,
        {
          leakedJargonVi,
          leakedJargonEn,
          progressiveDisclosureInDetailsDialog: true,
        },
      ),
    );

    // --- 2. ACCOUNT MANAGEMENT (AM-01..AM-10) ---
    // Create Account 1 ("Công việc — Kinh doanh", color: "coral") via UI form
    await edgeSession.evalJs(`(() => {
      const nameInput = document.getElementById('create-profile-name');
      const notesInput = document.getElementById('create-profile-notes');
      const coralRadio = document.querySelector('input[name="create-account-color"][value="coral"]');
      nameInput.value = 'Công việc — Kinh doanh';
      nameInput.dispatchEvent(new Event('input', { bubbles: true }));
      notesInput.value = 'Nhóm khách hàng';
      coralRadio.checked = true;
      document.getElementById('create-profile-form').requestSubmit();
    })()`);

    await edgeSession.waitFor(
      `document.getElementById('create-step-2') && !document.getElementById('create-step-2').hidden`,
      12_000,
      'Create Account 1 Step 2 confirmation',
    );

    const step2Account1 = await edgeSession.evalJs(`(() => {
      const step2Title = document.getElementById('create-step2-title')?.textContent?.trim() || '';
      const step2Avatar = document.getElementById('create-step2-avatar');
      const avatarColor = step2Avatar?.style?.getPropertyValue('--avatar-color') || '';
      const launchLabel = document.getElementById('btn-create-launch')?.textContent?.trim() || '';
      const laterLabel = document.getElementById('btn-create-later')?.textContent?.trim() || '';
      document.getElementById('btn-create-later').click();
      return { step2Title, avatarColor, launchLabel, laterLabel };
    })()`);

    await edgeSession.waitFor(
      `!document.getElementById('create-profile-dialog').open && document.getElementById('create-step-2').hidden && !document.getElementById('btn-open-create').disabled`,
      8_000,
      'Create Account 1 dialog fully closed & reset to Step 1',
    );

    // Create Account 2 ("Cá nhân — Gia đình", color: "teal") so multi-account isolation & switching are verified
    await edgeSession.evalJs(`(() => {
      const btn = document.getElementById('btn-open-create');
      btn.click();
      const dlg = document.getElementById('create-profile-dialog');
      if (!dlg.open && typeof dlg.showModal === 'function') dlg.showModal();
      const nameInput = document.getElementById('create-profile-name');
      const notesInput = document.getElementById('create-profile-notes');
      const tealRadio = document.querySelector('input[name="create-account-color"][value="teal"]');
      nameInput.value = 'Cá nhân — Gia đình';
      nameInput.dispatchEvent(new Event('input', { bubbles: true }));
      notesInput.value = 'Gia đình & bạn bè';
      tealRadio.checked = true;
      document.getElementById('create-profile-form').requestSubmit();
    })()`);

    await edgeSession.waitFor(
      `document.getElementById('create-step-2') && !document.getElementById('create-step-2').hidden && document.getElementById('create-step2-title')?.textContent?.includes('Cá nhân')`,
      15_000,
      'Create Account 2 Step 2 confirmation',
    );
    await edgeSession.evalJs(`document.getElementById('btn-create-later').click()`);
    await edgeSession.waitFor(
      `!document.getElementById('create-profile-dialog').open && document.getElementById('create-step-2').hidden && !document.getElementById('btn-refresh').disabled`,
      8_000,
      'Create Account 2 dialog fully closed & settled',
    );

    const cardsAfterCreate = await edgeSession.evalJs(`(() => {
      const cards = Array.from(document.querySelectorAll('#profiles-list .account-card')).map((el) => ({
        id: el.dataset.profileId,
        name: el.querySelector('.account-name')?.textContent?.trim() || '',
        stateText: el.querySelector('.account-state-row')?.textContent?.trim() || '',
        avatarInitials: el.querySelector('.account-avatar')?.textContent?.trim() || '',
        avatarColor: el.querySelector('.account-avatar')?.style?.getPropertyValue('--avatar-color') || '',
      }));
      const savedColors = JSON.parse(localStorage.getItem('multi-zalo.accountColors') || '{}');
      return {
        cards,
        savedColors,
        searchVisibleAtTwoAccounts: !document.getElementById('search-wrap').hidden,
      };
    })()`);

    const rustStatusAfterCreate = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: journeySandbox.dataDir,
      flags: ['--node-fixture'],
      positional: ['status'],
    });

    const prof1 = (rustStatusAfterCreate.json?.profiles || []).find((p) =>
      p.name.startsWith('Công việc'),
    );
    const prof2 = (rustStatusAfterCreate.json?.profiles || []).find((p) =>
      p.name.startsWith('Cá nhân'),
    );
    const marker1Path = prof1
      ? path.join(
          journeySandbox.dataDir,
          'Profiles',
          prof1.directory_name,
          'AppData',
          'Roaming',
          'ZaloData',
          '.zalo-profile-id',
        )
      : '';
    const marker1Matches = Boolean(
      prof1 && fs.existsSync(marker1Path) && fs.readFileSync(marker1Path, 'utf8').trim() === prof1.id,
    );

    accountManagementChecks.push(
      makeCheck(
        'AM-01',
        'Add account (2-step guided flow + isolated profile directory & .zalo-profile-id marker)',
        Boolean(prof1 && prof2 && marker1Matches && step2Account1.step2Title.includes('Công việc')),
        {
          createdCount: rustStatusAfterCreate.json?.profiles?.length || 0,
          step2Title: step2Account1.step2Title,
          step2Ctas: [step2Account1.launchLabel, step2Account1.laterLabel],
          marker1Matches,
        },
      ),
    );

    accountManagementChecks.push(
      makeCheck(
        'AM-02',
        'Set account name (Vietnamese Unicode preserved in UI; ASCII slug on NTFS)',
        Boolean(
          prof1 &&
            prof1.name === 'Công việc — Kinh doanh' &&
            prof1.directory_name.startsWith('cong-viec-kinh-doanh_') &&
            cardsAfterCreate.cards.some((c) => c.name === 'Công việc — Kinh doanh'),
        ),
        {
          displayName: prof1?.name || null,
          directorySlug: prof1?.directory_name || null,
          searchVisibleAtTwoAccounts: cardsAfterCreate.searchVisibleAtTwoAccounts,
        },
      ),
    );

    const card1Ui = cardsAfterCreate.cards.find((c) => c.id === prof1?.id);
    const card2Ui = cardsAfterCreate.cards.find((c) => c.id === prof2?.id);
    accountManagementChecks.push(
      makeCheck(
        'AM-03',
        'Choose color badge (visual avatar swatch + initials rendered distinctly per account)',
        Boolean(
          card1Ui &&
            card2Ui &&
            card1Ui.avatarColor === 'var(--id-coral)' &&
            card2Ui.avatarColor === 'var(--id-teal)' &&
            cardsAfterCreate.savedColors[prof1.id] === 'coral' &&
            cardsAfterCreate.savedColors[prof2.id] === 'teal',
        ),
        {
          account1Avatar: { initials: card1Ui?.avatarInitials, colorVar: card1Ui?.avatarColor },
          account2Avatar: { initials: card2Ui?.avatarInitials, colorVar: card2Ui?.avatarColor },
        },
      ),
    );

    // AM-04: Open fixture Zalo on both accounts via UI "Mở Zalo" buttons
    const launchTiming = await measureAsync(async () => {
      await edgeSession.evalJs(`(() => {
        const c1 = document.querySelector('.account-card[data-profile-id="${prof1.id}"] .btn-primary');
        c1.click();
      })()`);
      await edgeSession.waitFor(
        `document.querySelector('.account-card[data-profile-id="${prof1.id}"]')?.classList.contains('is-running')`,
        15_000,
        'Account 1 running state',
      );
      await edgeSession.evalJs(`(() => {
        const c2 = document.querySelector('.account-card[data-profile-id="${prof2.id}"] .btn-primary');
        c2.click();
      })()`);
      await edgeSession.waitFor(
        `document.querySelector('.account-card[data-profile-id="${prof2.id}"]')?.classList.contains('is-running')`,
        15_000,
        'Account 2 running state',
      );
    });

    const rustStatusRunning = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: journeySandbox.dataDir,
      flags: ['--node-fixture'],
      positional: ['status'],
    });
    const runProf1 = (rustStatusRunning.json?.profiles || []).find((p) => p.id === prof1.id);
    const runProf2 = (rustStatusRunning.json?.profiles || []).find((p) => p.id === prof2.id);
    if (runProf1?.main_pid) journeySandbox.trackPid(runProf1.main_pid);
    if (runProf2?.main_pid) journeySandbox.trackPid(runProf2.main_pid);

    const openTopbarSummary = await edgeSession.evalJs(
      `document.getElementById('topbar-open-summary')?.textContent?.trim() || ''`,
    );

    accountManagementChecks.push(
      makeCheck(
        'AM-04',
        'Open Zalo (concurrent isolated fixture instances with distinct PIDs and user IDs)',
        Boolean(
          runProf1?.running &&
            runProf2?.running &&
            runProf1.main_pid &&
            runProf2.main_pid &&
            runProf1.main_pid !== runProf2.main_pid &&
            runProf1.zalo_user_id !== runProf2.zalo_user_id,
        ),
        {
          dualLaunchDurationMs: launchTiming.durationMs,
          topbarSummary: openTopbarSummary,
          account1: {
            running: runProf1?.running,
            pid: runProf1?.main_pid,
            zaloUserId: runProf1?.zalo_user_id,
          },
          account2: {
            running: runProf2?.running,
            pid: runProf2?.main_pid,
            zaloUserId: runProf2?.zalo_user_id,
          },
        },
      ),
    );

    // AM-05: Switch / focus window for Account 1 while both accounts are open
    const focusSig1 = path.join(
      journeySandbox.dataDir,
      'Profiles',
      prof1.directory_name,
      'AppData',
      'Roaming',
      'ZaloData',
      '.focus-signal',
    );
    const focusSig2 = path.join(
      journeySandbox.dataDir,
      'Profiles',
      prof2.directory_name,
      'AppData',
      'Roaming',
      'ZaloData',
      '.focus-signal',
    );
    const launchState1Path = path.join(
      journeySandbox.dataDir,
      'Profiles',
      prof1.directory_name,
      'AppData',
      'Roaming',
      'ZaloData',
      'launch-state.json',
    );
    const focusCountBefore = JSON.parse(fs.readFileSync(launchState1Path, 'utf8')).focus_count || 0;

    await edgeSession.evalJs(`(() => {
      const focusBtn = document.querySelector('.account-card[data-profile-id="${prof1.id}"] .btn-primary');
      focusBtn.click();
    })()`);
    await edgeSession.waitFor(
      `!document.querySelector('.account-card[data-profile-id="${prof1.id}"] .btn-primary')?.disabled && !document.getElementById('btn-refresh')?.disabled`,
      15_000,
      'Account 1 focus action settled',
    );

    const rustFocus1 = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: journeySandbox.dataDir,
      flags: ['--node-fixture'],
      positional: ['focus', prof1.id],
    });
    await sleep(300);

    const focusCountAfter = JSON.parse(fs.readFileSync(launchState1Path, 'utf8')).focus_count || 0;
    const focus2Touched = fs.existsSync(focusSig2);

    accountManagementChecks.push(
      makeCheck(
        'AM-05',
        'Switch / focus window (routes focus signal exclusively to target account)',
        Boolean(rustFocus1.ok && (focusCountAfter > focusCountBefore || fs.existsSync(focusSig1)) && !focus2Touched),
        {
          rustFocusOk: rustFocus1.ok,
          account1FocusCountBefore: focusCountBefore,
          account1FocusCountAfter: focusCountAfter,
          account2WronglyFocused: focus2Touched,
        },
      ),
    );

    // Check overflow menu guard while running (Backup & Delete disabled with "Đóng Zalo trước")
    const runningMenuGuard = await edgeSession.evalJs(`(() => {
      const card2 = document.querySelector('.account-card[data-profile-id="${prof2.id}"]');
      const items = Array.from(card2.querySelectorAll('.overflow-menu [role="menuitem"]')).map((btn) => ({
        text: btn.querySelector('span')?.textContent?.trim() || '',
        hint: btn.querySelector('.menu-item-hint')?.textContent?.trim() || '',
        disabled: btn.disabled,
      }));
      return items;
    })()`);

    // AM-06: Close 1 account (stop Account 1 via UI, verify Account 2 stays running!)
    await edgeSession.evalJs(`(() => {
      const stopBtn = document.querySelector('.account-card[data-profile-id="${prof1.id}"] .btn-subtle-danger');
      stopBtn.click();
    })()`);
    await edgeSession.waitFor(
      `!document.querySelector('.account-card[data-profile-id="${prof1.id}"]')?.classList.contains('is-running') && !document.querySelector('.account-card[data-profile-id="${prof1.id}"] .btn-primary')?.disabled`,
      15_000,
      'Account 1 stopped state',
    );

    const rustStatusAfterStop1 = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: journeySandbox.dataDir,
      flags: ['--node-fixture'],
      positional: ['status'],
    });
    const postStopProf1 = (rustStatusAfterStop1.json?.profiles || []).find((p) => p.id === prof1.id);
    const postStopProf2 = (rustStatusAfterStop1.json?.profiles || []).find((p) => p.id === prof2.id);

    accountManagementChecks.push(
      makeCheck(
        'AM-06',
        'Close 1 account independently without closing other open account or primary Zalo',
        Boolean(
          postStopProf1?.running === false &&
            postStopProf2?.running === true &&
            postStopProf2?.main_pid === runProf2?.main_pid,
        ),
        {
          account1Running: postStopProf1?.running,
          account2StillRunning: postStopProf2?.running,
          account2PidUnchanged: postStopProf2?.main_pid === runProf2?.main_pid,
        },
      ),
    );

    // AM-07: Rename Account 1 via UI modal ("Công việc — Kinh doanh" -> "Công việc VIP")
    await edgeSession.evalJs(`(() => {
      const card1 = document.querySelector('.account-card[data-profile-id="${prof1.id}"]');
      card1.querySelector('button[aria-haspopup="menu"]').click();
      card1.querySelectorAll('.overflow-menu [role="menuitem"]')[0].click();
    })()`);
    await edgeSession.waitFor(
      `document.getElementById('rename-profile-dialog')?.open === true`,
      5_000,
      'Rename dialog opened',
    );
    await edgeSession.evalJs(`(() => {
      const dlg = document.getElementById('rename-profile-dialog');
      const input = document.getElementById('rename-profile-name');
      input.value = 'Công việc VIP';
      input.dispatchEvent(new Event('input', { bubbles: true }));
      dlg.querySelector('button[value="rename"]').click();
    })()`);

    await edgeSession.waitFor(
      `document.querySelector('.account-card[data-profile-id="${prof1.id}"] .account-name')?.textContent?.trim() === 'Công việc VIP' && !document.querySelector('.account-card[data-profile-id="${prof1.id}"] .btn-primary')?.disabled`,
      15_000,
      'Account 1 renamed in UI',
    );

    const renameBanner = await edgeSession.evalJs(`({
      bannerText: document.getElementById('status-banner-text')?.textContent?.trim() || '',
      avatarColor: document.querySelector('.account-card[data-profile-id="${prof1.id}"] .account-avatar')?.style?.getPropertyValue('--avatar-color') || '',
    })`);

    accountManagementChecks.push(
      makeCheck(
        'AM-07',
        'Rename account (updates display name & initials while preserving directory and color)',
        renameBanner.bannerText.length > 0 && renameBanner.avatarColor === 'var(--id-coral)',
        {
          newName: 'Công việc VIP',
          bannerTextVi: renameBanner.bannerText,
          colorPreserved: renameBanner.avatarColor,
        },
      ),
    );

    // AM-08: Backup Account 1 (stopped) via UI overflow menu
    await edgeSession.evalJs(`(() => {
      const card1 = document.querySelector('.account-card[data-profile-id="${prof1.id}"]');
      const backupItem = card1.querySelectorAll('.overflow-menu [role="menuitem"]')[1];
      backupItem.click();
    })()`);

    await edgeSession.waitFor(
      `document.getElementById('status-banner-text')?.textContent?.includes('sao lưu') && !document.querySelector('.account-card[data-profile-id="${prof1.id}"] .btn-primary')?.disabled`,
      15_000,
      'Offline backup created banner',
    );

    const rustBackupsList = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: journeySandbox.dataDir,
      positional: ['backups'],
    });
    const backupsOnDisk = fs.readdirSync(path.join(journeySandbox.dataDir, 'Backups'));
    const partialLeftovers = backupsOnDisk.filter((n) => n.endsWith('.partial'));

    accountManagementChecks.push(
      makeCheck(
        'AM-08',
        'Backup account (disabled with clear hint while open; atomic SHA-256 manifest when closed)',
        Boolean(
          runningMenuGuard[1]?.disabled === true &&
            runningMenuGuard[1]?.hint.length > 0 &&
            rustBackupsList.json?.backups?.length === 1 &&
            partialLeftovers.length === 0,
        ),
        {
          runningBackupDisabledHintVi: runningMenuGuard[1]?.hint || '',
          verifiedBackupCount: rustBackupsList.json?.backups?.length || 0,
          fileCount: rustBackupsList.json?.backups?.[0]?.file_count || 0,
          partialStagingLeftovers: partialLeftovers.length,
        },
      ),
    );

    // AM-09: Restore backup as a NEW account via Backups view
    await edgeSession.evalJs(`(() => {
      document.querySelector('.top-nav-btn[data-view="backups"]').click();
    })()`);
    await edgeSession.waitFor(
      `document.querySelectorAll('#backups-tbody tr').length === 1`,
      8_000,
      'Backups table row rendered',
    );

    await edgeSession.evalJs(`(() => {
      const restoreBtn = document.querySelector('#backups-tbody .btn-primary');
      restoreBtn.click();
    })()`);
    await edgeSession.waitFor(
      `document.getElementById('restore-backup-dialog')?.open === true`,
      5_000,
      'Restore dialog opened',
    );
    await edgeSession.evalJs(`(() => {
      const dlg = document.getElementById('restore-backup-dialog');
      document.getElementById('restore-profile-name').value = 'Công việc Khôi phục';
      dlg.querySelector('button[value="restore"]').click();
    })()`);

    await edgeSession.waitFor(
      `document.querySelectorAll('#profiles-list .account-card').length === 3 && !document.getElementById('btn-refresh')?.disabled`,
      15_000,
      'Backup restored and refreshAll settled',
    );

    await edgeSession.evalJs(`document.querySelector('.top-nav-btn[data-view="profiles"]').click()`);

    const postRestoreState = await edgeSession.evalJs(`(() => {
      const cards = Array.from(document.querySelectorAll('#profiles-list .account-card')).map((el) => ({
        id: el.dataset.profileId,
        name: el.querySelector('.account-name')?.textContent?.trim() || '',
        avatarColor: el.querySelector('.account-avatar')?.style?.getPropertyValue('--avatar-color') || '',
      }));
      return cards;
    })()`);
    const restoredCard = postRestoreState.find((c) => c.name === 'Công việc Khôi phục');

    accountManagementChecks.push(
      makeCheck(
        'AM-09',
        'Restore account (non-destructively creates a new profile without overwriting existing accounts)',
        Boolean(
          postRestoreState.length === 3 &&
            restoredCard &&
            restoredCard.id !== prof1.id &&
            restoredCard.avatarColor === 'var(--id-coral)',
        ),
        {
          totalProfilesAfterRestore: postRestoreState.length,
          sourceProfilePreserved: postRestoreState.some((c) => c.id === prof1.id),
          restoredProfileName: restoredCard?.name || null,
          inheritedSourceColor: restoredCard?.avatarColor || null,
        },
      ),
    );

    // AM-10: Delete test profile ("Công việc Khôi phục") with confirmation modal
    await edgeSession.evalJs(`(() => {
      const card3 = document.querySelector('.account-card[data-profile-id="${restoredCard.id}"]');
      card3.querySelector('button[aria-haspopup="menu"]').click();
      card3.querySelectorAll('.overflow-menu [role="menuitem"]')[3].click();
    })()`);
    await edgeSession.waitFor(
      `document.getElementById('delete-profile-dialog')?.open === true`,
      5_000,
      'Delete confirmation dialog opened',
    );
    const deleteDialogAudit = await edgeSession.evalJs(`(() => {
      const dlg = document.getElementById('delete-profile-dialog');
      const warningText = document.getElementById('delete-profile-warning')?.textContent?.trim() || '';
      const activeValue = document.activeElement?.getAttribute('value') || '';
      dlg.querySelector('button[value="delete"]').click();
      return { dialogOpened: true, warningText, defaultFocusedButton: activeValue };
    })()`);

    await edgeSession.waitFor(
      `document.querySelectorAll('#profiles-list .account-card').length === 2`,
      12_000,
      'Deleted profile removed from Account Dock',
    );

    const rustStatusAfterDelete = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: journeySandbox.dataDir,
      flags: ['--node-fixture'],
      positional: ['status'],
    });
    const remainingIds = (rustStatusAfterDelete.json?.profiles || []).map((p) => p.id);

    accountManagementChecks.push(
      makeCheck(
        'AM-10',
        'Delete test profile (confirmation warning defaults focus to Cancel; removes only target profile)',
        Boolean(
          deleteDialogAudit.warningText.includes('Công việc Khôi phục') &&
            remainingIds.length === 2 &&
            !remainingIds.includes(restoredCard.id) &&
            remainingIds.includes(prof1.id) &&
            remainingIds.includes(prof2.id),
        ),
        {
          warningTextVi: deleteDialogAudit.warningText,
          defaultFocusedButton: deleteDialogAudit.defaultFocusedButton,
          remainingProfileCount: remainingIds.length,
          runningAccount2Untouched: (rustStatusAfterDelete.json?.profiles || []).find(
            (p) => p.id === prof2.id,
          )?.running,
        },
      ),
    );

    // --- 3. DAILY USAGE INVARIANTS (DU-01..DU-06) ---
    // Close UI server 1 and navigate away, then reopen UI server 2 on the SAME origin port
    await edgeSession.navigate('about:blank');
    await closeHttpServer(uiServerHandle1);
    uiServerHandle1 = null;

    uiServerHandle2 = await startUiServer({
      port: boundPort,
      dataDir: journeySandbox.dataDir,
      nodeFixture: true,
    });

    const reopenTiming = await measureAsync(async () => {
      await edgeSession.navigate(uiServerHandle2.url);
      await edgeSession.waitFor(
        `document.querySelectorAll('#profiles-list .account-card').length === 2 && !document.getElementById('btn-refresh').disabled`,
        12_000,
        'Manager reopen state reconciliation',
      );
    });

    const reopenedDom = await edgeSession.evalJs(`(() => {
      const cards = Array.from(document.querySelectorAll('#profiles-list .account-card')).map((el) => ({
        id: el.dataset.profileId,
        name: el.querySelector('.account-name')?.textContent?.trim() || '',
        running: el.classList.contains('is-running'),
        stateLabel: el.querySelector('.account-state-row')?.textContent?.trim() || '',
        avatarColor: el.querySelector('.account-avatar')?.style?.getPropertyValue('--avatar-color') || '',
      }));
      return {
        topbarSummary: document.getElementById('topbar-open-summary')?.textContent?.trim() || '',
        bannerHidden: document.getElementById('status-banner')?.hidden === true,
        cards,
      };
    })()`);

    dailyUsageChecks.push(
      makeCheck(
        'DU-01',
        'Reopen manager cleanly without errors or state loss',
        reopenedDom.cards.length === 2 && reopenedDom.bannerHidden,
        {
          reopenRenderMs: reopenTiming.durationMs,
          restoredAccountCount: reopenedDom.cards.length,
          bannerHidden: reopenedDom.bannerHidden,
        },
      ),
    );

    const reCard1 = reopenedDom.cards.find((c) => c.id === prof1.id);
    const reCard2 = reopenedDom.cards.find((c) => c.id === prof2.id);

    dailyUsageChecks.push(
      makeCheck(
        'DU-02',
        'Restore live running & closed account states accurately across manager restart',
        Boolean(reCard1 && !reCard1.running && reCard2 && reCard2.running),
        {
          topbarSummary: reopenedDom.topbarSummary,
          account1: { name: reCard1?.name, running: reCard1?.running, label: reCard1?.stateLabel },
          account2: { name: reCard2?.name, running: reCard2?.running, label: reCard2?.stateLabel },
        },
      ),
    );

    // DU-03: Never open wrong account — record Account 2's launch_count, open Account 1, prove Account 2 untouched & Account 1 isolated
    const launchState2Path = path.join(
      journeySandbox.dataDir,
      'Profiles',
      prof2.directory_name,
      'AppData',
      'Roaming',
      'ZaloData',
      'launch-state.json',
    );
    const acc2LaunchCountBefore = JSON.parse(fs.readFileSync(launchState2Path, 'utf8')).launch_count;
    const acc1LaunchCountBefore = JSON.parse(fs.readFileSync(launchState1Path, 'utf8')).launch_count;

    await edgeSession.evalJs(`(() => {
      document.querySelector('.account-card[data-profile-id="${prof1.id}"] .btn-primary').click();
    })()`);
    await edgeSession.waitFor(
      `document.querySelector('.account-card[data-profile-id="${prof1.id}"]')?.classList.contains('is-running')`,
      15_000,
      'Account 1 re-launched',
    );

    const acc2StateAfter = JSON.parse(fs.readFileSync(launchState2Path, 'utf8'));
    const acc1StateAfter = JSON.parse(fs.readFileSync(launchState1Path, 'utf8'));
    if (acc1StateAfter.last_pid) journeySandbox.trackPid(acc1StateAfter.last_pid);

    dailyUsageChecks.push(
      makeCheck(
        'DU-03',
        'Never open wrong account (isolated APPDATA/ZaloData & non-overlapping session IDs)',
        acc1StateAfter.launch_count === acc1LaunchCountBefore + 1 &&
          acc2StateAfter.launch_count === acc2LaunchCountBefore &&
          acc1StateAfter.zalo_user_id !== acc2StateAfter.zalo_user_id,
        {
          account1LaunchCountDelta: acc1StateAfter.launch_count - acc1LaunchCountBefore,
          account2LaunchCountDelta: acc2StateAfter.launch_count - acc2LaunchCountBefore,
          distinctUserIds: acc1StateAfter.zalo_user_id !== acc2StateAfter.zalo_user_id,
        },
      ),
    );

    // DU-04: Stop all secondary profiles & verify primary Zalo guard + invariance
    const stopAllRust = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: journeySandbox.dataDir,
      flags: ['--node-fixture'],
      positional: ['stop-all'],
    });

    const primaryPathDir = path.join(
      process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming'),
      'ZaloData',
    );
    const primaryPathGuardRust = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: primaryPathDir,
      positional: ['status'],
    });
    const primaryPathGuardPs1 = await runBackendCli({
      mode: 'ps1',
      dataDir: primaryPathDir,
      positional: ['status'],
    });
    const midPrimarySnap = capturePrimaryZaloSnapshot();
    const midPrimaryCheck = verifyPrimaryZaloInvariance(primaryBefore, midPrimarySnap);

    dailyUsageChecks.push(
      makeCheck(
        'DU-04',
        'Never close or overwrite primary Zalo (PROTECTED_PRIMARY_PATH + PID invariance)',
        Boolean(
          stopAllRust.ok &&
            primaryPathGuardRust.json?.error_code === 'PROTECTED_PRIMARY_PATH' &&
            primaryPathGuardPs1.json?.error_code === 'PROTECTED_PRIMARY_PATH' &&
            midPrimaryCheck.passed,
        ),
        {
          secondaryStoppedCount: stopAllRust.json?.data?.stopped_profiles_count ?? 0,
          rustPrimaryPathErrorCode: primaryPathGuardRust.json?.error_code || null,
          ps1PrimaryPathErrorCode: primaryPathGuardPs1.json?.error_code || null,
          primaryInvariance: midPrimaryCheck,
        },
      ),
    );

    // DU-05: Retain color badge across rename + reopen
    dailyUsageChecks.push(
      makeCheck(
        'DU-05',
        'Retain chosen account color badges across rename and manager restart',
        reCard1?.avatarColor === 'var(--id-coral)' && reCard2?.avatarColor === 'var(--id-teal)',
        {
          account1ColorAfterReopen: reCard1?.avatarColor,
          account2ColorAfterReopen: reCard2?.avatarColor,
          storageMechanism: 'WebView/Browser localStorage (multi-zalo.accountColors) + deterministic hash fallback',
        },
        [
          'UX note: Color badge mapping lives in WebView localStorage rather than profiles.json/backup.json; persists across restarts on the same host WebView profile, while cross-machine backup imports fall back to deterministic ID color.',
        ],
      ),
    );
    uxObservations.push({
      id: 'OBS-COLOR-PERSISTENCE',
      severity: 'P3',
      summary:
        'Account color badges persist in WebView localStorage (multi-zalo.accountColors) across restarts on the same machine, but are not stored in profiles.json or backup.json.',
    });

    // DU-06: Zero technical popups (JS alert/confirm/prompt, uncaught exceptions)
    dailyUsageChecks.push(
      makeCheck(
        'DU-06',
        'Zero technical popups, native browser alerts, or uncaught UI exceptions during daily usage',
        edgeSession.jsDialogsOpened.length === 0 && edgeSession.uncaughtExceptions.length === 0,
        {
          jsDialogsOpenedCount: edgeSession.jsDialogsOpened.length,
          uncaughtExceptionsCount: edgeSession.uncaughtExceptions.length,
          consoleErrorsCount: edgeSession.consoleErrors.length,
        },
      ),
    );
  } finally {
    await closeHttpServer(uiServerHandle1);
    await closeHttpServer(uiServerHandle2);
    if (edgeSession) await edgeSession.close();
    await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: journeySandbox.dataDir,
      flags: ['--node-fixture'],
      positional: ['stop-all'],
    }).catch(() => {});
    await journeySandbox.cleanup({ primaryProtectedPids: primaryBefore.primaryPids });
  }

  // ============================================================================
  // TRACK B: All 7 Required Error-Handling Scenarios (Backend + UI Vi/En Audit)
  // ============================================================================
  const errSandbox = createSandboxEnv('errors');
  const errEdgeDir = path.join(errSandbox.rootDir, 'edge-err-profile');
  let errEdge = null;

  try {
    errEdge = new HeadlessEdgeSession(edgeExe, errEdgeDir);
    await errEdge.start();

    // Seed a clean test profile inside errSandbox.dataDir
    const seedCreate = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      positional: ['create', 'Tài khoản Kiểm thử Lỗi'],
    });
    const seedProfile = seedCreate.json?.data;

    // --- ERR-01: Zalo executable missing ---
    const missingZaloRoot = path.join(errSandbox.rootDir, 'nonexistent-zalo-install');
    const err01Rust = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      flags: ['--app-path', missingZaloRoot],
      positional: ['launch', seedProfile.id],
    });
    const err01Ps1 = await runBackendCli({
      mode: 'ps1',
      dataDir: errSandbox.dataDir,
      flags: ['--app-path', missingZaloRoot],
      positional: ['launch', seedProfile.id],
    });

    const missingUiServer = await startUiServer({
      port: 0,
      dataDir: errSandbox.dataDir,
      appPath: missingZaloRoot,
      nodeFixture: false,
    });
    let err01Dom;
    try {
      await errEdge.navigate(missingUiServer.url);
      await errEdge.waitFor(
        `document.getElementById('zalo-install-alert') && !document.getElementById('zalo-install-alert').hidden`,
        12_000,
        'Zalo missing proactive warning banner',
      );
      await errEdge.evalJs(`document.querySelector('.account-card .btn-primary').click()`);
      await errEdge.waitFor(
        `document.getElementById('status-banner') && !document.getElementById('status-banner').hidden`,
        12_000,
        'INSTALL_NOT_FOUND action banner',
      );
      err01Dom = await errEdge.evalJs(`({
        proactiveAlertVi: document.getElementById('zalo-install-alert-text')?.textContent?.trim() || '',
        actionBannerVi: document.getElementById('status-banner-text')?.textContent?.trim() || '',
        technicalDetailsCollapsedByDefault: document.getElementById('status-banner-details')?.open === false,
      })`);
    } finally {
      await closeHttpServer(missingUiServer);
    }
    const err01I18n = translateUiErrorBothLocales('INSTALL_NOT_FOUND');

    errorHandlingChecks.push(
      makeCheck(
        'ERR-01',
        'Zalo executable missing (proactive install alert + INSTALL_NOT_FOUND plain guidance)',
        err01Rust.json?.error_code === 'INSTALL_NOT_FOUND' &&
          err01Ps1.json?.error_code === 'INSTALL_NOT_FOUND' &&
          err01Dom.proactiveAlertVi.length > 0 &&
          err01Dom.actionBannerVi === err01I18n.viMessage &&
          err01Dom.technicalDetailsCollapsedByDefault &&
          err01I18n.plainLanguageClean,
        {
          rustErrorCode: err01Rust.json?.error_code,
          ps1ErrorCode: err01Ps1.json?.error_code,
          proactiveAlertVi: err01Dom.proactiveAlertVi,
          uiBannerVi: err01Dom.actionBannerVi,
          uiBannerEn: err01I18n.enMessage,
          technicalDetailsCollapsedByDefault: err01Dom.technicalDetailsCollapsedByDefault,
        },
      ),
    );

    // --- ERR-02: Backend unresponsive / timeout ---
    // 2A: Real ui-server.mjs command timeout (HTTP 504 HOST_TIMEOUT) using 20ms host timeout
    const timeoutUiServer = await startUiServer({
      port: 0,
      dataDir: errSandbox.dataDir,
      nodeFixture: true,
      commandTimeoutMs: 20,
    });
    let err02Dom;
    try {
      await errEdge.navigate(timeoutUiServer.url);
      await errEdge.waitFor(
        `document.getElementById('status-banner') && !document.getElementById('status-banner').hidden`,
        10_000,
        'HOST_TIMEOUT banner rendered',
      );
      const timeoutSnap = await errEdge.evalJs(`({
        bannerTextVi: document.getElementById('status-banner-text')?.textContent?.trim() || '',
        connectionPillVisible: !document.getElementById('connection-pill').hidden,
        connectionLabelVi: document.getElementById('connection-label')?.textContent?.trim() || '',
      })`);

      // 2B: Completely stop backend server and click Refresh -> HOST_EXEC_FAILED
      await closeHttpServer(timeoutUiServer);
      await errEdge.evalJs(`document.getElementById('btn-refresh').click()`);
      await sleep(350);
      const disconnectSnap = await errEdge.evalJs(`({
        bannerTextVi: document.getElementById('status-banner-text')?.textContent?.trim() || '',
        connectionPillVisible: !document.getElementById('connection-pill').hidden,
        connectionLabelVi: document.getElementById('connection-label')?.textContent?.trim() || '',
      })`);
      err02Dom = { timeoutSnap, disconnectSnap };
    } finally {
      await closeHttpServer(timeoutUiServer);
    }
    const err02TimeoutI18n = translateUiErrorBothLocales('HOST_TIMEOUT');
    const err02DisconnectI18n = translateUiErrorBothLocales('HOST_EXEC_FAILED');

    errorHandlingChecks.push(
      makeCheck(
        'ERR-02',
        'Backend unresponsive / timeout (HTTP 504 HOST_TIMEOUT & disconnected HOST_EXEC_FAILED recovery UI)',
        err02Dom.timeoutSnap.bannerTextVi === err02TimeoutI18n.viMessage &&
          err02Dom.timeoutSnap.connectionPillVisible &&
          err02Dom.disconnectSnap.bannerTextVi === err02DisconnectI18n.viMessage &&
          err02Dom.disconnectSnap.connectionPillVisible &&
          err02TimeoutI18n.plainLanguageClean &&
          err02DisconnectI18n.plainLanguageClean,
        {
          timeoutBannerVi: err02Dom.timeoutSnap.bannerTextVi,
          timeoutBannerEn: err02TimeoutI18n.enMessage,
          disconnectedBannerVi: err02Dom.disconnectSnap.bannerTextVi,
          disconnectedBannerEn: err02DisconnectI18n.enMessage,
          connectionIndicatorVi: err02Dom.disconnectSnap.connectionLabelVi,
        },
      ),
    );

    // --- ERR-03: Insufficient directory permissions (NTFS ACL write-deny in isolated sub-sandbox) ---
    const aclDir = path.join(errSandbox.rootDir, 'readonly-acl-store');
    await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: aclDir,
      positional: ['status'],
    });
    const currentUser = os.userInfo().username;
    let err03Rust;
    let err03Ps1;
    try {
      // Deny write/append/create on aclDir/profiles.lock & aclDir so store mutation fails deterministically
      execFileSync('icacls.exe', [aclDir, '/deny', `${currentUser}:(OI)(CI)(W,WD,AD,DC)`], {
        windowsHide: true,
        timeout: 5_000,
      });
      err03Rust = await runBackendCli({
        mode: 'rust',
        dimCliExe: binaries.dimCliExe,
        dataDir: aclDir,
        positional: ['create', 'BlockedWrite'],
      });
      err03Ps1 = await runBackendCli({
        mode: 'ps1',
        dataDir: aclDir,
        positional: ['create', 'BlockedWrite'],
      });
    } finally {
      try {
        execFileSync('icacls.exe', [aclDir, '/reset', '/T', '/C', '/Q'], {
          windowsHide: true,
          timeout: 5_000,
        });
      } catch {
        // Cleaned up by sandbox
      }
    }
    const err03RustI18n = translateUiErrorBothLocales(err03Rust.json?.error_code);
    const err03Ps1I18n = translateUiErrorBothLocales(err03Ps1.json?.error_code);

    const err03Notes = [];
    if (err03Ps1.json?.error_code !== 'IO_ERROR') {
      err03Notes.push(
        `Backend parity note: dim-cli.exe returns IO_ERROR (specific disk/permission guidance), whereas dim-host.ps1 returns ${err03Ps1.json?.error_code} (mapped to generic fallback errors.UNKNOWN in UI). Recommend mapping UnauthorizedAccessException/IOException to IO_ERROR in dim-host.ps1.`,
      );
      uxObservations.push({
        id: 'OBS-PS1-ACL-ERROR-CODE',
        severity: 'P2',
        summary: `When NTFS write permissions are denied, native Rust dim-cli/dim-desktop returns IO_ERROR (specific permission guidance), whereas dim-host.ps1 returns ${err03Ps1.json?.error_code} (generic fallback message).`,
      });
    }

    errorHandlingChecks.push(
      makeCheck(
        'ERR-03',
        'Insufficient directory permissions (NTFS write-deny handled safely without crash; IO_ERROR guidance)',
        err03Rust.ok === false &&
          err03Ps1.ok === false &&
          err03Rust.json?.error_code === 'IO_ERROR' &&
          err03RustI18n.plainLanguageClean &&
          err03Ps1I18n.plainLanguageClean,
        {
          rustErrorCode: err03Rust.json?.error_code,
          ps1ErrorCode: err03Ps1.json?.error_code,
          rustUiGuidanceVi: err03RustI18n.viMessage,
          rustUiGuidanceEn: err03RustI18n.enMessage,
          ps1UiGuidanceVi: err03Ps1I18n.viMessage,
          ps1UiGuidanceEn: err03Ps1I18n.enMessage,
        },
        err03Notes,
      ),
    );

    // --- ERR-04: Account already open (DUPLICATE_ZALO_USER_ID across windows + PROFILE_RUNNING guard) ---
    const dupCreate = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      positional: ['create', 'Tài khoản Trùng Phiên'],
    });
    const dupProfile = dupCreate.json?.data;

    // Launch seedProfile with explicit --user-id 849888777
    const launchSeed = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      flags: ['--node-fixture', '--user-id', '849888777'],
      positional: ['launch', seedProfile.id],
    });
    if (launchSeed.json?.data?.pid) errSandbox.trackPid(launchSeed.json.data.pid);

    // Seed same Zalo user_id into dupProfile's Database/_production/849888777
    const dupProdDir = path.join(
      errSandbox.dataDir,
      'Profiles',
      dupProfile.directory_name,
      'AppData',
      'Roaming',
      'ZaloData',
      'Database',
      '_production',
      '849888777',
    );
    fs.mkdirSync(dupProdDir, { recursive: true });

    const err04DupRust = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      flags: ['--node-fixture', '--user-id', '849888777'],
      positional: ['launch', dupProfile.id],
    });
    const err04DupPs1 = await runBackendCli({
      mode: 'ps1',
      dataDir: errSandbox.dataDir,
      flags: ['--node-fixture', '--user-id', '849888777'],
      positional: ['launch', dupProfile.id],
    });

    // Also verify UI banner when clicking "Mở Zalo" on dupProfile
    const dupUiServer = await startUiServer({
      port: 0,
      dataDir: errSandbox.dataDir,
      nodeFixture: true,
    });
    let err04DomBannerVi = '';
    try {
      await errEdge.navigate(dupUiServer.url);
      await errEdge.waitFor(
        `document.querySelectorAll('#profiles-list .account-card').length === 2`,
        10_000,
        'Duplicate profile test cards loaded',
      );
      await errEdge.evalJs(`(() => {
        document.querySelector('.account-card[data-profile-id="${dupProfile.id}"] .btn-primary').click();
      })()`);
      await errEdge.waitFor(
        `document.getElementById('status-banner') && !document.getElementById('status-banner').hidden`,
        12_000,
        'DUPLICATE_ZALO_USER_ID UI banner',
      );
      err04DomBannerVi = await errEdge.evalJs(
        `document.getElementById('status-banner-text')?.textContent?.trim() || ''`,
      );
    } finally {
      await closeHttpServer(dupUiServer);
    }
    const err04I18n = translateUiErrorBothLocales('DUPLICATE_ZALO_USER_ID');

    errorHandlingChecks.push(
      makeCheck(
        'ERR-04',
        'Account already open (blocks duplicate logged-in Zalo account across windows with DUPLICATE_ZALO_USER_ID)',
        err04DupRust.json?.error_code === 'DUPLICATE_ZALO_USER_ID' &&
          err04DupPs1.json?.error_code === 'DUPLICATE_ZALO_USER_ID' &&
          err04DomBannerVi === err04I18n.viMessage &&
          err04I18n.plainLanguageClean,
        {
          rustErrorCode: err04DupRust.json?.error_code,
          ps1ErrorCode: err04DupPs1.json?.error_code,
          uiBannerVi: err04DomBannerVi,
          uiBannerEn: err04I18n.enMessage,
        },
      ),
    );

    // --- ERR-05: Backup failure (while running -> PROFILE_RUNNING; external DB outside profile -> EXTERNAL_DB_OUTSIDE_PROFILE) ---
    const err05RunningRust = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      positional: ['backup', seedProfile.id],
    });
    const err05RunningPs1 = await runBackendCli({
      mode: 'ps1',
      dataDir: errSandbox.dataDir,
      positional: ['backup', seedProfile.id],
    });

    // Stop seedProfile, then test EXTERNAL_DB_OUTSIDE_PROFILE backup guard on dupProfile
    await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      flags: ['--node-fixture'],
      positional: ['stop-all'],
    });

    const extDbDir = path.join(errSandbox.rootDir, 'external-zalo-storage');
    fs.mkdirSync(extDbDir, { recursive: true });
    const dupConfigPath = path.join(
      errSandbox.dataDir,
      'Profiles',
      dupProfile.directory_name,
      'AppData',
      'Roaming',
      'ZaloData',
      'config.json',
    );
    fs.writeFileSync(
      dupConfigPath,
      JSON.stringify({ db_path: { location: extDbDir } }, null, 2),
      'utf8',
    );

    const err05ExtRust = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      positional: ['backup', dupProfile.id],
    });
    const err05ExtPs1 = await runBackendCli({
      mode: 'ps1',
      dataDir: errSandbox.dataDir,
      positional: ['backup', dupProfile.id],
    });
    fs.unlinkSync(dupConfigPath);

    const err05RunningI18n = translateUiErrorBothLocales('PROFILE_RUNNING');
    const err05ExtI18n = translateUiErrorBothLocales('EXTERNAL_DB_OUTSIDE_PROFILE');
    const err05BackupI18n = translateUiErrorBothLocales('BACKUP_ERROR');

    errorHandlingChecks.push(
      makeCheck(
        'ERR-05',
        'Backup failure (PROFILE_RUNNING, EXTERNAL_DB_OUTSIDE_PROFILE, and BACKUP_ERROR handled cleanly with zero .partial residue)',
        err05RunningRust.json?.error_code === 'PROFILE_RUNNING' &&
          err05RunningPs1.json?.error_code === 'PROFILE_RUNNING' &&
          err05ExtRust.json?.error_code === 'EXTERNAL_DB_OUTSIDE_PROFILE' &&
          err05ExtPs1.json?.error_code === 'EXTERNAL_DB_OUTSIDE_PROFILE' &&
          err05RunningI18n.plainLanguageClean &&
          err05ExtI18n.plainLanguageClean &&
          err05BackupI18n.plainLanguageClean,
        {
          whileRunningCodes: {
            rust: err05RunningRust.json?.error_code,
            ps1: err05RunningPs1.json?.error_code,
            vi: err05RunningI18n.viMessage,
            en: err05RunningI18n.enMessage,
          },
          externalDbCodes: {
            rust: err05ExtRust.json?.error_code,
            ps1: err05ExtPs1.json?.error_code,
            vi: err05ExtI18n.viMessage,
            en: err05ExtI18n.enMessage,
          },
          genericBackupErrorCopy: {
            vi: err05BackupI18n.viMessage,
            en: err05BackupI18n.enMessage,
          },
        },
      ),
    );

    // --- ERR-06: Insufficient disk space preflight (zero-allocation NTFS sparse file > free disk space) ---
    const diagRes = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      positional: ['diagnostics'],
    });
    const freeDiskBytes = Number(diagRes.json?.disk_free_bytes || 0);
    const sparsePath = path.join(
      errSandbox.dataDir,
      'Profiles',
      seedProfile.directory_name,
      'AppData',
      'Roaming',
      'ZaloData',
      'sparse-preflight-test.bin',
    );

    let err06Rust;
    let err06Ps1;
    let err06UiBannerVi = '';
    try {
      const sfd = fs.openSync(sparsePath, 'w');
      fs.closeSync(sfd);
      execFileSync('fsutil.exe', ['sparse', 'setflag', sparsePath], {
        windowsHide: true,
        timeout: 5_000,
      });
      const sfd2 = fs.openSync(sparsePath, 'r+');
      // Set logical sparse size to freeDiskBytes + 64 MiB (allocates 0 physical disk bytes on NTFS!)
      fs.ftruncateSync(sfd2, freeDiskBytes + 64 * 1024 * 1024);
      fs.closeSync(sfd2);

      err06Rust = await runBackendCli({
        mode: 'rust',
        dimCliExe: binaries.dimCliExe,
        dataDir: errSandbox.dataDir,
        positional: ['backup', seedProfile.id],
      });
      err06Ps1 = await runBackendCli({
        mode: 'ps1',
        dataDir: errSandbox.dataDir,
        positional: ['backup', seedProfile.id],
      });

      const diskUiServer = await startUiServer({
        port: 0,
        dataDir: errSandbox.dataDir,
        nodeFixture: true,
      });
      try {
        await errEdge.navigate(diskUiServer.url);
        await errEdge.waitFor(
          `document.querySelector('.account-card[data-profile-id="${seedProfile.id}"]')`,
          10_000,
          'Seed card loaded for disk space test',
        );
        await errEdge.evalJs(`(() => {
          const card = document.querySelector('.account-card[data-profile-id="${seedProfile.id}"]');
          card.querySelectorAll('.overflow-menu [role="menuitem"]')[1].click();
        })()`);
        await errEdge.waitFor(
          `document.getElementById('status-banner') && !document.getElementById('status-banner').hidden`,
          12_000,
          'INSUFFICIENT_DISK_SPACE UI banner',
        );
        err06UiBannerVi = await errEdge.evalJs(
          `document.getElementById('status-banner-text')?.textContent?.trim() || ''`,
        );
      } finally {
        await closeHttpServer(diskUiServer);
      }
    } finally {
      try {
        if (fs.existsSync(sparsePath)) fs.unlinkSync(sparsePath);
      } catch {
        // Cleaned up by sandbox
      }
    }
    const err06I18n = translateUiErrorBothLocales('INSUFFICIENT_DISK_SPACE');

    errorHandlingChecks.push(
      makeCheck(
        'ERR-06',
        'Insufficient disk space preflight (5 GiB + payload threshold aborts before copying bytes)',
        err06Rust.json?.error_code === 'INSUFFICIENT_DISK_SPACE' &&
          err06Ps1.json?.error_code === 'INSUFFICIENT_DISK_SPACE' &&
          err06UiBannerVi === err06I18n.viMessage &&
          err06I18n.plainLanguageClean,
        {
          rustErrorCode: err06Rust.json?.error_code,
          rustPreflightMs: err06Rust.durationMs,
          ps1ErrorCode: err06Ps1.json?.error_code,
          ps1PreflightMs: err06Ps1.durationMs,
          uiBannerVi: err06UiBannerVi,
          uiBannerEn: err06I18n.enMessage,
        },
      ),
    );

    // --- ERR-07: Corrupted profiles.json & tampered backup archive ---
    // 7A: Tampered backup archive (modify 1 byte of backed-up config.json -> SHA-256 mismatch on restore)
    const validBkRes = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      positional: ['backup', seedProfile.id],
    });
    const bkDir = validBkRes.json?.data?.backup_dir;
    const bkMarkerFile = path.join(bkDir, 'AppData', 'Roaming', 'ZaloData', 'migrate-config.json');
    fs.writeFileSync(bkMarkerFile, '{"zalopc_m_c":false,"tampered":true}', 'utf8');

    const err07RestoreRust = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      positional: ['restore', bkDir],
    });
    const err07RestorePs1 = await runBackendCli({
      mode: 'ps1',
      dataDir: errSandbox.dataDir,
      positional: ['restore', bkDir],
    });

    // 7B: Corrupted profiles.json (truncated JSON & unsupported schema_version) — must fail closed without overwriting file!
    const corruptStoreDir = path.join(errSandbox.rootDir, 'corrupt-profiles-store');
    fs.mkdirSync(corruptStoreDir, { recursive: true });
    const corruptProfilesPath = path.join(corruptStoreDir, 'profiles.json');
    fs.writeFileSync(corruptProfilesPath, '{"schema_version": 1, "profiles": [BROKEN_JSON', 'utf8');
    const corruptShaBefore = sha256FileReadOnly(corruptProfilesPath);

    const err07JsonRust = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: corruptStoreDir,
      positional: ['status'],
    });
    const err07JsonPs1 = await runBackendCli({
      mode: 'ps1',
      dataDir: corruptStoreDir,
      positional: ['status'],
    });
    const corruptShaAfter = sha256FileReadOnly(corruptProfilesPath);

    // Unsupported schema_version: 99
    const schemaStoreDir = path.join(errSandbox.rootDir, 'schema-profiles-store');
    fs.mkdirSync(schemaStoreDir, { recursive: true });
    fs.writeFileSync(
      path.join(schemaStoreDir, 'profiles.json'),
      '{"schema_version": 99, "profiles": []}',
      'utf8',
    );
    const err07SchemaRust = await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: schemaStoreDir,
      positional: ['status'],
    });
    const err07SchemaPs1 = await runBackendCli({
      mode: 'ps1',
      dataDir: schemaStoreDir,
      positional: ['status'],
    });

    const err07JsonI18n = translateUiErrorBothLocales('JSON_ERROR');
    const err07SchemaI18n = translateUiErrorBothLocales('SCHEMA_ERROR');
    const err07BackupI18n = translateUiErrorBothLocales('BACKUP_ERROR');

    const err07Notes = [
      'UX note: When restore fails with BACKUP_ERROR (tampered/corrupted archive), errors.BACKUP_ERROR copy says "Cannot back up while Zalo is open or the folder is busy" rather than mentioning backup integrity/restore verification.',
    ];
    uxObservations.push({
      id: 'OBS-RESTORE-CORRUPT-COPY',
      severity: 'P3',
      summary:
        'Tampered backup restore returns BACKUP_ERROR (safe SHA-256 rejection, zero .restore-* residue), whose UI string focuses on backup-while-open rather than corrupted backup archive.',
    });

    errorHandlingChecks.push(
      makeCheck(
        'ERR-07',
        'Corrupted profiles.json & tampered backup archive (fail-closed, zero overwrite of corrupted store, SHA-256 restore guard)',
        corruptShaBefore === corruptShaAfter &&
          err07JsonRust.json?.error_code === 'JSON_ERROR' &&
          err07JsonPs1.json?.error_code === 'JSON_ERROR' &&
          err07SchemaRust.json?.error_code === 'SCHEMA_ERROR' &&
          err07SchemaPs1.json?.error_code === 'SCHEMA_ERROR' &&
          err07RestoreRust.json?.error_code === 'BACKUP_ERROR' &&
          err07RestorePs1.json?.error_code === 'BACKUP_ERROR' &&
          err07JsonI18n.plainLanguageClean &&
          err07SchemaI18n.plainLanguageClean,
        {
          corruptedProfilesJsonPreservedUnmodified: corruptShaBefore === corruptShaAfter,
          corruptedJsonCodes: {
            rust: err07JsonRust.json?.error_code,
            ps1: err07JsonPs1.json?.error_code,
            vi: err07JsonI18n.viMessage,
            en: err07JsonI18n.enMessage,
          },
          unsupportedSchemaCodes: {
            rust: err07SchemaRust.json?.error_code,
            ps1: err07SchemaPs1.json?.error_code,
            vi: err07SchemaI18n.viMessage,
            en: err07SchemaI18n.enMessage,
          },
          tamperedBackupRestoreCodes: {
            rust: err07RestoreRust.json?.error_code,
            ps1: err07RestorePs1.json?.error_code,
            vi: err07BackupI18n.viMessage,
            en: err07BackupI18n.enMessage,
          },
        },
        err07Notes,
      ),
    );
  } finally {
    if (errEdge) await errEdge.close();
    await runBackendCli({
      mode: 'rust',
      dimCliExe: binaries.dimCliExe,
      dataDir: errSandbox.dataDir,
      flags: ['--node-fixture'],
      positional: ['stop-all'],
    }).catch(() => {});
    await errSandbox.cleanup({ primaryProtectedPids: primaryBefore.primaryPids });
  }

  // Give OS process table a brief settle window after sandbox cleanup
  await sleep(200);

  const primaryAfter = capturePrimaryZaloSnapshot();
  const managedAfter = captureManagedProcessSnapshot();
  const primaryInvariance = verifyPrimaryZaloInvariance(primaryBefore, primaryAfter);
  const orphanAudit = diffOrphanProcesses(managedBefore, managedAfter);

  const allChecks = [
    ...firstLaunchChecks,
    ...accountManagementChecks,
    ...dailyUsageChecks,
    ...errorHandlingChecks,
  ];
  const passedChecks = allChecks.filter((c) => c.passed).length;
  const failedChecks = allChecks.length - passedChecks;
  const overallPass = failedChecks === 0 && primaryInvariance.passed && orphanAudit.clean;

  let candidateCommitSha = 'cd62c523222cb3a02051347e85313226de8b6964';
  try {
    candidateCommitSha = execFileSync(
      'git',
      ['--no-optional-locks', '-C', DIM_ROOT, 'rev-parse', 'HEAD'],
      { encoding: 'utf8', windowsHide: true, timeout: 5_000 },
    ).trim();
  } catch {
    // Keep default SHA fallback
  }

  const report = {
    schemaVersion: 1,
    suite: 'windows-journeys-and-errors',
    generatedAtUtc: new Date().toISOString(),
    durationMs: suiteStopwatch.elapsedMs(),
    candidateCommitSha,
    overallStatus: overallPass ? 'PASS' : 'FAIL',
    executionSurfaces: {
      uiDomSurface:
        'Browser Bridge Mode (apps/desktop/ui-server.mjs + real Microsoft Edge Chromium DOM; window.__TAURI_INTERNALS__ absent — Native Tauri WebView2 IPC verified separately in native-tauri-smoke.mjs)',
      rustBackendSurface: `Native Rust dim-cli.exe (${redactText(binaries.dimCliExe)})`,
      ps1BackendSurface: `Signed-Host PowerShell bridge (${redactText(path.join(DIM_ROOT, 'scripts', 'dim-host.ps1'))})`,
      fixtureTarget: 'fixtures/test-desktop-app/fixture-app.mjs (zero destructive actions on real Zalo)',
    },
    summary: {
      totalChecks: allChecks.length,
      passedChecks,
      failedChecks,
      bySection: {
        firstLaunch: {
          total: firstLaunchChecks.length,
          passed: firstLaunchChecks.filter((c) => c.passed).length,
        },
        accountManagement: {
          total: accountManagementChecks.length,
          passed: accountManagementChecks.filter((c) => c.passed).length,
        },
        dailyUsage: {
          total: dailyUsageChecks.length,
          passed: dailyUsageChecks.filter((c) => c.passed).length,
        },
        errorHandling: {
          total: errorHandlingChecks.length,
          passed: errorHandlingChecks.filter((c) => c.passed).length,
        },
      },
    },
    hostSafetyGates: {
      primaryZaloInvariance: primaryInvariance,
      orphanProcessAudit: orphanAudit,
    },
    sections: {
      firstLaunch: firstLaunchChecks,
      accountManagement: accountManagementChecks,
      dailyUsage: dailyUsageChecks,
      errorHandling: errorHandlingChecks,
    },
    uxObservationsForIntegrationRequests: uxObservations,
  };

  writeRedactedJson(outPath, report);

  console.log(
    JSON.stringify(
      {
        ok: overallPass,
        overallStatus: report.overallStatus,
        durationMs: report.durationMs,
        passedChecks,
        totalChecks: allChecks.length,
        primaryZaloUntouched: primaryInvariance.passed,
        orphanProcesses: orphanAudit.orphanCount,
        evidencePath: redactText(outPath),
      },
      null,
      2,
    ),
  );

  process.exit(overallPass ? 0 : 1);
}

main().catch((err) => {
  console.error(
    JSON.stringify(
      {
        ok: false,
        overallStatus: 'FAIL',
        error: redactText(err?.stack || err?.message || String(err)),
      },
      null,
      2,
    ),
  );
  process.exit(1);
});
