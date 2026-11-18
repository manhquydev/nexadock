import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  resolveAccountState,
  resolveIsolationCheckResult,
  getAccountInitials,
  getAccountColorId,
  saveAccountColor,
  removeAccountColor,
  pickNextAvailableColorId
} from '../app.js';
import { initI18n, setLocale, getLocale, LOCALE_STORAGE_KEY } from '../i18n/index.js';

await initI18n();

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const uiDir = path.resolve(__dirname, '..');

const html = fs.readFileSync(path.join(uiDir, 'index.html'), 'utf8');
const css = fs.readFileSync(path.join(uiDir, 'styles.css'), 'utf8');
const appJs = fs.readFileSync(path.join(uiDir, 'app.js'), 'utf8');
const themeInitJs = fs.readFileSync(path.join(uiDir, 'theme-init.js'), 'utf8');
const i18nJs = fs.readFileSync(path.join(uiDir, 'i18n', 'index.js'), 'utf8');

test('HTML adheres to Modern Web Guidance, native radio groups, & CSP rules', () => {
  assert.match(html, /<meta\s+name="color-scheme"\s+content="light dark">/i);
  assert.match(html, /<meta\s+name="text-scale"\s+content="scale">/i);
  assert.match(html, /<script\s+src="\.\/theme-init\.js"><\/script>/i);
  assert.doesNotMatch(html, /\brole="dialog"/i, 'Native <dialog> must not have role="dialog"');
  assert.doesNotMatch(html, /\baria-modal=/i, 'Native <dialog> must not have aria-modal');
  assert.doesNotMatch(html, /https?:\/\//i, 'No remote CDNs, fonts, or external URLs allowed');
  assert.match(html, /id="delete-profile-dialog"[\s\S]*?value="cancel"[^>]*autofocus/i, 'Cancel button in delete dialog must have autofocus');
  assert.match(html, /<fieldset[^>]*>[\s\S]*?<legend[^>]*>[\s\S]*?name="ui-locale"/i, 'Language control must use native radio inputs inside fieldset/legend');
  assert.match(html, /<fieldset[^>]*>[\s\S]*?<legend[^>]*>[\s\S]*?name="ui-theme"/i, 'Appearance control must use native radio inputs inside fieldset/legend');
});

test('CSS adheres to Modern Web Guidance & Quiet Workspace rules', () => {
  assert.match(css, /color-scheme:\s*light dark;/);
  assert.match(css, /light-dark\(/);
  assert.match(css, /@media\s*\(prefers-color-scheme:\s*dark\)/);
  assert.match(css, /dialog:modal\s*\{[^}]*overscroll-behavior:\s*contain;/s);
  assert.match(css, /dialog::backdrop\s*\{[^}]*overflow:\s*hidden;[^}]*overscroll-behavior:\s*contain;/s);
  assert.doesNotMatch(css, /linear-gradient|radial-gradient|conic-gradient/i, 'Quiet Workspace forbids flashy gradients');
  assert.doesNotMatch(css, /backdrop-filter/i, 'Quiet Workspace forbids glassmorphism');
  assert.doesNotMatch(css, /@import\s+url\(|https?:\/\//i, 'No remote fonts or URLs in CSS');
  assert.doesNotMatch(css, /\bfont-size\s*:\s*\d+(?:\.\d+)?px\b/i, 'All CSS font-size rules must use rem/em to respect OS text scaling');
});

test('JS preserves Tauri & HTTP contracts, cross-platform WebView compat, & forbids innerHTML', () => {
  assert.doesNotMatch(appJs, /\.innerHTML\b/);
  assert.doesNotMatch(i18nJs, /\.innerHTML\b/);
  assert.doesNotMatch(i18nJs, /\bwith\s*\{\s*type:\s*['"]json['"]\s*\}/, 'Avoid import attributes for broader WebView compatibility');
  assert.match(appJs, /pendingProfileActions/);
  assert.match(appJs, /pendingGlobalActions/);
  assert.match(appJs, /aria-haspopup/);

  const requiredTauriCommands = [
    'get_status',
    'create_profile',
    'rename_profile',
    'delete_profile',
    'launch_profile',
    'focus_profile',
    'stop_profile',
    'backup_profile',
    'list_backups',
    'restore_backup',
    'get_diagnostics',
    'verify_isolation'
  ];
  for (const cmd of requiredTauriCommands) {
    assert.ok(appJs.includes(`'${cmd}'`), `Missing Tauri command ${cmd}`);
  }

  const requiredRoutes = [
    '/api/status',
    '/api/backups',
    '/api/diagnostics',
    '/api/verify-isolation',
    '/api/profiles',
    '/api/backups/restore'
  ];
  for (const route of requiredRoutes) {
    assert.ok(appJs.includes(route), `Missing HTTP route ${route}`);
  }

  assert.match(
    appJs,
    /'delete_profile'\s*,\s*\{[^}]*\bconfirm:\s*true\b/,
    'delete_profile must pass confirm: true in tauriArgs'
  );
  assert.match(
    appJs,
    /meta\[name="dim-session-token"\]/,
    'app.js must read dim-session-token meta tag'
  );
  assert.match(
    appJs,
    /['"]X-DIM-Session-Token['"]/,
    'app.js must attach X-DIM-Session-Token request header'
  );
  assert.match(
    themeInitJs,
    /['"]multi-zalo\.theme['"]/,
    'theme-init.js must read multi-zalo.theme'
  );
  assert.match(
    appJs,
    /['"]multi-zalo\.theme['"]/,
    'app.js must persist theme under multi-zalo.theme'
  );
});

test('resolveIsolationCheckResult never converts NOT_TESTED, BLOCKED, or UNKNOWN into PASS', () => {
  assert.equal(resolveIsolationCheckResult({ status: 'NOT_TESTED', passed: true }).raw, 'NOT_TESTED');
  assert.equal(resolveIsolationCheckResult({ status: 'BLOCKED', passed: false }).raw, 'BLOCKED');
  assert.equal(resolveIsolationCheckResult({ status: 'UNKNOWN' }).raw, 'UNKNOWN');
  assert.equal(resolveIsolationCheckResult({ passed: true }).raw, 'PASS');
  assert.equal(resolveIsolationCheckResult({ passed: false }).raw, 'FAIL');
  assert.equal(resolveIsolationCheckResult({}).raw, 'UNKNOWN');
});

test('resolveAccountState maps all ProfileLifecycleState values accurately in vi-VN and en-US', () => {
  setLocale('en-US');
  assert.deepEqual(resolveAccountState({ id: 'p1', running: false, lifecycle_state: 'stopped' }), {
    label: 'Closed',
    kind: 'stopped'
  });
  assert.deepEqual(resolveAccountState({ id: 'p1', running: true, lifecycle_state: 'starting' }), {
    label: 'Opening Zalo...',
    kind: 'pending'
  });
  assert.deepEqual(resolveAccountState({ id: 'p1', running: true, lifecycle_state: 'running' }), {
    label: 'Window open',
    kind: 'running'
  });
  assert.deepEqual(resolveAccountState({ id: 'p1', running: true, lifecycle_state: 'stopping' }), {
    label: 'Closing Zalo...',
    kind: 'pending'
  });
  assert.deepEqual(resolveAccountState({ id: 'p1', running: false, lifecycle_state: 'RecoveryRequired' }), {
    label: 'Reopen to recover',
    kind: 'error'
  });
  setLocale('vi-VN');
  assert.equal(resolveAccountState({ id: 'p1', running: true, lifecycle_state: 'starting' }).label, 'Đang mở ứng dụng...');
  assert.equal(resolveAccountState({ id: 'p1', running: true, lifecycle_state: 'stopping' }).label, 'Đang đóng ứng dụng...');
  assert.equal(resolveAccountState({ id: 'p1', running: false, lifecycle_state: 'recovery_required' }).label, 'Cần mở lại để khôi phục');
});

test('getAccountInitials derives clean initials for single and multi-word Vietnamese names', () => {
  assert.equal(getAccountInitials('Cá nhân'), 'CN');
  assert.equal(getAccountInitials('Đức - Công việc'), 'ĐV');
  assert.equal(getAccountInitials('Shop'), 'SH');
  assert.equal(getAccountInitials(''), 'Z');
});

function createMockLocalStorage() {
  const store = new Map();
  return {
    store,
    storage: {
      getItem: (k) => (store.has(k) ? store.get(k) : null),
      setItem: (k, v) => store.set(k, String(v)),
      removeItem: (k) => store.delete(k)
    }
  };
}

test('account color persistence saves by profile id, survives rename, avoids 8-way collision, and removes cleanly', () => {
  const prevStorage = globalThis.localStorage;
  const { store, storage } = createMockLocalStorage();
  globalThis.localStorage = storage;

  try {
    saveAccountColor('prof-persist-1', 'coral');
    assert.equal(getAccountColorId({ id: 'prof-persist-1', name: 'Công việc' }), 'coral');
    assert.equal(getAccountColorId({ id: 'prof-persist-1', name: 'Tên mới sau khi đổi' }), 'coral');

    saveAccountColor('prof-persist-1', 'plum');
    assert.equal(getAccountColorId({ id: 'prof-persist-1', name: 'Đức — Đối tác' }), 'plum');

    removeAccountColor('prof-persist-1');
    const savedAfterDelete = JSON.parse(store.get('multi-zalo.accountColors') || '{}');
    assert.equal(Object.hasOwn(savedAfterDelete, 'prof-persist-1'), false);

    const profiles = [];
    const assigned = [];
    for (let i = 1; i <= 8; i += 1) {
      const nextColor = pickNextAvailableColorId(profiles);
      assigned.push(nextColor);
      const id = `prof-seq-${i}`;
      saveAccountColor(id, nextColor);
      profiles.push({ id, name: `Tài khoản ${i}` });
    }
    assert.equal(assigned.length, 8);
    assert.equal(new Set(assigned).size, 8, `Expected 8 distinct colors, got ${assigned.join(', ')}`);

    removeAccountColor('prof-seq-4');
    const mapAfterOneRemoved = JSON.parse(store.get('multi-zalo.accountColors') || '{}');
    assert.equal(Object.hasOwn(mapAfterOneRemoved, 'prof-seq-4'), false);
    assert.equal(Object.keys(mapAfterOneRemoved).length, 7);
  } finally {
    if (prevStorage === undefined) delete globalThis.localStorage;
    else globalThis.localStorage = prevStorage;
  }
});

function relativeLuminance(hex) {
  const clean = hex.replace(/^#/, '');
  const r = parseInt(clean.slice(0, 2), 16) / 255;
  const g = parseInt(clean.slice(2, 4), 16) / 255;
  const b = parseInt(clean.slice(4, 6), 16) / 255;
  const toLinear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * toLinear(r) + 0.7152 * toLinear(g) + 0.0722 * toLinear(b);
}

function contrastRatioAgainstWhite(hex) {
  const lum = relativeLuminance(hex);
  return (1.0 + 0.05) / (lum + 0.05);
}

test('all 8 account identity --id-* hex colors in styles.css meet WCAG 2.2 AA >= 4.5:1 against #ffffff', () => {
  const expectedTokens = [
    '--id-blue',
    '--id-teal',
    '--id-green',
    '--id-amber',
    '--id-coral',
    '--id-plum',
    '--id-indigo',
    '--id-slate'
  ];
  const tokens = new Map();
  for (const match of css.matchAll(/(--id-[a-z]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;/g)) {
    tokens.set(match[1], match[2]);
  }
  assert.equal(tokens.size, 8, `Expected 8 --id-* hex tokens, found ${tokens.size}`);
  for (const token of expectedTokens) {
    const hex = tokens.get(token);
    assert.ok(hex, `Missing hex value for ${token} in styles.css`);
    const ratio = contrastRatioAgainstWhite(hex);
    assert.ok(ratio >= 4.5, `${token} (${hex}) contrast against #ffffff is ${ratio.toFixed(2)}:1, expected >= 4.5:1`);
  }
});

test('locale persistence updates localStorage and document.documentElement.lang', () => {
  const prevStorage = globalThis.localStorage;
  const prevDocument = globalThis.document;
  const { store, storage } = createMockLocalStorage();
  globalThis.localStorage = storage;
  globalThis.document = {
    documentElement: { lang: 'vi' },
    title: '',
    querySelectorAll: () => []
  };

  try {
    setLocale('en-US');
    assert.equal(getLocale(), 'en-US');
    assert.equal(store.get(LOCALE_STORAGE_KEY), 'en-US');
    assert.equal(globalThis.document.documentElement.lang, 'en');

    setLocale('vi-VN');
    assert.equal(getLocale(), 'vi-VN');
    assert.equal(store.get(LOCALE_STORAGE_KEY), 'vi-VN');
    assert.equal(globalThis.document.documentElement.lang, 'vi');
  } finally {
    if (prevStorage === undefined) delete globalThis.localStorage;
    else globalThis.localStorage = prevStorage;
    if (prevDocument === undefined) delete globalThis.document;
    else globalThis.document = prevDocument;
  }
});
