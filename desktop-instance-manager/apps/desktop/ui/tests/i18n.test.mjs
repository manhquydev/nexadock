import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  initI18n,
  DEFAULT_LOCALE,
  DICTIONARIES,
  getLocale,
  setLocale,
  t,
  getMissingKeys,
  clearMissingKeys,
  interpolate,
  normalizeSupportStatus,
  translateSupportStatus,
  resolveErrorInfo,
  formatBytes,
  formatDateTime,
  formatNumber,
  applyDomTranslations
} from '../i18n/index.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const uiDir = path.resolve(__dirname, '..');

await initI18n();

function flattenKeys(obj, prefix = '') {
  const out = [];
  for (const [k, v] of Object.entries(obj)) {
    const next = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === 'object' && !Array.isArray(v)) {
      out.push(...flattenKeys(v, next));
    } else {
      out.push(next);
    }
  }
  return out.sort();
}

function extractPlaceholders(val) {
  const str = Array.isArray(val) ? val.join('\n') : String(val ?? '');
  return Array.from(str.matchAll(/\{([a-zA-Z0-9_]+)\}/g), (m) => m[1]).sort();
}

test('default locale is vi-VN', () => {
  assert.equal(DEFAULT_LOCALE, 'vi-VN');
  assert.equal(getLocale(), 'vi-VN');
});

test('vi-VN and en-US dictionaries have 100% key parity, placeholder parity, and non-empty values', () => {
  const viKeys = flattenKeys(DICTIONARIES['vi-VN']);
  const enKeys = flattenKeys(DICTIONARIES['en-US']);

  const missingInEn = viKeys.filter((k) => !enKeys.includes(k));
  const missingInVi = enKeys.filter((k) => !viKeys.includes(k));

  assert.deepEqual(missingInEn, [], `Keys missing in en-US: ${missingInEn.join(', ')}`);
  assert.deepEqual(missingInVi, [], `Keys missing in vi-VN: ${missingInVi.join(', ')}`);
  assert.ok(viKeys.length > 80, 'Expected substantial i18n key coverage');

  for (const k of viKeys) {
    setLocale('vi-VN');
    const viVal = t(k);
    assert.ok(viVal.trim().length > 0, `Empty vi-VN value for ${k}`);
    setLocale('en-US');
    const enVal = t(k);
    assert.ok(enVal.trim().length > 0, `Empty en-US value for ${k}`);

    const viRaw = k.split('.').reduce((acc, part) => acc[part], DICTIONARIES['vi-VN']);
    const enRaw = k.split('.').reduce((acc, part) => acc[part], DICTIONARIES['en-US']);
    assert.deepEqual(
      extractPlaceholders(viRaw),
      extractPlaceholders(enRaw),
      `Placeholder mismatch for key ${k}`
    );
  }
  setLocale('vi-VN');
});

test('vi-VN dictionary contains proper Vietnamese diacritics', () => {
  const viText = JSON.stringify(DICTIONARIES['vi-VN']);
  const hasDiacritics = /[àáạảãâầấậẩẫăằắặẳẵèéẹẻẽêềếệểễìíịỉĩòóọỏõôồốộổỗơờớợởỡùúụủũưừứựửữỳýỵỷỹđ]/i.test(viText);
  assert.ok(hasDiacritics, 'vi-VN.json must contain Vietnamese diacritics');
  assert.match(DICTIONARIES['vi-VN'].nav.accounts, /Tài khoản/);
  assert.match(DICTIONARIES['vi-VN'].nav.backups, /Sao lưu/);
  assert.match(DICTIONARIES['vi-VN'].diagnostics.title, /Kiểm tra sự cố/);
  assert.match(DICTIONARIES['vi-VN'].nav.settings, /Cài đặt/);
});

test('all static i18n keys referenced in index.html and app.js exist in dictionaries', () => {
  const html = fs.readFileSync(path.join(uiDir, 'index.html'), 'utf8');
  const appJs = fs.readFileSync(path.join(uiDir, 'app.js'), 'utf8');
  const allKeys = new Set(flattenKeys(DICTIONARIES['vi-VN']));

  const htmlAttrRegex = /data-i18n(?:-[a-z-]+)?="([^"]+)"/g;
  for (const match of html.matchAll(htmlAttrRegex)) {
    const key = match[1];
    assert.ok(allKeys.has(key), `HTML references unknown i18n key: ${key}`);
  }

  const jsCallRegex = /\bt\(\s*['"]([a-zA-Z0-9_.]+)['"]/g;
  for (const match of appJs.matchAll(jsCallRegex)) {
    const key = match[1];
    assert.ok(allKeys.has(key), `app.js references unknown i18n key: ${key}`);
  }
});

