export const DEFAULT_LOCALE = 'vi-VN';
export const SUPPORTED_LOCALES = Object.freeze(['vi-VN', 'en-US']);
export const LOCALE_STORAGE_KEY = 'nexadock.locale';
export const LEGACY_LOCALE_STORAGE_KEY = 'multi-zalo.locale';

export const DICTIONARIES = {
  'vi-VN': {},
  'en-US': {}
};

let currentLocale = loadInitialLocale();
let initialized = false;
const listeners = new Set();
const missingKeys = new Set();

function loadInitialLocale() {
  try {
    if (typeof localStorage !== 'undefined') {
      const saved = localStorage.getItem(LOCALE_STORAGE_KEY);
      if (saved && SUPPORTED_LOCALES.includes(saved)) {
        return saved;
      }
      const legacy = localStorage.getItem(LEGACY_LOCALE_STORAGE_KEY);
      if (legacy && SUPPORTED_LOCALES.includes(legacy)) {
        try {
          localStorage.setItem(LOCALE_STORAGE_KEY, legacy);
        } catch (_writeErr) {
          // Ignore storage write errors
        }
        return legacy;
      }
    }
  } catch (_err) {
    // Ignore storage access errors
  }
  return DEFAULT_LOCALE;
}

async function loadDictionary(locale, { timeoutMs = 8000 } = {}) {
  const url = new URL(`./${locale}.json`, import.meta.url);
  if (typeof process !== 'undefined' && process.versions?.node && url.protocol === 'file:' && typeof globalThis.__TEST_FETCH__ !== 'function') {
    const fs = await import('node:fs');
    const path = await import('node:path');
    const { fileURLToPath } = await import('node:url');
    const filePath = fileURLToPath(url);
    return JSON.parse(fs.readFileSync(path.resolve(filePath), 'utf8'));
  }

  const fetchImpl = typeof globalThis.__TEST_FETCH__ === 'function' ? globalThis.__TEST_FETCH__ : fetch;
  const controller = typeof AbortController !== 'undefined' ? new AbortController() : null;
  const timer = controller && timeoutMs > 0
    ? setTimeout(() => controller.abort(new Error(`i18n load timeout after ${timeoutMs}ms`)), timeoutMs)
    : null;

  try {
    const resp = await fetchImpl(url, {
      headers: { Accept: 'application/json' },
      ...(controller ? { signal: controller.signal } : {})
    });
    if (!resp.ok) {
      throw new Error(`Failed to load i18n dictionary for ${locale}: HTTP ${resp.status}`);
    }
    const parsed = await resp.json();
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      throw new Error(`Invalid i18n dictionary payload for ${locale}`);
    }
    return parsed;
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export function hasLoadedDictionary(locale = currentLocale) {
  const dict = DICTIONARIES[locale];
  return Boolean(dict && Object.keys(dict).length > 0);
}

export async function initI18n({ force = false, timeoutMs = 8000 } = {}) {
  if (initialized && !force) return { ok: true, errors: [] };
  const results = await Promise.allSettled([
    loadDictionary('vi-VN', { timeoutMs }),
    loadDictionary('en-US', { timeoutMs })
  ]);
  const errors = [];
  if (results[0].status === 'fulfilled') {
    DICTIONARIES['vi-VN'] = results[0].value;
  } else {
    errors.push(results[0].reason);
  }
  if (results[1].status === 'fulfilled') {
    DICTIONARIES['en-US'] = results[1].value;
  } else {
    errors.push(results[1].reason);
  }
  initialized = hasLoadedDictionary('vi-VN') || hasLoadedDictionary('en-US');
  if (errors.length > 0 && typeof console !== 'undefined' && console.error) {
    console.error('i18n initialization warning:', errors);
  }
  return { ok: errors.length === 0, errors };
}

function resolvePath(dict, key) {
  if (!dict || typeof key !== 'string') return undefined;
  const parts = key.split('.');
  let cur = dict;
  for (const part of parts) {
    if (cur == null || typeof cur !== 'object' || !Object.hasOwn(cur, part)) {
      return undefined;
    }
    cur = cur[part];
  }
  return typeof cur === 'string' ? cur : undefined;
}

export function interpolate(template, params = {}) {
  if (typeof template !== 'string') return '';
  if (!params || typeof params !== 'object') return template;
  return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (_match, name) => {
    if (!Object.hasOwn(params, name) || params[name] == null) {
      return '';
    }
    return String(params[name]);
  });
}

export function getLocale() {
  return currentLocale;
}

export function setLocale(nextLocale) {
  const normalized = SUPPORTED_LOCALES.includes(nextLocale) ? nextLocale : DEFAULT_LOCALE;
  currentLocale = normalized;

  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(LOCALE_STORAGE_KEY, normalized);
    }
  } catch (_err) {
    // Ignore storage access errors
  }

  if (typeof document !== 'undefined' && document.documentElement) {
    document.documentElement.lang = normalized === 'vi-VN' ? 'vi' : 'en';
  }

  for (const fn of listeners) {
    try {
      fn(normalized);
    } catch (_err) {
      // Do not break other listeners
    }
  }
  return normalized;
}

