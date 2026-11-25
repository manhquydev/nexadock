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
  pickNextAvailableColorId,
  classifyErrorBehavior,
  applyAdvancedMode,
  getSavedAdvancedMode,
  syncAdvancedModeFromStorage,
  selectView
} from '../app.js';
import { initI18n, setLocale, getLocale, LOCALE_STORAGE_KEY } from '../i18n/index.js';

await initI18n();

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const uiDir = path.resolve(__dirname, '..');

const html = fs.readFileSync(path.join(uiDir, 'index.html'), 'utf8');
const css = fs.readFileSync(path.join(uiDir, 'styles.css'), 'utf8');
const appJs = fs.readFileSync(path.join(uiDir, 'app.js'), 'utf8');
const themeInitJs = fs.readFileSync(path.join(uiDir, 'theme-init.js'), 'utf8');
const vaultPanelJs = fs.readFileSync(path.join(uiDir, 'vault-panel.js'), 'utf8');
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
  assert.match(html, /<fieldset[^>]*>[\s\S]*?<legend[^>]*>[\s\S]*?name="create-session-mode"/i, 'Session mode selector must use native radio inputs inside fieldset/legend');
  assert.match(html, /id="account-rail"[\s\S]*?role="toolbar"/i, 'Account rail must expose vertical toolbar semantics');
  assert.match(html, /id="stage-web-session"/i, 'Unified surface must include #stage-web-session');
  assert.match(html, /id="stage-desktop-session"/i, 'Unified surface must include #stage-desktop-session');
  assert.match(html, /id="quick-switcher-dialog"[^>]*closedby="any"/i, 'Quick Switcher must use native <dialog closedby="any">');
  assert.match(html, /id="home-toolbar"[^>]*role="toolbar"/i, '#home-toolbar must expose role="toolbar"');
  assert.match(html, /id="web-limits-tip"[^>]*role="region"[^>]*aria-labelledby="web-limits-title"/i, '#web-limits-tip must use non-tooltip region semantics');
  assert.doesNotMatch(html, /id="web-limits-tip"[^>]*role="tooltip"/i, 'Rich #web-limits-tip must not use role="tooltip"');
  assert.match(html, /id="web-session-menu"[\s\S]*?role="menu"[\s\S]*?role="menuitem"/i, '#web-session-menu must expose menu and menuitem roles');
  assert.match(html, /id="backups-empty"[\s\S]*?<h2\s+class="empty-title"/i, '#backups-empty must use an <h2> heading');
  assert.match(html, /id="profiles-empty"[\s\S]*?id="btn-clear-search"/i, '#profiles-empty must include #btn-clear-search');
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
  assert.match(css, /scrollbar-color:/, 'Custom scrollbar color must be configured');
  assert.match(css, /@supports\s+not\s*\(\s*scrollbar-color:\s*auto\s*\)\s*\{[^}]*scrollbar-width:\s*auto;/s, 'WebKit scrollbar fallback must reset scrollbar-width: auto');
  assert.match(css, /\.account-grid\s*\{[^}]*align-items:\s*stretch;/s, '.account-grid must align-items: stretch');
  assert.match(css, /\.account-name\s*\{[^}]*-webkit-line-clamp:\s*2;[^}]*overflow-wrap:\s*anywhere;/s, '.account-name must clamp to 2 lines with overflow-wrap: anywhere');
  assert.match(css, /\.account-note\s*\{[^}]*-webkit-line-clamp:\s*2;[^}]*overflow-wrap:\s*anywhere;/s, '.account-note must clamp to 2 lines with overflow-wrap: anywhere');
  assert.match(css, /\.rail-item-name\s*\{[^}]*line-height:\s*1\.35;/s, '.rail-item-name must use line-height: 1.35 for tall Vietnamese diacritics');
  assert.match(css, /\.account-avatar\s*\{[^}]*inline-size:\s*2\.625rem;[^}]*block-size:\s*2\.625rem;/s, '.account-avatar must use rem dimensions');
  assert.match(css, /dialog\.modal\s*\{[^}]*overflow-y:\s*auto;/s, 'dialog.modal must support overflow-y: auto');
  assert.doesNotMatch(css, /filter:\s*brightness/i, 'Buttons must use semantic tokens instead of filter: brightness');
  assert.match(css, /\.btn-link\s*\{[^}]*min-block-size:\s*1\.5rem;/s, '.btn-link must satisfy WCAG 2.5.8 24px min-block-size');
  assert.match(css, /\.toast-info\b[\s\S]*?\.toast-warn\b/s, 'Toast must define .toast-info and .toast-warn variants');
  assert.match(css, /\.badge-neutral\b/s, 'Badge must define .badge-neutral');
  assert.match(css, /\.rail-tooltip\.is-floating-open\s*\{[^}]*white-space:\s*normal;[^}]*overflow-wrap:\s*anywhere;/s, 'Floating tooltip must wrap long names cleanly');
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
    'verify_isolation',
    'get_workspace_snapshot',
    'activate_account',
    'set_account_pinned',
    'set_companion_snap',
    'web_session_create',
    'web_session_show',
    'web_session_set_bounds',
    'web_session_hide_all',
    'web_session_reload',
    'web_session_reset',
    'web_session_delete',
    'web_session_rename'
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
    /['"]nexadock\.theme['"]/,
    'theme-init.js must read nexadock.theme'
  );
  assert.match(
    appJs,
    /['"]nexadock\.theme['"]/,
    'app.js must persist theme under nexadock.theme'
  );
  assert.match(
    appJs,
    /'set_keep_in_tray'\s*,\s*\{\s*enabled:/,
    'set_keep_in_tray must pass { enabled }'
  );
  assert.match(
    appJs,
    /'set_companion_snap'\s*,\s*\{[\s\S]*?\brailWidthPx\b/,
    'set_companion_snap must pass railWidthPx'
  );
  assert.match(
    appJs,
    /isComposing\s*\|\|\s*event\.keyCode\s*===\s*229/,
    'Quick Switcher Enter handler must be IME-safe'
  );
  assert.match(
    appJs,
    /\bactivationSeq\b[\s\S]*?\bwebSyncSeq\b/,
    'app.js must guard concurrent account activations and WebView sync via monotonic sequences'
  );
  assert.match(
    appJs,
    /matchMedia\(`\(resolution:\s*\$\{dpr\}dppx\)`\)/,
    'app.js must observe devicePixelRatio changes via matchMedia resolution query'
  );
  assert.match(
    i18nJs,
    /\[data-i18n-title\]/,
    'applyDomTranslations must localize data-i18n-title attributes'
  );
  assert.match(
    html,
    /id="stage-web-session"[\s\S]*?badge-warn[\s\S]*?data-i18n="webSession\.badge"/,
    'Web Session stage must visibly badge WebIsolated as Experimental'
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

test('account color persistence saves by profile id, survives rename, avoids 8-way collision, and removes cleanly', async () => {
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
    const savedAfterDelete = JSON.parse(store.get('nexadock.accountColors') || '{}');
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
    const mapAfterOneRemoved = JSON.parse(store.get('nexadock.accountColors') || '{}');
    assert.equal(Object.hasOwn(mapAfterOneRemoved, 'prof-seq-4'), false);
    assert.equal(Object.keys(mapAfterOneRemoved).length, 7);

    // Verify legacy multi-zalo.accountColors / multi-zalo.theme fallback+migration
    store.clear();
    store.set('multi-zalo.accountColors', JSON.stringify({ 'legacy-1': 'teal' }));
    store.set('multi-zalo.theme', 'dark');
    assert.equal(getAccountColorId({ id: 'legacy-1', name: 'Legacy' }), 'teal');
    assert.equal(JSON.parse(store.get('nexadock.accountColors') || '{}')['legacy-1'], 'teal');
    const { getSavedThemePreference } = await import('../app.js');
    assert.equal(getSavedThemePreference(), 'dark');
    assert.equal(store.get('nexadock.theme'), 'dark');
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

function contrastRatio(hexA, hexB) {
  const lumA = relativeLuminance(hexA);
  const lumB = relativeLuminance(hexB);
  const lighter = Math.max(lumA, lumB);
  const darker = Math.min(lumA, lumB);
  return (lighter + 0.05) / (darker + 0.05);
}

test('semantic button & accent tokens in styles.css meet WCAG 2.2 AA >= 4.5:1 in light and dark themes', () => {
  const vars = new Map();
  for (const match of css.matchAll(/(--[a-z0-9-]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;/g)) {
    vars.set(match[1], match[2]);
  }
  const pairs = [
    ['light .btn-primary', '--accent-text-light', '--accent-light'],
    ['dark .btn-primary', '--accent-text-dark', '--accent-dark'],
    ['light .btn-primary.btn-tonal', '--accent-tonal-fg-light', '--accent-soft-light'],
    ['dark .btn-primary.btn-tonal', '--accent-tonal-fg-dark', '--accent-soft-dark'],
    ['light .btn-secondary', '--text-primary-light', '--bg-surface-light'],
    ['dark .btn-secondary', '--text-primary-dark', '--bg-surface-dark'],
    ['light .btn-subtle-danger', '--danger-fg-light', '--bg-surface-light'],
    ['dark .btn-subtle-danger', '--danger-fg-dark', '--bg-surface-dark'],
    ['light .btn-danger', '--accent-text-light', '--danger-solid-light'],
    ['dark .btn-danger', '--accent-text-dark', '--danger-solid-dark'],
    ['light .btn-link', '--accent-tonal-fg-light', '--bg-surface-light'],
    ['dark .btn-link', '--accent-tonal-fg-dark', '--bg-surface-dark']
  ];
  for (const [label, fgToken, bgToken] of pairs) {
    const fg = vars.get(fgToken);
    const bg = vars.get(bgToken);
    assert.ok(fg && bg, `Missing color token for ${label}: ${fgToken}=${fg}, ${bgToken}=${bg}`);
    const ratio = contrastRatio(fg, bg);
    assert.ok(ratio >= 4.5, `${label} (${fg} on ${bg}) contrast is ${ratio.toFixed(2)}:1, expected >= 4.5:1`);
  }
});

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

test('classifyErrorBehavior distinguishes security guards, action-required, non-retryable, critical, and retryable errors', () => {
  const guard = classifyErrorBehavior('PROTECTED_PRIMARY_PROCESS');
  assert.equal(guard.category, 'info_only');
  assert.equal(guard.retryable, false);
  assert.equal(guard.liveRole, 'status');
  assert.equal(guard.tone, 'info');

  assert.equal(classifyErrorBehavior('PROTECTED_PRIMARY_PATH').retryable, false);
  assert.equal(classifyErrorBehavior('PATH_TRAVERSAL_ATTEMPT').retryable, false);
  assert.equal(classifyErrorBehavior('PATH_TRAVERSAL_ATTEMPT').liveRole, 'status');

  const dup = classifyErrorBehavior('DUPLICATE_ZALO_USER_ID');
  assert.equal(dup.category, 'action_required');
  assert.equal(dup.retryable, false);
  assert.equal(dup.liveRole, 'status');
  assert.equal(dup.action, 'open_diagnostics');

  const invalid = classifyErrorBehavior('INVALID_NAME');
  assert.equal(invalid.category, 'non_retryable');
  assert.equal(invalid.retryable, false);
  assert.equal(invalid.liveRole, 'status');

  for (const rustNonRetryable of ['NOT_FOUND', 'DUPLICATE_NAME', 'CONFIRM_REQUIRED', 'SCHEMA_ERROR']) {
    assert.equal(classifyErrorBehavior(rustNonRetryable).retryable, false);
    assert.equal(classifyErrorBehavior(rustNonRetryable).liveRole, 'status');
  }
  for (const guardCode of ['PRIMARY_PROTECTED', 'NOT_RUNNING', 'WEB_WORKSPACE_REQUIRES_MACOS_14']) {
    assert.equal(classifyErrorBehavior(guardCode).category, 'info_only');
    assert.equal(classifyErrorBehavior(guardCode).retryable, false);
    assert.equal(classifyErrorBehavior(guardCode).liveRole, 'status');
  }
  for (const actionCode of ['UNVERIFIED_BINARY', 'PROFILE_MARKER_MISMATCH', 'EXTERNAL_DB_OUTSIDE_PROFILE']) {
    assert.equal(classifyErrorBehavior(actionCode).category, 'action_required');
    assert.equal(classifyErrorBehavior(actionCode).retryable, false);
    assert.equal(classifyErrorBehavior(actionCode).liveRole, 'status');
  }

  const crit = classifyErrorBehavior('HOST_EXEC_FAILED');
  assert.equal(crit.category, 'critical_system');
  assert.equal(crit.retryable, true);
  assert.equal(crit.liveRole, 'alert');

  const proc = classifyErrorBehavior('PROCESS_ERROR');
  assert.equal(proc.category, 'retryable');
  assert.equal(proc.retryable, true);
  assert.equal(proc.liveRole, 'alert');
});

test('theme-init reads nexadock.advancedMode synchronously before first paint', () => {
  assert.match(themeInitJs, /localStorage\.getItem\(\s*['"]nexadock\.advancedMode['"]\s*\)\s*===\s*['"]true['"]/);
  assert.match(themeInitJs, /setAttribute\(\s*['"]data-advanced-mode['"]/);
});

test('applyAdvancedMode is safe when document is undefined', () => {
  const prevStorage = globalThis.localStorage;
  const prevDocument = globalThis.document;
  const backing = new Map();
  globalThis.localStorage = {
    getItem: (k) => (backing.has(k) ? backing.get(k) : null),
    setItem: (k, v) => backing.set(k, String(v)),
    removeItem: (k) => backing.delete(k)
  };
  delete globalThis.document;
  try {
    applyAdvancedMode(false);
    assert.equal(backing.get('nexadock.advancedMode'), 'false');
    applyAdvancedMode(true);
    assert.equal(backing.get('nexadock.advancedMode'), 'true');
  } finally {
    if (prevStorage === undefined) delete globalThis.localStorage;
    else globalThis.localStorage = prevStorage;
    if (prevDocument !== undefined) globalThis.document = prevDocument;
  }
});

test('Everyday Vault nav/view stays accessible while diagnostics and MTProto vault controls stay Advanced-only', () => {
  const vaultNavMatch = html.match(/<button[^>]*id="btn-nav-vault"[^>]*>/i);
  assert.ok(vaultNavMatch, '#btn-nav-vault must exist in navigation');
  assert.doesNotMatch(vaultNavMatch[0], /\badvanced-only\b/, '#btn-nav-vault must remain visible in Everyday mode');

  const vaultViewMatch = html.match(/<section[^>]*id="view-vault"[^>]*>/i);
  assert.ok(vaultViewMatch, '#view-vault section must exist');
  assert.doesNotMatch(vaultViewMatch[0], /\badvanced-only\b/, '#view-vault must remain visible in Everyday mode');

  const desktopVaultMatch = html.match(/<button[^>]*id="btn-desktop-vault"[^>]*>/i);
  assert.ok(desktopVaultMatch, '#btn-desktop-vault shortcut must exist');
  assert.doesNotMatch(desktopVaultMatch[0], /\badvanced-only\b/, '#btn-desktop-vault must remain visible in Everyday mode');

  const diagNavMatch = html.match(/<button[^>]*data-view="diagnostics"[^>]*>/i);
  assert.ok(diagNavMatch, 'Diagnostics nav button must exist');
  assert.match(diagNavMatch[0], /\badvanced-only\b/, 'Diagnostics nav button must remain gated by .advanced-only');

  assert.match(vaultPanelJs, /vault-mode-option-mtp\b[^'"]*advanced-only/);
  assert.match(vaultPanelJs, /'vault-hint advanced-only'/);
  assert.match(vaultPanelJs, /'vault-form vault-form-phone advanced-only'/);
  assert.match(vaultPanelJs, /'vault-form vault-form-code advanced-only'/);
  assert.match(vaultPanelJs, /'vault-delete-session advanced-only'/);
  assert.match(vaultPanelJs, /'btn btn-primary advanced-only'/);
  assert.match(vaultPanelJs, /details\.className\s*=\s*['"]advanced-details vault-log-details advanced-only['"]/);
});

test('styles.css defines .btn-subtle, valid danger-fg, single [hidden] block, and accessible sidebar/ok contrast', () => {
  assert.match(css, /\.btn-subtle\s*\{/);
  assert.doesNotMatch(css, /--text-danger\b/, 'Undefined --text-danger must not be referenced');
  assert.match(css, /\.vault-error\s*\{[^}]*var\(--danger-fg/s);
  const hiddenRuleCount = (css.match(/^\s*\[hidden\],/gm) || []).length;
  assert.equal(hiddenRuleCount, 1, 'Duplicate [hidden] reset blocks must stay consolidated');

  const vars = new Map();
  for (const match of css.matchAll(/(--[a-z0-9-]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;/g)) {
    vars.set(match[1], match[2]);
  }
  const sidebarRatio = contrastRatio(vars.get('--text-secondary-light'), vars.get('--bg-sidebar-light'));
  assert.ok(sidebarRatio >= 4.5, `--text-secondary-light on --bg-sidebar-light contrast ${sidebarRatio.toFixed(2)}:1 < 4.5:1`);
  const okRatio = contrastRatio(vars.get('--ok-fg-light'), vars.get('--ok-bg-light'));
  assert.ok(okRatio >= 4.5, `--ok-fg-light on --ok-bg-light contrast ${okRatio.toFixed(2)}:1 < 4.5:1`);
});

test('applyAdvancedMode keeps in-memory state synced on storage failures and selectView blocks diagnostics in Everyday mode', () => {
  const prevStorage = globalThis.localStorage;
  const prevDocument = globalThis.document;
  globalThis.localStorage = {
    getItem: () => 'false',
    setItem: () => {
      throw new Error('QuotaExceededError');
    },
    removeItem: () => {}
  };
  delete globalThis.document;
  try {
    syncAdvancedModeFromStorage();
    assert.equal(getSavedAdvancedMode(), false);
    assert.equal(selectView('diagnostics'), 'settings', 'Everyday mode must reroute diagnostics navigation to settings');

    // Even when localStorage.setItem throws, in-memory state tracks Advanced mode cleanly.
    applyAdvancedMode(true);
    assert.equal(getSavedAdvancedMode(), true);
    assert.equal(selectView('diagnostics'), 'diagnostics');

    applyAdvancedMode(false);
    assert.equal(getSavedAdvancedMode(), false);
    assert.equal(selectView('diagnostics'), 'settings');
  } finally {
    applyAdvancedMode(false);
    if (prevStorage === undefined) delete globalThis.localStorage;
    else globalThis.localStorage = prevStorage;
    if (prevDocument !== undefined) globalThis.document = prevDocument;
  }

  const cdpJs = fs.readFileSync(path.join(__dirname, 'visual-cdp.mjs'), 'utf8');
  assert.match(cdpJs, /getElementById\(['"]btn-help-diagnostics['"]\)\?\.closest\(['"]\.advanced-only['"]\)/);
});