test('safe interpolation treats parameters as plain text and handles missing params', () => {
  const rendered = interpolate('Hello {name}, count={count}', {
    name: '<script>alert(1)</script>',
    count: 3
  });
  assert.equal(rendered, 'Hello <script>alert(1)</script>, count=3');
  assert.equal(interpolate('A {missing} B', {}), 'A  B');
});

test('missing-key detection records unknown keys and returns key string', () => {
  clearMissingKeys();
  const res = t('nonexistent.key.for.test');
  assert.equal(res, 'nonexistent.key.for.test');
  assert.deepEqual(getMissingKeys(), ['nonexistent.key.for.test']);
  clearMissingKeys();
});

test('support status normalization handles both PascalCase and SCREAMING_SNAKE_CASE', () => {
  assert.equal(normalizeSupportStatus('PartiallySupported'), 'PARTIALLY_SUPPORTED');
  assert.equal(normalizeSupportStatus('SupportedAndTested'), 'SUPPORTED_AND_TESTED');
  assert.equal(normalizeSupportStatus('NOT_TESTED'), 'NOT_TESTED');

  setLocale('vi-VN');
  const vi = translateSupportStatus('PartiallySupported');
  assert.equal(vi.code, 'PARTIALLY_SUPPORTED');
  assert.equal(vi.label, 'Hỗ trợ một phần');

  setLocale('en-US');
  const en = translateSupportStatus('NOT_TESTED');
  assert.equal(en.code, 'NOT_TESTED');
  assert.equal(en.label, 'Not tested');
  setLocale('vi-VN');
});

test('resolveErrorInfo separates localized userMessage from rawCode and rawDetail', () => {
  setLocale('vi-VN');
  const info = resolveErrorInfo('PROTECTED_PRIMARY_PROCESS', 'Target PID 4120 is primary Zalo');
  assert.doesNotMatch(info.userMessage, /Target PID 4120/);
  assert.match(info.userMessage, /Zalo đang dùng trên máy được bảo vệ/);
  assert.equal(info.rawCode, 'PROTECTED_PRIMARY_PROCESS');
  assert.equal(info.rawDetail, 'Target PID 4120 is primary Zalo');
});

test('Intl formatters format bytes, numbers, and dates cleanly', () => {
  assert.equal(formatBytes(null), '—');
  assert.equal(formatBytes(512, 'en-US'), '512 B');
  assert.equal(formatBytes(1536, 'en-US'), '1.5 KB');
  assert.equal(formatNumber(1234, 'en-US'), '1,234');
  assert.equal(formatDateTime(null), '—');
  assert.notEqual(formatDateTime('2026-01-15T10:30:00Z', 'vi-VN'), '—');
});

test('initI18n handles HTTP 404, invalid JSON, timeout, and rapid locale switching without raw key flash', async () => {
  const savedVi = DICTIONARIES['vi-VN'];
  const savedEn = DICTIONARIES['en-US'];

  try {
    // 1. HTTP 404 on vi-VN, valid en-US
    globalThis.__TEST_FETCH__ = async (url) => {
      if (String(url).includes('vi-VN')) return { ok: false, status: 404 };
      return { ok: true, status: 200, json: async () => savedEn };
    };
    DICTIONARIES['vi-VN'] = {};
    DICTIONARIES['en-US'] = {};
    const res404 = await initI18n({ force: true });
    assert.equal(res404.ok, false);
    assert.equal(res404.errors.length, 1);

    // 2. Invalid JSON payload
    globalThis.__TEST_FETCH__ = async () => ({
      ok: true,
      status: 200,
      json: async () => { throw new SyntaxError('Unexpected token'); }
    });
    DICTIONARIES['vi-VN'] = {};
    DICTIONARIES['en-US'] = {};
    const resBadJson = await initI18n({ force: true });
    assert.equal(resBadJson.ok, false);
    assert.equal(resBadJson.errors.length, 2);

    // Ensure applyDomTranslations does NOT overwrite static HTML when dictionaries are empty
    let overwritten = false;
    const fakeRoot = {
      querySelectorAll: () => [{ getAttribute: () => 'app.name', set textContent(_v) { overwritten = true; } }]
    };
    applyDomTranslations(fakeRoot);
    assert.equal(overwritten, false);

    // 3. Timeout via AbortController
    globalThis.__TEST_FETCH__ = (_url, opts) => new Promise((_resolve, reject) => {
      opts.signal.addEventListener('abort', () => reject(opts.signal.reason || new Error('Aborted')));
    });
    const resTimeout = await initI18n({ force: true, timeoutMs: 25 });
    assert.equal(resTimeout.ok, false);
    assert.match(String(resTimeout.errors[0]), /timeout/i);
  } finally {
    delete globalThis.__TEST_FETCH__;
    DICTIONARIES['vi-VN'] = savedVi;
    DICTIONARIES['en-US'] = savedEn;
    await initI18n({ force: true });
  }

  // 4. Rapid locale switching settles deterministically on final locale
  setLocale('en-US');
  setLocale('vi-VN');
  setLocale('en-US');
  setLocale('vi-VN');
  assert.equal(getLocale(), 'vi-VN');
  assert.equal(t('nav.accounts'), 'Tài khoản');
});