export function onLocaleChange(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function t(key, params = {}) {
  let template = resolvePath(DICTIONARIES[currentLocale], key);
  if (template === undefined && currentLocale !== 'en-US') {
    template = resolvePath(DICTIONARIES['en-US'], key);
  }
  if (template === undefined && currentLocale !== DEFAULT_LOCALE) {
    template = resolvePath(DICTIONARIES[DEFAULT_LOCALE], key);
  }
  if (template === undefined) {
    missingKeys.add(key);
    return key;
  }
  return interpolate(template, params);
}

export function getMissingKeys() {
  return Array.from(missingKeys);
}

export function clearMissingKeys() {
  missingKeys.clear();
}

export function normalizeSupportStatus(rawStatus) {
  if (!rawStatus || typeof rawStatus !== 'string') return 'UNKNOWN';
  const trimmed = rawStatus.trim();
  if (!trimmed) return 'UNKNOWN';
  if (trimmed === trimmed.toUpperCase() && trimmed.includes('_')) {
    return trimmed;
  }
  return trimmed
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .replace(/[\s-]+/g, '_')
    .toUpperCase();
}

export function hasTranslation(key) {
  return resolvePath(DICTIONARIES[currentLocale], key) !== undefined
    || resolvePath(DICTIONARIES['en-US'], key) !== undefined;
}

export function translateSupportStatus(rawStatus) {
  const code = normalizeSupportStatus(rawStatus);
  const key = `supportStatus.${code}`;
  return {
    code,
    label: hasTranslation(key) ? t(key) : code
  };
}

export function resolveErrorInfo(errorCode, rawMessage = '') {
  const normalized = typeof errorCode === 'string' && errorCode.trim()
    ? errorCode.trim().toUpperCase()
    : 'UNKNOWN';
  const key = `errors.${normalized}`;
  const hasKey = resolvePath(DICTIONARIES[currentLocale], key) !== undefined
    || resolvePath(DICTIONARIES['en-US'], key) !== undefined;

  const userMessage = hasKey ? t(key) : t('errors.UNKNOWN');
  return {
    userMessage,
    rawCode: normalized,
    rawDetail: rawMessage ? String(rawMessage) : ''
  };
}

export function translateErrorCode(errorCode, rawMessage = '') {
  return resolveErrorInfo(errorCode, rawMessage).userMessage;
}

export function formatNumber(value, locale = currentLocale) {
  if (value == null || Number.isNaN(Number(value))) return '—';
  return new Intl.NumberFormat(locale).format(Number(value));
}

export function formatBytes(bytes, locale = currentLocale) {
  if (bytes == null || Number.isNaN(Number(bytes))) return '—';
  const n = Number(bytes);
  if (n < 1024) {
    return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(n)} B`;
  }
  if (n < 1024 * 1024) {
    return `${new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(n / 1024)} KB`;
  }
  if (n < 1024 * 1024 * 1024) {
    return `${new Intl.NumberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(n / (1024 * 1024))} MB`;
  }
  return `${new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(n / (1024 * 1024 * 1024))} GB`;
}

export function formatDateTime(isoString, locale = currentLocale) {
  if (!isoString) return '—';
  const date = new Date(isoString);
  if (Number.isNaN(date.getTime())) return String(isoString);
  return new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short'
  }).format(date);
}

export function applyDomTranslations(root = typeof document !== 'undefined' ? document : null) {
  if (!root) return;
  const hasAnyDictionary = hasLoadedDictionary(currentLocale)
    || hasLoadedDictionary('en-US')
    || hasLoadedDictionary(DEFAULT_LOCALE);
  if (!hasAnyDictionary) return;

  if (root === document && document.documentElement) {
    document.documentElement.lang = currentLocale === 'vi-VN' ? 'vi' : 'en';
    document.title = t('app.windowTitle');
  }

  for (const el of root.querySelectorAll('[data-i18n]')) {
    const key = el.getAttribute('data-i18n');
    if (key) el.textContent = t(key);
  }
  for (const el of root.querySelectorAll('[data-i18n-placeholder]')) {
    const key = el.getAttribute('data-i18n-placeholder');
    if (key) el.setAttribute('placeholder', t(key));
  }
  for (const el of root.querySelectorAll('[data-i18n-aria-label]')) {
    const key = el.getAttribute('data-i18n-aria-label');
    if (key) el.setAttribute('aria-label', t(key));
  }
  for (const el of root.querySelectorAll('[data-i18n-title]')) {
    const key = el.getAttribute('data-i18n-title');
    if (key) el.setAttribute('title', t(key));
  }
}
