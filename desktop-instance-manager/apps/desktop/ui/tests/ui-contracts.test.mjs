import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { resolveIsolationCheckResult } from '../app.js';
import { setLocale, getLocale, LOCALE_STORAGE_KEY } from '../i18n/index.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const uiDir = path.resolve(__dirname, '..');

const html = fs.readFileSync(path.join(uiDir, 'index.html'), 'utf8');
const css = fs.readFileSync(path.join(uiDir, 'styles.css'), 'utf8');
const appJs = fs.readFileSync(path.join(uiDir, 'app.js'), 'utf8');
const i18nJs = fs.readFileSync(path.join(uiDir, 'i18n', 'index.js'), 'utf8');

test('HTML adheres to Modern Web Guidance & CSP rules', () => {
  assert.match(html, /<meta\s+name="color-scheme"\s+content="light dark">/i);
  assert.match(html, /<script\s+src="\.\/theme-init\.js"><\/script>/i);
  assert.doesNotMatch(html, /\brole="dialog"/i, 'Native <dialog> must not have role="dialog"');
  assert.doesNotMatch(html, /\baria-modal=/i, 'Native <dialog> must not have aria-modal');
  assert.doesNotMatch(html, /https?:\/\//i, 'No remote CDNs, fonts, or external URLs allowed');
  assert.match(html, /id="delete-profile-dialog"[\s\S]*?value="cancel"[^>]*autofocus/i, 'Cancel button in delete dialog must have autofocus');
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
});

test('JS preserves Tauri & HTTP contracts and forbids innerHTML', () => {
  assert.doesNotMatch(appJs, /\.innerHTML\b/);
  assert.doesNotMatch(i18nJs, /\.innerHTML\b/);

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
});

test('resolveIsolationCheckResult never converts NOT_TESTED, BLOCKED, or UNKNOWN into PASS', () => {
  assert.equal(resolveIsolationCheckResult({ status: 'NOT_TESTED', passed: true }), 'NOT_TESTED');
  assert.equal(resolveIsolationCheckResult({ status: 'BLOCKED', passed: false }), 'BLOCKED');
  assert.equal(resolveIsolationCheckResult({ status: 'UNKNOWN' }), 'UNKNOWN');
  assert.equal(resolveIsolationCheckResult({ passed: true }), 'PASS');
  assert.equal(resolveIsolationCheckResult({ passed: false }), 'FAIL');
  assert.equal(resolveIsolationCheckResult({}), 'UNKNOWN');
});

test('locale persistence updates localStorage and document.documentElement.lang', () => {
  const store = new Map();
  globalThis.localStorage = {
    getItem: (k) => (store.has(k) ? store.get(k) : null),
    setItem: (k, v) => store.set(k, String(v)),
    removeItem: (k) => store.delete(k)
  };
  globalThis.document = {
    documentElement: { lang: 'vi' },
    title: '',
    querySelectorAll: () => []
  };

  setLocale('en-US');
  assert.equal(getLocale(), 'en-US');
  assert.equal(store.get(LOCALE_STORAGE_KEY), 'en-US');
  assert.equal(globalThis.document.documentElement.lang, 'en');

  setLocale('vi-VN');
  assert.equal(getLocale(), 'vi-VN');
  assert.equal(store.get(LOCALE_STORAGE_KEY), 'vi-VN');
  assert.equal(globalThis.document.documentElement.lang, 'vi');

  delete globalThis.localStorage;
  delete globalThis.document;
});