test('consumer-facing Account Dock copy avoids technical jargon on Home and Add Account flows', () => {
  const vi = JSON.parse(fs.readFileSync(path.join(uiDir, 'i18n', 'vi-VN.json'), 'utf8'));
  const en = JSON.parse(fs.readFileSync(path.join(uiDir, 'i18n', 'en-US.json'), 'utf8'));
  const forbidden = /\b(PID|sandbox|IPC|Authenticode|loopback)\b/i;

  for (const dict of [vi, en]) {
    const consumerSections = [dict.app, dict.nav, dict.statusBar, dict.accounts?.states, dict.accounts?.actions, dict.accounts?.primaryZalo, dict.dialogs?.create, dict.dialogs?.help, dict.help];
    const serialized = JSON.stringify(consumerSections);
    assert.doesNotMatch(serialized, forbidden, 'Consumer copy must not contain technical jargon');
  }
});

test('every Rust DimError::code() and bridge error code resolves to localized non-fallback copy in vi-VN and en-US', () => {
  const rustErrorCodes = [
    'INVALID_NAME',
    'DUPLICATE_NAME',
    'NOT_FOUND',
    'PROFILE_RUNNING',
    'PROFILE_MARKER_MISMATCH',
    'PROTECTED_PRIMARY_PROCESS',
    'PROTECTED_PRIMARY_PATH',
    'DUPLICATE_ZALO_USER_ID',
    'INSUFFICIENT_DISK_SPACE',
    'PATH_TRAVERSAL',
    'FOLDER_REDIRECTION_DETECTED',
    'INSTALL_NOT_FOUND',
    'UNVERIFIED_BINARY',
    'NOT_RUNNING',
    'UNSUPPORTED_PLATFORM',
    'BACKUP_ERROR',
    'SHARED_EXTERNAL_DB_PATH',
    'EXTERNAL_DB_OUTSIDE_PROFILE',
    'RESTORE_ERROR',
    'PROCESS_ERROR',
    'LAUNCH_FAILED',
    'SCHEMA_ERROR',
    'CONFIRM_REQUIRED',
    'IO_ERROR',
    'JSON_ERROR'
  ];
  const bridgeErrorCodes = [
    'HOST_EXEC_FAILED',
    'HOST_TIMEOUT',
    'UNAUTHORIZED',
    'RATE_LIMITED'
  ];
  const allCodes = [...rustErrorCodes, ...bridgeErrorCodes];

  try {
    for (const locale of ['vi-VN', 'en-US']) {
      setLocale(locale);
      const fallback = t('errors.UNKNOWN');
      assert.ok(fallback.trim().length > 0 && fallback !== 'errors.UNKNOWN', `Missing errors.UNKNOWN in ${locale}`);

      for (const code of allCodes) {
        const info = resolveErrorInfo(code, 'raw technical context');
        assert.equal(info.rawCode, code, `rawCode mismatch for ${code} in ${locale}`);
        assert.ok(info.userMessage.trim().length > 0, `Empty error message for ${code} in ${locale}`);
        assert.notEqual(info.userMessage, `errors.${code}`, `Missing translation key for ${code} in ${locale}`);
        assert.notEqual(info.userMessage, fallback, `Error code ${code} fell back to errors.UNKNOWN in ${locale}`);
        assert.equal(info.userMessage, DICTIONARIES[locale].errors[code], `Expected localized ${locale} string for ${code}`);
      }

      const unknownInfo = resolveErrorInfo('UNKNOWN');
      assert.equal(unknownInfo.rawCode, 'UNKNOWN');
      assert.equal(unknownInfo.userMessage, fallback, `UNKNOWN must resolve to errors.UNKNOWN in ${locale}`);
    }
  } finally {
    setLocale('vi-VN');
  }
});
