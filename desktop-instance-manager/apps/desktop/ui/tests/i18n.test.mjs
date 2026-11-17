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
  formatNumber
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

function extractPlaceholders(str) {
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
  assert.match(DICTIONARIES['vi-VN'].nav.profiles, /Hồ sơ Zalo/);
  assert.match(DICTIONARIES['vi-VN'].nav.backups, /Sao lưu/);
  assert.match(DICTIONARIES['vi-VN'].nav.diagnostics, /Chẩn đoán/);
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
  assert.equal(en.label, 'Not Tested');
  setLocale('vi-VN');
});

test('resolveErrorInfo separates localized userMessage from rawCode and rawDetail', () => {
  setLocale('vi-VN');
  const info = resolveErrorInfo('PROTECTED_PRIMARY_PROCESS', 'Target PID 4120 is primary Zalo');
  assert.doesNotMatch(info.userMessage, /Target PID 4120/);
  assert.match(info.userMessage, /Zalo chính/);
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
