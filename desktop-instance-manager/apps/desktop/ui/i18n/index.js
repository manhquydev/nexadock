import viVN from './vi-VN.json' with { type: 'json' };
import enUS from './en-US.json' with { type: 'json' };

export const DEFAULT_LOCALE = 'vi-VN';
export const SUPPORTED_LOCALES = Object.freeze(['vi-VN', 'en-US']);
export const LOCALE_STORAGE_KEY = 'multi-zalo.locale';

export const DICTIONARIES = Object.freeze({
  'vi-VN': viVN,
  'en-US': enUS
});

let currentLocale = loadInitialLocale();
const listeners = new Set();
const missingKeys = new Set();

function loadInitialLocale() {
  try {
    if (typeof localStorage !== 'undefined') {
      const saved = localStorage.getItem(LOCALE_STORAGE_KEY);
      if (saved && SUPPORTED_LOCALES.includes(saved)) {
        return saved;
      }
    }
  } catch (_err) {
    // Ignore storage access errors
  }
  return DEFAULT_LOCALE;
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
  // Convert Rust Debug PascalCase (e.g. PartiallySupported -> PARTIALLY_SUPPORTED)
  const snake = trimmed
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .replace(/[\s-]+/g, '_')
    .toUpperCase();
  return snake;
}

export function translateSupportStatus(rawStatus) {
  const code = normalizeSupportStatus(rawStatus);
  const key = `supportStatus.${code}`;
  const hasKey = resolvePath(DICTIONARIES[currentLocale], key) !== undefined
    || resolvePath(DICTIONARIES['en-US'], key) !== undefined;
  return {
    code,
    label: hasKey ? t(key) : code
  };
}

export function translateErrorCode(errorCode, fallbackMessage = '') {
  if (!errorCode || typeof errorCode !== 'string') {
    return fallbackMessage || t('errors.UNKNOWN');
  }
  const normalized = errorCode.trim().toUpperCase();
  const key = `errors.${normalized}`;
  const hasKey = resolvePath(DICTIONARIES[currentLocale], key) !== undefined
    || resolvePath(DICTIONARIES['en-US'], key) !== undefined;
  const localized = hasKey ? t(key) : (fallbackMessage || t('errors.UNKNOWN'));
  return fallbackMessage && fallbackMessage !== localized
    ? `${localized} (${normalized}: ${fallbackMessage})`
    : `${localized} (${normalized})`;
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
}
