import {
  initI18n,
  getLocale,
  setLocale,
  onLocaleChange,
  t,
  translateSupportStatus,
  resolveErrorInfo,
  formatBytes,
  formatDateTime,
  formatNumber,
  applyDomTranslations,
  LOCALE_STORAGE_KEY,
  SUPPORTED_LOCALES
} from './i18n/index.js';

const UI_VERSION = '0.1.0';
const THEME_STORAGE_KEY = 'multi-zalo.theme';
const ACCOUNT_COLORS_STORAGE_KEY = 'multi-zalo.accountColors';
const PINNED_STORAGE_KEY = 'khoang.pinnedAccounts';
const RAIL_EXPANDED_KEY = 'khoang.railExpanded';
const ACTIVE_ACCOUNT_KEY = 'khoang.activeAccountId';

const ACCOUNT_COLORS = [
  { id: 'blue', cssVar: '--id-blue' },
  { id: 'teal', cssVar: '--id-teal' },
  { id: 'green', cssVar: '--id-green' },
  { id: 'amber', cssVar: '--id-amber' },
  { id: 'coral', cssVar: '--id-coral' },
  { id: 'plum', cssVar: '--id-plum' },
  { id: 'indigo', cssVar: '--id-indigo' },
  { id: 'slate', cssVar: '--id-slate' }
];

const state = {
  status: null,
  workspace: null,
  backups: [],
  diagnostics: null,
  isolation: null,
  activeView: 'profiles', // 'profiles' | 'backups' | 'settings' | 'diagnostics' | 'help' | 'desktop-stage' | 'web-stage'
  activeAccount: null, // { id, kind }
  lastActivationResult: null,
  railFilterQuery: '',
  searchQuery: '',
  switcherQuery: '',
  switcherActiveIndex: 0,
  connectionMode: null, // 'tauri' | 'bridge' | null
  connectionOk: false,
  activeRenameId: null,
  activeRenameKind: 'desktop_isolated',
  activeDeleteId: null,
  activeDeleteKind: 'desktop_isolated',
  activeResetWebId: null,
  activeRestoreBackupDir: null,
  activeRestoreSourceId: null,
  createdAccountForStep2: null,
  openDialogContext: null,
  openMenuProfileId: null,
  refreshSeq: 0,
  activationSeq: 0,
  webSyncSeq: 0,
  webSyncChain: Promise.resolve(),
  initialized: false,
  banner: null,
  toasts: [],
  nextToastSeq: 1,
  profileErrors: new Map(),
  diagnosticsReturnView: 'settings',
  lastNavTriggerEl: null,
  contextMenuCleanup: null,
  pendingProfileActions: new Map(),
  pendingGlobalActions: new Set(),
  overlayBlockers: new Set(),
  fallbackMenuCleanups: new Set(),
  mountedWebSessionId: null,
  lastWebBoundsKey: ''
};

export function getTauriInvoke() {
  if (typeof window === 'undefined') return null;
  if (window.__TAURI__?.core?.invoke) return window.__TAURI__.core.invoke;
  if (window.__TAURI_INTERNALS__?.invoke) return window.__TAURI_INTERNALS__.invoke;
  return null;
}

function loadJsonStorage(key, fallback) {
  try {
    if (typeof localStorage !== 'undefined') {
      const raw = localStorage.getItem(key);
      if (raw) return JSON.parse(raw);
    }
  } catch (_err) {
    // Ignore storage errors
  }
  return fallback;
}

function saveJsonStorage(key, value) {
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(key, JSON.stringify(value));
    }
  } catch (_err) {
    // Ignore storage errors
  }
}

function loadAccountColorsMap() {
  const parsed = loadJsonStorage(ACCOUNT_COLORS_STORAGE_KEY, {});
  return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
}

export function saveAccountColor(profileId, colorId) {
  if (!profileId || !ACCOUNT_COLORS.some((c) => c.id === colorId)) return;
  const map = loadAccountColorsMap();
  map[profileId] = colorId;
  saveJsonStorage(ACCOUNT_COLORS_STORAGE_KEY, map);
}

export function removeAccountColor(profileId) {
  if (!profileId) return;
  const map = loadAccountColorsMap();
  if (Object.hasOwn(map, profileId)) {
    delete map[profileId];
    saveJsonStorage(ACCOUNT_COLORS_STORAGE_KEY, map);
  }
}

export function getAccountColorId(prof) {
  const map = loadAccountColorsMap();
  const saved = prof?.id ? map[prof.id] : null;
  const matched = ACCOUNT_COLORS.find((c) => c.id === saved);
  if (matched) return matched.id;
  if (prof?.color_token && ACCOUNT_COLORS.some((c) => c.id === prof.color_token)) {
    return prof.color_token;
  }

  const seed = String(prof?.id || prof?.name || prof?.display_name || 'z');
  let hash = 0;
  for (let i = 0; i < seed.length; i += 1) {
    hash = ((hash << 5) - hash + seed.charCodeAt(i)) | 0;
  }
  return ACCOUNT_COLORS[Math.abs(hash) % ACCOUNT_COLORS.length].id;
}

export function getAccountColorVar(prof) {
  const colorId = getAccountColorId(prof);
  const matched = ACCOUNT_COLORS.find((c) => c.id === colorId) || ACCOUNT_COLORS[0];
  return `var(${matched.cssVar})`;
}

export function pickNextAvailableColorId(profiles = []) {
  const list = Array.isArray(profiles) ? profiles : [];
  const used = new Set(list.map((p) => getAccountColorId(p)));
  const available = ACCOUNT_COLORS.find((c) => !used.has(c.id));
  if (available) return available.id;
  return ACCOUNT_COLORS[list.length % ACCOUNT_COLORS.length].id;
}

export function getAccountInitials(name) {
  const cleaned = String(name || '')
    .trim()
    .replace(/[^\p{L}\p{N}\s]/gu, ' ')
    .trim();
  if (!cleaned) return 'Z';
  const parts = cleaned.split(/\s+/).filter(Boolean);
  if (parts.length === 1) {
    return Array.from(parts[0]).slice(0, 2).join('').toUpperCase();
  }
  const first = Array.from(parts[0])[0] || '';
  const last = Array.from(parts[parts.length - 1])[0] || '';
  return `${first}${last}`.toUpperCase();
}

function updateConnectionBadge(mode = state.connectionMode, ok = state.connectionOk) {
  state.connectionMode = mode;
  state.connectionOk = ok;
  if (typeof document === 'undefined') return;
  const pill = document.getElementById('connection-pill');
  const label = document.getElementById('connection-label');
  if (!pill || !label) return;

  if (!mode || ok) {
    pill.hidden = true;
    return;
  }
  pill.hidden = false;
  label.textContent = t('app.disconnected');
}

export function getSessionToken() {
  if (typeof document === 'undefined') return '';
  return document.querySelector('meta[name="dim-session-token"]')?.content || '';
}

export async function apiCall(route, method = 'GET', body = null, tauriCmd = null, tauriArgs = {}) {
  const invoke = getTauriInvoke();
  if (invoke && tauriCmd) {
    try {
      const result = await invoke(tauriCmd, tauriArgs);
      updateConnectionBadge('tauri', true);
      return result;
    } catch (err) {
      const rawMsg = typeof err === 'string' ? err : err?.message || t('errors.HOST_EXEC_FAILED');
      const codeMatch = rawMsg.match(/^\[([A-Z0-9_]+)\]\s*(.*)$/s);
      const errorCode = codeMatch ? codeMatch[1] : 'HOST_EXEC_FAILED';
      const cleanMsg = codeMatch ? codeMatch[2] : rawMsg;
      const isDataOrAppError = Boolean(codeMatch) && errorCode !== 'HOST_EXEC_FAILED' && errorCode !== 'PROCESS_ERROR';
      updateConnectionBadge('tauri', isDataOrAppError);
      const tauriErr = new Error(cleanMsg || t('errors.HOST_EXEC_FAILED'));
      tauriErr.errorCode = errorCode;
      throw tauriErr;
    }
  }

  const opts = {
    method,
    headers: { Accept: 'application/json' }
  };
  const token = getSessionToken();
  if (token) {
    opts.headers['X-DIM-Session-Token'] = token;
  }
  if (body !== null) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }

  const timeoutMs = Number(globalThis.__MZ_API_TIMEOUT_MS) > 0
    ? Number(globalThis.__MZ_API_TIMEOUT_MS)
    : 32000;
  const controller = typeof AbortController !== 'undefined' ? new AbortController() : null;
  if (controller) {
    opts.signal = controller.signal;
  }
  const timer = controller
    ? setTimeout(() => {
      controller.abort();
    }, timeoutMs)
    : null;

  try {
    const res = await fetch(route, opts);
    const rawData = await res.json().catch(() => ({ ok: false, message: `HTTP ${res.status}` }));
    const data = rawData && typeof rawData === 'object' ? rawData : { ok: res.ok, data: rawData };
    const isInfraError = res.status === 401
      || res.status >= 500
      || data?.error_code === 'HOST_EXEC_FAILED'
      || data?.error_code === 'HOST_TIMEOUT'
      || data?.error_code === 'UNAUTHORIZED';
    updateConnectionBadge('bridge', !isInfraError);
    if (!res.ok && data) {
      if (data.ok === undefined) data.ok = false;
      if (!data.error_code) {
        if (res.status === 401) data.error_code = 'UNAUTHORIZED';
        else if (res.status === 504) data.error_code = 'HOST_TIMEOUT';
        else if (res.status >= 500) data.error_code = 'HOST_EXEC_FAILED';
      }
    }
    return data;
  } catch (err) {
    updateConnectionBadge('bridge', false);
    if (err?.name === 'AbortError' || controller?.signal?.aborted) {
      const timeoutErr = new Error(err?.message || `Request timed out after ${timeoutMs}ms`);
      timeoutErr.errorCode = 'HOST_TIMEOUT';
      throw timeoutErr;
    }
    if (err && typeof err === 'object') {
      try {
        err.errorCode = 'HOST_EXEC_FAILED';
      } catch (_ignore) {
        // Ignore non-extensible error objects
      }
      throw err;
    }
    const wrappedErr = new Error(String(err || 'Fetch failed'));
    wrappedErr.errorCode = 'HOST_EXEC_FAILED';
    throw wrappedErr;
  } finally {
    if (timer) clearTimeout(timer);
  }
}

async function tauriInvokeEnvelope(cmd, args = {}) {
  const invoke = getTauriInvoke();
  if (!invoke) return null;
  const env = await invoke(cmd, args);
  updateConnectionBadge('tauri', true);
  return env;
}

function announceToScreenReader(text) {
  if (typeof document === 'undefined') return;
  const el = document.getElementById('sr-announcer');
  if (!el) return;
  el.textContent = '';
  setTimeout(() => {
    el.textContent = text;
  }, 40);
}

function renderBanner() {
  const banner = document.getElementById('status-banner');
  const textEl = document.getElementById('status-banner-text');
  const detailsEl = document.getElementById('status-banner-details');
  const rawEl = document.getElementById('status-banner-raw');
  if (!banner || !textEl || !detailsEl || !rawEl) return;

  if (!state.banner) {
    banner.hidden = true;
    banner.className = 'banner';
    banner.setAttribute('role', 'status');
    banner.setAttribute('aria-live', 'polite');
    textEl.textContent = '';
    detailsEl.hidden = true;
    detailsEl.open = false;
    rawEl.textContent = '';
    return;
  }

  const behavior = state.banner.errorCode ? classifyErrorBehavior(state.banner.errorCode) : null;
  const isAssertive = behavior
    ? behavior.liveRole === 'alert'
    : state.banner.kind === 'error';
  const bannerTone = behavior && behavior.tone === 'info'
    ? 'info'
    : (behavior && behavior.tone === 'warn' ? 'warn' : state.banner.kind);
  banner.className = `banner banner-${bannerTone}`;
  banner.setAttribute('role', isAssertive ? 'alert' : 'status');
  banner.setAttribute('aria-live', isAssertive ? 'assertive' : 'polite');
  if (state.banner.errorCode) {
    const info = resolveErrorInfo(state.banner.errorCode, state.banner.rawDetail);
    textEl.textContent = info.userMessage;
    const rawLines = [`${t('errors.rawCodeLabel')}: ${info.rawCode}`];
    if (info.rawDetail) {
      rawLines.push(`${t('errors.rawMessageLabel')}: ${info.rawDetail}`);
    }
    rawEl.textContent = rawLines.join('\n');
    detailsEl.hidden = false;
  } else {
    textEl.textContent = state.banner.messageKey
      ? t(state.banner.messageKey, state.banner.messageParams || {})
      : (state.banner.rawText || '');
    detailsEl.hidden = true;
    detailsEl.open = false;
    rawEl.textContent = '';
  }
  banner.hidden = false;
}

function dismissToast(id) {
  const idx = state.toasts.findIndex((item) => item.id === id);
  if (idx < 0) return;
  const [removed] = state.toasts.splice(idx, 1);
  if (removed?.timer) clearTimeout(removed.timer);
  renderToasts();
}

function scheduleToastTimer(toastObj) {
  if (toastObj.timer) clearTimeout(toastObj.timer);
  toastObj.timer = null;
  if (toastObj.hovered || toastObj.focused) return;
  toastObj.timer = setTimeout(() => {
    dismissToast(toastObj.id);
  }, 4500);
}

function renderToasts() {
  if (typeof document === 'undefined') return;
  const region = document.getElementById('toast-region');
  if (!region) return;
  const activeToastEl = document.activeElement?.closest?.('.toast[data-toast-id]');
  const focusedToastId = activeToastEl ? Number(activeToastEl.dataset.toastId) : null;
  region.replaceChildren();

  for (const toast of state.toasts) {
    const item = document.createElement('div');
    item.className = `toast toast-${toast.kind}`;
    item.dataset.toastId = String(toast.id);
    if (toast.dedupeKey) item.dataset.toastKey = toast.dedupeKey;

    const icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    icon.setAttribute('width', '16');
    icon.setAttribute('height', '16');
    icon.setAttribute('viewBox', '0 0 24 24');
    icon.setAttribute('fill', 'none');
    icon.setAttribute('stroke', 'currentColor');
    icon.setAttribute('stroke-width', '2.2');
    icon.setAttribute('aria-hidden', 'true');
    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    path.setAttribute('d', toast.kind === 'error' ? 'M12 9v4m0 4h.01M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z' : 'M20 6L9 17l-5-5');
    icon.appendChild(path);

    const text = document.createElement('span');
    text.className = 'toast-text';
    text.textContent = toast.text;

    const closeBtn = document.createElement('button');
    closeBtn.type = 'button';
    closeBtn.className = 'toast-close';
    closeBtn.setAttribute('aria-label', t('messages.dismissToast'));
    closeBtn.textContent = '×';
    closeBtn.addEventListener('click', () => dismissToast(toast.id));

    item.addEventListener('mouseenter', () => {
      toast.hovered = true;
      if (toast.timer) {
        clearTimeout(toast.timer);
        toast.timer = null;
      }
    });
    item.addEventListener('mouseleave', () => {
      toast.hovered = false;
      scheduleToastTimer(toast);
    });
    item.addEventListener('focusin', () => {
      toast.focused = true;
      if (toast.timer) {
        clearTimeout(toast.timer);
        toast.timer = null;
      }
    });
    item.addEventListener('focusout', (ev) => {
      if (!item.contains(ev.relatedTarget)) {
        toast.focused = false;
        scheduleToastTimer(toast);
      }
    });

    item.appendChild(icon);
    item.appendChild(text);
    item.appendChild(closeBtn);
    region.appendChild(item);
    if (focusedToastId === toast.id) {
      closeBtn.focus();
    }
  }
}

function showToast(messageKey, messageParams = {}, kind = 'success', dedupeKey = null) {
  const text = t(messageKey, messageParams);
  const key = dedupeKey || messageKey;
  const existing = state.toasts.find((item) => item.dedupeKey === key);
  if (existing) {
    existing.text = text;
    existing.kind = kind;
    scheduleToastTimer(existing);
    renderToasts();
    return;
  }
  const toastObj = {
    id: state.nextToastSeq++,
    dedupeKey: key,
    kind,
    text,
    timer: null
  };
  state.toasts.push(toastObj);
  while (state.toasts.length > 3) {
    const oldest = state.toasts.shift();
    if (oldest?.timer) clearTimeout(oldest.timer);
  }
  scheduleToastTimer(toastObj);
  renderToasts();
}

export function classifyErrorBehavior(errorCode) {
  const code = String(errorCode || 'UNKNOWN').trim().toUpperCase();
  if (
    code === 'HOST_EXEC_FAILED' ||
    code === 'HOST_TIMEOUT' ||
    code === 'RUNTIME_UNAVAILABLE' ||
    code === 'BRIDGE_DISCONNECTED' ||
    code === 'INTERNAL_ERROR'
  ) {
    return {
      category: 'critical_system',
      retryable: true,
      liveRole: 'alert',
      tone: 'error',
      action: 'open_diagnostics'
    };
  }
  if (
    code === 'PROTECTED_PRIMARY_PROCESS' ||
    code === 'PROTECTED_PRIMARY_PATH' ||
    code === 'PRIMARY_PROTECTED' ||
    code === 'PRIMARY_NOT_RUNNING' ||
    code === 'PRIMARY_IN_TRAY' ||
    code === 'PRIMARY_HIDDEN_IN_TRAY' ||
    code === 'PRIMARY_FOREGROUND_BLOCKED' ||
    code.startsWith('PATH_TRAVERSAL') ||
    code === 'COMPANION_SNAP_UNSUPPORTED' ||
    code === 'WEB_SESSION_UNSUPPORTED' ||
    code === 'WEB_WORKSPACE_REQUIRES_MACOS_14' ||
    code === 'WEB_WORKSPACE_WAYLAND_UNSUPPORTED' ||
    code === 'NOT_RUNNING'
  ) {
    return {
      category: 'info_only',
      retryable: false,
      liveRole: 'status',
      tone: 'info',
      action: 'dismiss'
    };
  }
  if (
    code === 'INSTALL_NOT_FOUND' ||
    code === 'FOLDER_REDIRECTION_DETECTED' ||
    code === 'INSUFFICIENT_DISK_SPACE' ||
    code === 'PROFILE_IDENTITY_MISMATCH' ||
    code === 'PROFILE_MARKER_MISMATCH' ||
    code === 'DUPLICATE_ZALO_USER_ID' ||
    code === 'PROFILE_RUNNING' ||
    code === 'UNVERIFIED_BINARY' ||
    code === 'EXTERNAL_DB_OUTSIDE_PROFILE' ||
    code === 'SHARED_EXTERNAL_DB_PATH'
  ) {
    return {
      category: 'action_required',
      retryable: false,
      liveRole: 'status',
      tone: 'warn',
      action: code === 'PROFILE_RUNNING' ? 'dismiss' : 'open_diagnostics'
    };
  }
  if (
    code === 'NOT_FOUND' ||
    code === 'PROFILE_NOT_FOUND' ||
    code === 'BACKUP_NOT_FOUND' ||
    code === 'INVALID_NAME' ||
    code === 'INVALID_PROFILE_NAME' ||
    code === 'DUPLICATE_NAME' ||
    code === 'INVALID_INPUT' ||
    code === 'CONFIRM_REQUIRED' ||
    code === 'CONFIRMATION_REQUIRED' ||
    code === 'SCHEMA_ERROR' ||
    code === 'JSON_ERROR' ||
    code === 'UNSUPPORTED_PLATFORM'
  ) {
    return {
      category: 'non_retryable',
      retryable: false,
      liveRole: 'status',
      tone: 'error',
      action: 'dismiss'
    };
  }
  return {
    category: 'retryable',
    retryable: true,
    liveRole: 'alert',
    tone: 'error',
    action: 'retry'
  };
}

function showProfileError(profileId, errorCode, rawDetail = '', retryFn = null) {
  if (!profileId) {
    showErrorBanner(errorCode, rawDetail);
    return;
  }
  state.profileErrors.set(profileId, {
    errorCode: errorCode || 'UNKNOWN',
    rawDetail: rawDetail ? String(rawDetail) : '',
    retryFn: typeof retryFn === 'function' ? retryFn : null
  });
}

function clearProfileError(profileId) {
  if (!profileId) return;
  state.profileErrors.delete(profileId);
}

function showKeyBanner(messageKey, messageParams = {}, kind = 'info') {
  if (kind === 'success') {
    showToast(messageKey, messageParams, 'success', messageKey);
    return;
  }
  state.banner = { kind, messageKey, messageParams };
  renderBanner();
}

function showErrorBanner(errorCode, rawDetail = '') {
  const code = errorCode || 'UNKNOWN';
  const detail = rawDetail ? String(rawDetail) : '';
  if (state.banner?.kind === 'error' && state.banner.errorCode === code && state.banner.rawDetail === detail) {
    return;
  }
  state.banner = {
    kind: 'error',
    errorCode: code,
    rawDetail: detail
  };
  renderBanner();
}

function clearBanner() {
  state.banner = null;
  renderBanner();
}

function getSystemThemePreference() {
  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  return 'light';
}

export function getSavedThemePreference() {
  try {
    if (typeof localStorage !== 'undefined') {
      const saved = localStorage.getItem(THEME_STORAGE_KEY);
      if (saved === 'light' || saved === 'dark' || saved === 'system') {
        return saved;
      }
    }
  } catch (_err) {
    // Ignore storage errors
  }
  return 'system';
}

export function applyThemePreference(pref) {
  const normalized = pref === 'light' || pref === 'dark' || pref === 'system' ? pref : 'system';
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(THEME_STORAGE_KEY, normalized);
    }
  } catch (_err) {
    // Ignore storage errors
  }

  const root = document.documentElement;
  const meta = document.querySelector('meta[name="color-scheme"]');
  if (normalized === 'system') {
    root.removeAttribute('data-theme');
    if (meta) meta.setAttribute('content', 'light dark');
  } else {
    root.setAttribute('data-theme', normalized);
    if (meta) meta.setAttribute('content', normalized);
  }
  updateAppearanceControls();
}

function updateAppearanceControls() {
  const pref = getSavedThemePreference();
  for (const input of document.querySelectorAll('input[name="ui-theme"]')) {
    input.checked = input.value === pref;
  }
  const sysLabel = document.getElementById('theme-system-label');
  if (sysLabel) {
    const sysCurrent = getSystemThemePreference() === 'dark'
      ? t('settings.appearance.dark')
      : t('settings.appearance.light');
    sysLabel.textContent = t('settings.appearance.systemWithCurrent', { current: sysCurrent });
  }
}

function updateLanguageControls() {
  const current = getLocale();
  for (const input of document.querySelectorAll('input[name="ui-locale"]')) {
    input.checked = input.value === current;
  }
}

function updatePendingControls() {
  const isRefreshing = state.pendingGlobalActions.has('refresh');
  const isVerifying = state.pendingGlobalActions.has('verify');
  const isCreating = state.pendingGlobalActions.has('create');
  const isStoppingAll = state.pendingGlobalActions.has('stop-all');

  const btnRefresh = document.getElementById('btn-refresh');
  if (btnRefresh) btnRefresh.disabled = isRefreshing;

  const btnRefreshBackups = document.getElementById('btn-refresh-backups');
  if (btnRefreshBackups) btnRefreshBackups.disabled = isRefreshing;

  const btnRefreshDiag = document.getElementById('btn-refresh-diagnostics');
  if (btnRefreshDiag) btnRefreshDiag.disabled = isRefreshing;

  const btnVerify = document.getElementById('btn-run-isolation');
  if (btnVerify) btnVerify.disabled = isVerifying || isRefreshing;

  const btnCreate = document.getElementById('btn-open-create');
  if (btnCreate) btnCreate.disabled = isCreating;

  const btnOverviewCreate = document.getElementById('btn-overview-create');
  if (btnOverviewCreate) btnOverviewCreate.disabled = isCreating;

  const btnEmptyCreate = document.getElementById('btn-empty-create');
  if (btnEmptyCreate) btnEmptyCreate.disabled = isCreating;

  const btnStopAll = document.getElementById('btn-stop-all');
  if (btnStopAll) btnStopAll.disabled = isStoppingAll;
}

function updateDocumentTitle(viewName = state.activeView) {
  if (typeof document === 'undefined') return;
  const viewTitles = {
    profiles: t('nav.accounts'),
    backups: t('nav.backups'),
    settings: t('nav.settings'),
    diagnostics: t('diagnostics.title'),
    help: t('help.title'),
    'desktop-stage': state.activeAccount ? findAccountDisplayName(state.activeAccount) : t('nav.accounts'),
    'web-stage': state.activeAccount ? findAccountDisplayName(state.activeAccount) : t('nav.accounts')
  };
  const viewTitle = viewTitles[viewName] || t('nav.accounts');
  document.title = `${t('app.name')} — ${viewTitle}`;
}

/* ========================================================================
 * WebView Overlay Coordinator (Z-Order Protection for Native Child WebViews)
 * ======================================================================== */
function isWebViewStageActive() {
  return state.activeView === 'web-stage' && state.activeAccount?.kind === 'web_isolated';
}

function computePhysicalViewportBounds() {
  const viewport = document.getElementById('web-session-viewport');
  if (!viewport) return null;
  if (typeof document !== 'undefined' && document.hidden) return null;
  const rect = viewport.getBoundingClientRect();
  const dpr = typeof window !== 'undefined' && window.devicePixelRatio ? window.devicePixelRatio : 1;
  const x = Math.ceil(rect.left * dpr);
  const y = Math.ceil(rect.top * dpr);
  const right = Math.floor(rect.right * dpr);
  const bottom = Math.floor(rect.bottom * dpr);
  const width = right - x;
  const height = bottom - y;
  if (width < 40 || height < 40) return null;
  return { x, y, width, height };
}

function syncNativeWebViewState({ forceHide = false } = {}) {
  const seq = ++state.webSyncSeq;
  const run = async () => {
    const shell = document.getElementById('workspace-shell');
    const shouldShow = isWebViewStageActive()
      && state.overlayBlockers.size === 0
      && Boolean(state.workspace?.web_workspace?.supported);

    const bounds = shouldShow ? computePhysicalViewportBounds() : null;
    const invoke = getTauriInvoke();

    if (!shouldShow || !bounds) {
      if (shell) shell.dataset.webviewActive = 'false';
      const hadMounted = Boolean(state.mountedWebSessionId);
      state.mountedWebSessionId = null;
      state.lastWebBoundsKey = '';
      if ((hadMounted || forceHide) && invoke) {
        await tauriInvokeEnvelope('web_session_hide_all', {}).catch(() => {});
      }
      syncRailTooltipsForWebView(false);
      updateWebViewportPlaceholder();
      return;
    }

    if (seq !== state.webSyncSeq) return;

    if (shell) shell.dataset.webviewActive = 'true';
    syncRailTooltipsForWebView(true);

    if (!invoke) {
      updateWebViewportPlaceholder();
      return;
    }

    const targetId = state.activeAccount.id;
    const boundsKey = `${targetId}:${bounds.x},${bounds.y},${bounds.width},${bounds.height}`;
    if (state.mountedWebSessionId !== targetId) {
      state.mountedWebSessionId = targetId;
      state.lastWebBoundsKey = boundsKey;
      updateWebViewportPlaceholder();
      try {
        const env = await tauriInvokeEnvelope('web_session_show', {
          id: targetId,
          x: bounds.x,
          y: bounds.y,
          width: bounds.width,
          height: bounds.height
        });
        if (!env || env.ok === false) {
          state.mountedWebSessionId = null;
          state.lastWebBoundsKey = '';
          if (shell) shell.dataset.webviewActive = 'false';
          syncRailTooltipsForWebView(false);
          updateWebViewportPlaceholder();
          if (seq === state.webSyncSeq) {
            showErrorBanner(env?.error_code || 'WEB_SESSION_ERROR', env?.message || '');
          }
          return;
        }
        if (seq !== state.webSyncSeq && (!isWebViewStageActive() || state.overlayBlockers.size > 0)) {
          state.mountedWebSessionId = null;
          state.lastWebBoundsKey = '';
          await tauriInvokeEnvelope('web_session_hide_all', {}).catch(() => {});
        }
      } catch (err) {
        state.mountedWebSessionId = null;
        state.lastWebBoundsKey = '';
        if (shell) shell.dataset.webviewActive = 'false';
        syncRailTooltipsForWebView(false);
        updateWebViewportPlaceholder();
        if (seq === state.webSyncSeq) {
          showErrorBanner(err?.errorCode || 'WEB_SESSION_ERROR', err?.message || '');
        }
      }
    } else if (state.lastWebBoundsKey !== boundsKey) {
      const prevBoundsKey = state.lastWebBoundsKey;
      state.lastWebBoundsKey = boundsKey;
      try {
        const env = await tauriInvokeEnvelope('web_session_set_bounds', {
          id: targetId,
          x: bounds.x,
          y: bounds.y,
          width: bounds.width,
          height: bounds.height
        });
        if (!env || env.ok === false) {
          state.lastWebBoundsKey = prevBoundsKey;
          if (seq === state.webSyncSeq) {
            showErrorBanner(env?.error_code || 'WEB_SESSION_ERROR', env?.message || '');
          }
        }
      } catch (err) {
        state.lastWebBoundsKey = prevBoundsKey;
        if (seq === state.webSyncSeq) {
          showErrorBanner(err?.errorCode || 'WEB_SESSION_ERROR', err?.message || '');
        }
      }
    }
  };

  state.webSyncChain = state.webSyncChain.then(run, run);
  return state.webSyncChain;
}

function addOverlayBlocker(key) {
  if (!key) return;
  const hadNone = state.overlayBlockers.size === 0;
  state.overlayBlockers.add(key);
  if (hadNone) {
    syncNativeWebViewState({ forceHide: true });
  }
}

function removeOverlayBlocker(key) {
  if (!key) return;
  if (state.overlayBlockers.delete(key) && state.overlayBlockers.size === 0) {
    syncNativeWebViewState();
  }
}

function closeAccountContextMenu({ restoreFocus = false } = {}) {
  if (typeof document === 'undefined') return;
  const menu = document.getElementById('account-context-menu');
  if (typeof state.contextMenuCleanup === 'function') {
    state.contextMenuCleanup();
    state.contextMenuCleanup = null;
  }
  const targetAnchor = menu?._anchorEl || null;
  if (menu) {
    menu.hidden = true;
    menu.replaceChildren();
    menu._anchorEl = null;
  }
  removeOverlayBlocker('context-menu');
  if (restoreFocus && targetAnchor && targetAnchor.isConnected) {
    targetAnchor.focus();
  }
}

function openAccountContextMenu(acc, anchorEl, { clientX = null, clientY = null } = {}) {
  if (typeof document === 'undefined' || !acc) return;
  const menu = document.getElementById('account-context-menu');
  if (!menu) return;

  closeAccountContextMenu({ restoreFocus: false });
  menu._anchorEl = anchorEl || null;
  menu.replaceChildren();
  menu.setAttribute('aria-label', t('rail.contextMenuAria', { name: acc.display_name || acc.name || '' }));

  const isPrimary = acc.kind === 'desktop_primary';
  const isWeb = acc.kind === 'web_isolated';
  const isRunning = acc.state === 'running' || acc.running === true;
  const isPending = state.pendingProfileActions.has(acc.id);
  const blockOfflineOps = isProfileActiveOrUnclean(acc);

  const addItem = (label, onSelect, { disabled = false, danger = false } = {}) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = danger ? 'menu-item menu-item-danger' : 'menu-item';
    btn.setAttribute('role', 'menuitem');
    btn.tabIndex = -1;
    btn.disabled = disabled || isPending;
    btn.textContent = label;
    btn.addEventListener('click', () => {
      closeAccountContextMenu({ restoreFocus: false });
      onSelect();
    });
    menu.appendChild(btn);
  };

  const addSep = () => {
    const hr = document.createElement('hr');
    hr.className = 'menu-sep';
    menu.appendChild(hr);
  };

  if (isPrimary) {
    addItem(t('desktopStage.bringToFront'), () => activateAccount('primary', 'desktop_primary'), { disabled: !isRunning });
    addItem(acc.pinned ? t('rail.unpin') : t('rail.pin'), () => toggleAccountPinned('primary', Boolean(acc.pinned)));
  } else if (isWeb) {
    addItem(t('webSession.openWeb'), () => activateAccount(acc.id, 'web_isolated'));
    addItem(acc.pinned ? t('rail.unpin') : t('rail.pin'), () => toggleAccountPinned(acc.id, Boolean(acc.pinned)));
    addItem(t('accounts.actions.rename'), () => openRenameDialog(acc, 'web_isolated'));
    addItem(t('webSession.reset'), () => openResetWebDialog(acc));
    addSep();
    addItem(t('accounts.actions.delete'), () => openDeleteDialog(acc, 'web_isolated'), { danger: true });
  } else {
    const prof = (state.status?.profiles || []).find((p) => p.id === acc.id) || acc;
    addItem(isRunning ? t('rail.bringToFront') : t('rail.openAccount'), () => activateAccount(acc.id, 'desktop_isolated'));
    if (isRunning) {
      addItem(t('accounts.actions.stop'), () => runProfileAction('stop', prof));
    }
    addSep();
    addItem(acc.pinned ? t('rail.unpin') : t('rail.pin'), () => toggleAccountPinned(acc.id, Boolean(acc.pinned)));
    addItem(t('accounts.actions.rename'), () => openRenameDialog(prof, 'desktop_isolated'));
    addItem(t('accounts.actions.backup'), () => runProfileAction('backup', prof), { disabled: blockOfflineOps });
    addItem(t('accounts.actions.details'), () => openDetailsDialog(prof));
    addSep();
    addItem(t('accounts.actions.delete'), () => openDeleteDialog(prof, 'desktop_isolated'), { disabled: blockOfflineOps, danger: true });
  }

  addOverlayBlocker('context-menu');
  menu.hidden = false;

  const menuWidth = menu.offsetWidth || 210;
  const menuHeight = menu.offsetHeight || 220;
  const anchorRect = anchorEl?.getBoundingClientRect?.();
  const baseX = clientX != null ? clientX : (anchorRect ? anchorRect.right + 6 : 100);
  const baseY = clientY != null ? clientY : (anchorRect ? anchorRect.top : 100);
  const left = Math.max(8, Math.min(baseX, window.innerWidth - menuWidth - 8));
  const top = Math.max(8, Math.min(baseY, window.innerHeight - menuHeight - 8));
  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;

  const items = Array.from(menu.querySelectorAll('[role="menuitem"]:not(:disabled)'));
  if (items.length > 0) items[0].focus();

  const onKeyDown = (event) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeAccountContextMenu({ restoreFocus: true });
      return;
    }
    if (event.key === 'Tab') {
      closeAccountContextMenu({ restoreFocus: false });
      return;
    }
    const enabled = Array.from(menu.querySelectorAll('[role="menuitem"]:not(:disabled)'));
    if (enabled.length === 0) return;
    const idx = enabled.indexOf(document.activeElement);
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      enabled[idx === -1 ? 0 : (idx + 1) % enabled.length].focus();
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      enabled[idx <= 0 ? enabled.length - 1 : idx - 1].focus();
    } else if (event.key === 'Home') {
      event.preventDefault();
      enabled[0].focus();
    } else if (event.key === 'End') {
      event.preventDefault();
      enabled[enabled.length - 1].focus();
    }
  };

  const onDocPointer = (event) => {
    if (!menu.contains(event.target)) {
      closeAccountContextMenu({ restoreFocus: false });
    }
  };

  menu.addEventListener('keydown', onKeyDown);
  const ptrTimer = setTimeout(() => {
    if (!menu.hidden) document.addEventListener('pointerdown', onDocPointer);
  }, 0);
  state.contextMenuCleanup = () => {
    clearTimeout(ptrTimer);
    menu.removeEventListener('keydown', onKeyDown);
    document.removeEventListener('pointerdown', onDocPointer);
  };
}

function showRailFloatingTooltip(btn, text) {
  const portal = document.getElementById('tooltip-portal');
  if (!portal || !btn || !text) return;
  const shell = document.getElementById('workspace-shell');
  if (shell?.dataset.railExpanded === 'true') return;
  portal.replaceChildren();
  const tip = document.createElement('div');
  tip.className = 'rail-tooltip is-floating-open';
  tip.setAttribute('role', 'tooltip');
  tip.textContent = text;
  const rect = btn.getBoundingClientRect();
  tip.style.top = `${Math.round(rect.top + rect.height / 2)}px`;
  tip.style.left = `${Math.round(rect.right + 10)}px`;
  portal.appendChild(tip);
}

function hideRailFloatingTooltip() {
  const portal = document.getElementById('tooltip-portal');
  if (portal) portal.replaceChildren();
}

function syncRailTooltipsForWebView(webviewActive) {
  const portal = document.getElementById('tooltip-portal');
  const buttons = document.querySelectorAll('.rail-account-btn');
  if (webviewActive) {
    if (portal) portal.replaceChildren();
    for (const btn of buttons) {
      const tipText = btn.dataset.tooltipText || '';
      if (tipText) btn.setAttribute('title', tipText);
      btn.removeAttribute('interestfor');
      btn.removeAttribute('aria-describedby');
    }
    return;
  }

  for (const btn of buttons) {
    btn.removeAttribute('title');
    if (btn.dataset.tipWired === 'true') continue;
    btn.dataset.tipWired = 'true';
    btn.addEventListener('mouseenter', () => {
      if (!isWebViewStageActive() || state.overlayBlockers.size > 0) {
        showRailFloatingTooltip(btn, btn.dataset.tooltipText || '');
      }
    });
    btn.addEventListener('focusin', () => {
      if (!isWebViewStageActive() || state.overlayBlockers.size > 0) {
        showRailFloatingTooltip(btn, btn.dataset.tooltipText || '');
      }
    });
    btn.addEventListener('mouseleave', hideRailFloatingTooltip);
    btn.addEventListener('focusout', hideRailFloatingTooltip);
  }
}

function closeWebHeaderPopovers() {
  for (const popId of ['web-session-menu', 'web-limits-tip']) {
    const el = document.getElementById(popId);
    if (el && typeof el.hidePopover === 'function') {
      try {
        if (el.matches(':popover-open')) el.hidePopover();
      } catch (_err) {
        // Ignore if not open
      }
    }
    state.overlayBlockers.delete(`popover:${popId}`);
  }
}

function updateDiagnosticsBackBar() {
  const backBtn = document.getElementById('btn-diag-back');
  const backLabel = document.getElementById('diag-back-label');
  if (!backBtn || !backLabel) return;
  const target = state.diagnosticsReturnView || 'settings';
  backBtn.setAttribute('data-back-to', target);
  if (target === 'help') {
    backLabel.setAttribute('data-i18n', 'nav.backToHelp');
    backLabel.textContent = t('nav.backToHelp');
  } else if (target === 'profiles' || target === 'desktop-stage') {
    backLabel.setAttribute('data-i18n', 'nav.backToAccounts');
    backLabel.textContent = t('nav.backToAccounts');
  } else {
    backLabel.setAttribute('data-i18n', 'diagnostics.backToSettings');
    backLabel.textContent = t('diagnostics.backToSettings');
  }
}

function selectView(viewName, { triggerEl = null, returnView = null } = {}) {
  const wasWebStage = state.activeView === 'web-stage';
  if (viewName === 'diagnostics') {
    state.diagnosticsReturnView = returnView || 'settings';
  }
  state.lastNavTriggerEl = triggerEl || null;
  state.activeView = viewName;
  if (wasWebStage && viewName !== 'web-stage') {
    closeWebHeaderPopovers();
  }
  closeAccountContextMenu({ restoreFocus: false });

  for (const btn of document.querySelectorAll('.top-nav-btn[data-view]')) {
    const active = btn.getAttribute('data-view') === viewName;
    btn.classList.toggle('is-active', active);
    if (active) btn.setAttribute('aria-current', 'page');
    else btn.removeAttribute('aria-current');
  }

  for (const view of document.querySelectorAll('.view')) {
    view.hidden = view.id !== `view-${viewName}`;
  }

  const webStage = document.getElementById('stage-web-session');
  if (webStage) webStage.hidden = viewName !== 'web-stage';

  const desktopStage = document.getElementById('stage-desktop-session');
  if (desktopStage) desktopStage.hidden = viewName !== 'desktop-stage';

  if (viewName === 'diagnostics') {
    updateDiagnosticsBackBar();
  }

  const isStage = viewName === 'web-stage' || viewName === 'desktop-stage';
  if (!isStage) {
    for (const btn of document.querySelectorAll('.rail-account-btn')) {
      btn.setAttribute('aria-pressed', 'false');
    }
  }

  updateDocumentTitle(viewName);
  syncNativeWebViewState({ forceHide: viewName !== 'web-stage' });
}

function makeBadge(text, variant = 'neutral') {
  const span = document.createElement('span');
  span.className = `badge badge-${variant}`;
  span.textContent = text;
  return span;
}

function makeButton(label, variantClass, onClick, { disabled = false, sizeClass = 'btn-sm', ariaLabel = null } = {}) {
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = `btn ${variantClass} ${sizeClass}`.trim();
  btn.textContent = label;
  btn.disabled = disabled;
  if (ariaLabel) {
    btn.setAttribute('aria-label', ariaLabel);
  }
  btn.addEventListener('click', onClick);
  return btn;
}

function appendKvRow(dl, keyLabel, valueText) {
  const dt = document.createElement('dt');
  dt.textContent = keyLabel;
  const dd = document.createElement('dd');
  dd.textContent = valueText;
  dl.appendChild(dt);
  dl.appendChild(dd);
}

function normalizeLifecycleState(prof) {
  if (!prof) return '';
  const raw = prof.lifecycle_state ?? prof.state;
  if (raw == null) return '';
  return String(raw)
    .trim()
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .toLowerCase();
}

function isProfileActiveOrUnclean(prof) {
  const lc = normalizeLifecycleState(prof);
  return Boolean(prof?.running)
    || lc === 'starting'
    || lc === 'running'
    || lc === 'stopping'
    || lc === 'recovery_required';
}

export function resolveAccountState(prof) {
  const pendingAction = prof?.id ? state.pendingProfileActions.get(prof.id) : null;
  if (pendingAction === 'launch' || pendingAction === 'activate') return { label: t('accounts.states.starting'), kind: 'pending' };
  if (pendingAction === 'stop') return { label: t('accounts.states.stopping'), kind: 'pending' };
  if (prof?.error) return { label: t('accounts.states.error'), kind: 'error' };
  const lc = normalizeLifecycleState(prof);
  if (lc === 'starting') return { label: t('accounts.states.starting'), kind: 'pending' };
  if (lc === 'stopping') return { label: t('accounts.states.stopping'), kind: 'pending' };
  if (lc === 'recovery_required') return { label: t('accounts.states.recoveryRequired'), kind: 'error' };
  if (lc === 'running' || prof?.running === true) return { label: t('accounts.states.running'), kind: 'running' };
  if (lc === 'stopped' || prof?.running === false) return { label: t('accounts.states.stopped'), kind: 'stopped' };
  return { label: t('accounts.states.unknown'), kind: 'unknown' };
}

export function resolveIsolationCheckResult(check) {
  const explicit = check?.status ? String(check.status).toUpperCase() : null;
  if (explicit === 'NOT_TESTED') return { label: t('diagnostics.checks.notTested'), raw: 'NOT_TESTED', variant: 'neutral' };
  if (explicit === 'BLOCKED') return { label: t('diagnostics.checks.blocked'), raw: 'BLOCKED', variant: 'warn' };
  if (explicit === 'UNKNOWN') return { label: t('diagnostics.checks.unknown'), raw: 'UNKNOWN', variant: 'neutral' };
  if (explicit === 'FAIL' || check?.passed === false) return { label: t('diagnostics.checks.fail'), raw: 'FAIL', variant: 'fail' };
  if (explicit === 'PASS' || check?.passed === true) return { label: t('diagnostics.checks.pass'), raw: 'PASS', variant: 'pass' };
  return { label: t('diagnostics.checks.unknown'), raw: 'UNKNOWN', variant: 'neutral' };
}

function buildWorkspaceFallbackFromStatus(statusData) {
  const pinnedList = loadJsonStorage(PINNED_STORAGE_KEY, []);
  const pinnedSet = new Set(Array.isArray(pinnedList) ? pinnedList : []);
  const primary = statusData?.primary_zalo;
  const profiles = Array.isArray(statusData?.profiles) ? statusData.profiles : [];
  return {
    primary: {
      detected: Boolean(primary?.running || primary?.is_running),
      running: Boolean(primary?.is_running),
      window_state: primary?.is_running ? 'visible' : 'none',
      window_title: null,
      pinned: pinnedSet.has('primary')
    },
    accounts: profiles.map((p) => ({
      id: p.id,
      kind: 'desktop_isolated',
      display_name: p.name,
      color_token: getAccountColorId(p),
      pinned: pinnedSet.has(p.id),
      state: normalizeLifecycleState(p) || (p.running ? 'running' : 'stopped'),
      window_state: p.running ? 'visible' : 'none',
      companion_snapped: false,
      zalo_user_id: p.zalo_user_id || null,
      window_title: p.window_title || null
    })),
    web_workspace: {
      supported: false,
      reason_code: 'WEB_SESSION_UNSUPPORTED'
    }
  };
}

function getAllColorTrackedAccounts() {
  const desktop = Array.isArray(state.status?.profiles) ? state.status.profiles : [];
  const web = (state.workspace?.accounts || []).filter((a) => a.kind === 'web_isolated');
  return [...desktop, ...web];
}

function getAllUnifiedAccounts() {
  const ws = state.workspace;
  if (!ws) return [];
  const items = [];
  if (ws.primary) {
    items.push({
      id: 'primary',
      kind: 'desktop_primary',
      display_name: t('accounts.primaryZalo.heading'),
      pinned: Boolean(ws.primary.pinned),
      running: Boolean(ws.primary.running),
      state: ws.primary.running ? 'running' : 'stopped',
      window_state: ws.primary.window_state || 'none',
      window_title: ws.primary.window_title || null,
      detected: Boolean(ws.primary.detected)
    });
  }
  const allAccounts = ws.accounts || [];
  const pinned = [];
  const desktop = [];
  const web = [];
  for (const acc of allAccounts) {
    if (acc.pinned) pinned.push(acc);
    else if (acc.kind === 'web_isolated') web.push(acc);
    else desktop.push(acc);
  }
  items.push(...pinned, ...desktop, ...web);
  return items;
}

function findAccountDisplayName(target) {
  if (!target) return t('app.name');
  if (target.kind === 'desktop_primary' || target.id === 'primary') {
    return t('accounts.primaryZalo.heading');
  }
  const found = (state.workspace?.accounts || []).find((a) => a.id === target.id && a.kind === target.kind);
  if (found) return found.display_name;
  const prof = (state.status?.profiles || []).find((p) => p.id === target.id);
  return prof?.name || target.id;
}

function makeKindBadgeSvg(kind) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('width', '10');
  svg.setAttribute('height', '10');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', '2.2');
  svg.setAttribute('aria-hidden', 'true');

  if (kind === 'desktop_primary') {
    const p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    p.setAttribute('d', 'M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z');
    svg.appendChild(p);
  } else if (kind === 'web_isolated') {
    const c = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    c.setAttribute('cx', '12');
    c.setAttribute('cy', '12');
    c.setAttribute('r', '9');
    const l = document.createElementNS('http://www.w3.org/2000/svg', 'line');
    l.setAttribute('x1', '3');
    l.setAttribute('y1', '12');
    l.setAttribute('x2', '21');
    l.setAttribute('y2', '12');
    svg.appendChild(c);
    svg.appendChild(l);
  } else {
    const r = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    r.setAttribute('x', '3');
    r.setAttribute('y', '4');
    r.setAttribute('width', '18');
    r.setAttribute('height', '13');
    r.setAttribute('rx', '2');
    const l = document.createElementNS('http://www.w3.org/2000/svg', 'line');
    l.setAttribute('x1', '8');
    l.setAttribute('y1', '21');
    l.setAttribute('x2', '16');
    l.setAttribute('y2', '21');
    svg.appendChild(r);
    svg.appendChild(l);
  }
  return svg;
}

function renderTopbarSummary() {
  const summaryEl = document.getElementById('topbar-open-summary');
  const installAlert = document.getElementById('zalo-install-alert');
  const installAlertText = document.getElementById('zalo-install-alert-text');
  if (!summaryEl) return;

  if (state.pendingGlobalActions.has('refresh') && !state.status) {
    summaryEl.textContent = t('statusBar.refreshing');
  } else if (!state.status) {
    summaryEl.textContent = t('app.subtitle');
  } else {
    const runningDesktop = (state.status.profiles || []).filter((p) => p.running).length;
    const runningWeb = (state.workspace?.accounts || []).filter((a) => a.kind === 'web_isolated' && a.state === 'running').length;
    const runningCount = runningDesktop + runningWeb;
    summaryEl.textContent = runningCount > 0
      ? t('app.openCount', { count: formatNumber(runningCount) })
      : t('app.noneOpen');
  }

  if (installAlert && installAlertText) {
    const inst = state.status?.installation;
    if (inst && inst.valid === false) {
      installAlertText.textContent = t('statusBar.zaloNotFound');
      installAlert.hidden = false;
    } else {
      installAlert.hidden = true;
      installAlertText.textContent = '';
    }
  }
}

function renderAccountRail() {
  const rail = document.getElementById('rail-accounts');
  if (!rail) return;

  const groupPrimary = rail.querySelector('[data-group="primary"]');
  const groupPinned = rail.querySelector('[data-group="pinned"]');
  const groupDesktop = rail.querySelector('[data-group="desktop"]');
  const groupWeb = rail.querySelector('[data-group="web"]');
  const sepPinned = rail.querySelector('[data-sep-for="pinned"]');
  const sepDesktop = rail.querySelector('[data-sep-for="desktop"]');
  const sepWeb = rail.querySelector('[data-sep-for="web"]');
  const headingWeb = rail.querySelector('[data-heading-for="web"]');
  if (!groupPrimary || !groupPinned || !groupDesktop || !groupWeb) return;

  groupPrimary.replaceChildren();
  groupPinned.replaceChildren();
  groupDesktop.replaceChildren();
  groupWeb.replaceChildren();

  const q = state.railFilterQuery.trim().toLowerCase();
  const all = getAllUnifiedAccounts();
  const filtered = q
    ? all.filter((acc) => String(acc.display_name || '').toLowerCase().includes(q))
    : all;

  const makeRailBtn = (acc) => {
    const isSelected = state.activeAccount?.id === acc.id
      && state.activeAccount?.kind === acc.kind
      && (state.activeView === 'desktop-stage' || state.activeView === 'web-stage');
    const isPending = state.pendingProfileActions.has(acc.id);
    const isPrimary = acc.kind === 'desktop_primary';
    const isWeb = acc.kind === 'web_isolated';

    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = isPrimary
      ? 'rail-account-btn'
      : `rail-account-btn${acc.state === 'running' ? ' is-running' : ''}${acc.pinned ? ' is-pinned' : ''}`;
    btn.dataset.accountId = acc.id;
    btn.dataset.accountKind = acc.kind;
    btn.setAttribute('aria-pressed', String(isSelected));
    if (isPending) {
      btn.setAttribute('aria-disabled', 'true');
    }

    const kindLabel = isPrimary
      ? t('rail.kindPrimary')
      : (isWeb ? t('rail.kindWeb') : t('rail.kindDesktop'));
    const accState = isPrimary
      ? (acc.running ? { label: t('accounts.states.running'), kind: 'running' } : { label: t('accounts.states.stopped'), kind: 'stopped' })
      : resolveAccountState({ id: acc.id, running: acc.state === 'running', lifecycle_state: acc.state });

    const tooltipText = `${acc.display_name} · ${kindLabel} · ${accState.label}`;
    btn.dataset.tooltipText = tooltipText;
    btn.setAttribute('title', tooltipText);
    btn.setAttribute('aria-label', tooltipText);

    const avatarWrap = document.createElement('span');
    avatarWrap.className = 'rail-avatar-wrap';
    avatarWrap.setAttribute('aria-hidden', 'true');

    const avatar = document.createElement('span');
    avatar.className = isPrimary ? 'account-avatar rail-avatar avatar-protected' : 'account-avatar rail-avatar';
    if (!isPrimary) {
      avatar.style.setProperty('--avatar-color', getAccountColorVar({ id: acc.id, name: acc.display_name, color_token: acc.color_token }));
    }
    avatar.textContent = isPrimary ? 'Z' : getAccountInitials(acc.display_name);

    const kindBadge = document.createElement('span');
    kindBadge.className = 'rail-kind-badge';
    kindBadge.appendChild(makeKindBadgeSvg(acc.kind));

    const stateDot = document.createElement('span');
    const dotClass = acc.state === 'recovery_required' ? 'is-recovery' : `is-${accState.kind}`;
    stateDot.className = `rail-state-dot ${dotClass}`;

    avatarWrap.appendChild(avatar);
    avatarWrap.appendChild(kindBadge);
    avatarWrap.appendChild(stateDot);

    const meta = document.createElement('span');
    meta.className = 'rail-item-meta';

    const nameSpan = document.createElement('span');
    nameSpan.className = 'rail-item-name';
    nameSpan.textContent = acc.display_name;

    const subSpan = document.createElement('span');
    subSpan.className = 'rail-item-sub';
    subSpan.textContent = `${kindLabel} · ${accState.label}`;

    meta.appendChild(nameSpan);
    meta.appendChild(subSpan);

    btn.appendChild(avatarWrap);
    btn.appendChild(meta);

    btn.addEventListener('click', () => {
      if (btn.getAttribute('aria-disabled') === 'true') return;
      activateAccount(acc.id, acc.kind);
    });
    btn.addEventListener('contextmenu', (event) => {
      event.preventDefault();
      openAccountContextMenu(acc, btn, { clientX: event.clientX, clientY: event.clientY });
    });
    btn.addEventListener('keydown', (event) => {
      if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
        event.preventDefault();
        openAccountContextMenu(acc, btn);
      }
    });

    return btn;
  };

  for (const acc of filtered) {
    const btn = makeRailBtn(acc);
    if (acc.kind === 'desktop_primary') {
      groupPrimary.appendChild(btn);
    } else if (acc.pinned) {
      groupPinned.appendChild(btn);
    } else if (acc.kind === 'web_isolated') {
      groupWeb.appendChild(btn);
    } else {
      groupDesktop.appendChild(btn);
    }
  }

  groupPrimary.hidden = groupPrimary.childElementCount === 0;
  groupPinned.hidden = groupPinned.childElementCount === 0;
  groupDesktop.hidden = groupDesktop.childElementCount === 0;
  groupWeb.hidden = groupWeb.childElementCount === 0;

  if (sepPinned) sepPinned.hidden = groupPinned.hidden;
  if (sepDesktop) sepDesktop.hidden = groupDesktop.hidden;
  if (sepWeb) sepWeb.hidden = groupWeb.hidden;
  if (headingWeb) headingWeb.hidden = groupWeb.hidden;

  syncRailTooltipsForWebView(isWebViewStageActive() && state.overlayBlockers.size === 0);
  installRailRovingFallback();
}

function installRailRovingFallback() {
  if (typeof HTMLElement !== 'undefined' && 'focusGroup' in HTMLElement.prototype) return;
  const rail = document.getElementById('rail-accounts');
  if (!rail) return;
  const items = Array.from(rail.querySelectorAll('.rail-account-btn'));
  if (items.length === 0) return;

  const currentIdx = Math.max(0, items.findIndex((b) => b.getAttribute('aria-pressed') === 'true'));
  items.forEach((b, i) => {
    b.tabIndex = i === currentIdx ? 0 : -1;
  });

  if (rail.dataset.rovingWired === 'true') return;
  rail.dataset.rovingWired = 'true';

  rail.addEventListener('focusin', (event) => {
    const target = event.target?.closest?.('.rail-account-btn');
    if (!target) return;
    for (const b of rail.querySelectorAll('.rail-account-btn')) {
      b.tabIndex = b === target ? 0 : -1;
    }
  });

  rail.addEventListener('keydown', (event) => {
    const btns = Array.from(rail.querySelectorAll('.rail-account-btn'));
    if (btns.length === 0) return;
    const idx = btns.indexOf(document.activeElement);
    if (idx < 0) return;

    let nextIdx = -1;
    if (event.key === 'ArrowDown') nextIdx = (idx + 1) % btns.length;
    else if (event.key === 'ArrowUp') nextIdx = (idx - 1 + btns.length) % btns.length;
    else if (event.key === 'Home') nextIdx = 0;
    else if (event.key === 'End') nextIdx = btns.length - 1;
    else return;

    event.preventDefault();
    for (const b of btns) b.tabIndex = -1;
    btns[nextIdx].tabIndex = 0;
    btns[nextIdx].focus();
    btns[nextIdx].scrollIntoView({ block: 'nearest', inline: 'nearest' });
  });
}

/* ========================================================================
 * Unified One-Click Activation (activate_account)
 * ======================================================================== */
export async function activateAccount(id, kind) {
  if (!id || !kind) return;
  if (state.pendingProfileActions.has(id)) return;

  const seq = ++state.activationSeq;
  clearProfileError(id);
  state.activeAccount = { id, kind };
  state.lastActivationResult = null;
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(ACTIVE_ACCOUNT_KEY, JSON.stringify({ id, kind }));
    }
  } catch (_err) {
    // Ignore
  }

  state.pendingProfileActions.set(id, 'activate');
  selectView(kind === 'web_isolated' ? 'web-stage' : 'desktop-stage');
  renderProfiles();
  renderAccountRail();
  renderActiveStage();

  try {
    const invoke = getTauriInvoke();
    let res;
    if (invoke) {
      const env = await tauriInvokeEnvelope('activate_account', { id, kind });
      if (seq !== state.activationSeq) return;
      if (env && env.ok === false) {
        const isInfra = env.error_code === 'HOST_EXEC_FAILED' || env.error_code === 'HOST_TIMEOUT' || env.error_code === 'UNAUTHORIZED';
        if (isInfra) showErrorBanner(env.error_code, env.message);
        else showProfileError(id, env.error_code, env.message);
        state.lastActivationResult = {
          outcome: 'failed',
          remediation: { code: env.error_code, message: env.message }
        };
        if (env.error_code === 'NOT_FOUND') {
          state.activeAccount = null;
          selectView('profiles');
        }
      } else {
        state.lastActivationResult = env?.data || null;
      }
    } else if (kind === 'desktop_isolated') {
      const prof = (state.status?.profiles || []).find((p) => p.id === id);
      const action = prof?.running ? 'focus' : 'launch';
      res = await apiCall(`/api/profiles/${encodeURIComponent(id)}/${action}`, 'POST', {});
      if (seq !== state.activationSeq) return;
      if (res && res.ok === false) {
        const isInfra = res.error_code === 'HOST_EXEC_FAILED' || res.error_code === 'HOST_TIMEOUT' || res.error_code === 'UNAUTHORIZED';
        if (isInfra) showErrorBanner(res.error_code, res.message);
        else showProfileError(id, res.error_code, res.message);
        state.lastActivationResult = {
          outcome: 'failed',
          remediation: { code: res.error_code, message: res.message }
        };
      } else {
        state.lastActivationResult = { outcome: prof?.running ? 'focused' : 'launched', message: res?.message || '' };
      }
    } else if (kind === 'desktop_primary') {
      const running = Boolean(state.workspace?.primary?.running);
      state.lastActivationResult = running
        ? { outcome: 'focused', message: t('accounts.primaryZalo.running') }
        : {
          outcome: 'manual_action_required',
          remediation: { code: 'PRIMARY_NOT_RUNNING', message: t('errors.PRIMARY_NOT_RUNNING') }
        };
    }

    if (kind === 'web_isolated') {
      await refreshWorkspaceFast();
    } else {
      await refreshAccountStatusFast();
    }
    if (seq !== state.activationSeq) return;
    announceToScreenReader(findAccountDisplayName({ id, kind }));
  } catch (err) {
    if (seq !== state.activationSeq) return;
    const code = err?.errorCode || 'HOST_EXEC_FAILED';
    if (code === 'HOST_EXEC_FAILED' || code === 'HOST_TIMEOUT' || code === 'UNAUTHORIZED') {
      showErrorBanner(code, err?.message || '');
    } else {
      showProfileError(id, code, err?.message || '');
    }
  } finally {
    state.pendingProfileActions.delete(id);
    renderTopbarSummary();
    renderProfiles();
    renderPrimaryZalo();
    renderAccountRail();
    renderActiveStage();
    if (seq === state.activationSeq) {
      syncNativeWebViewState();
    }
  }
}

function updateWebViewportPlaceholder() {
  const placeholder = document.getElementById('web-viewport-placeholder');
  const progress = document.getElementById('web-stage-progress');
  const titleEl = document.getElementById('web-placeholder-title');
  const bodyEl = document.getElementById('web-placeholder-body');
  if (!placeholder || !titleEl || !bodyEl) return;

  const supported = Boolean(state.workspace?.web_workspace?.supported);
  const invoke = getTauriInvoke();

  if (!invoke) {
    placeholder.hidden = false;
    if (progress) progress.hidden = true;
    titleEl.textContent = t('webSession.previewNotice');
    bodyEl.textContent = t('webSession.limitsBody');
    return;
  }

  if (!supported) {
    placeholder.hidden = false;
    if (progress) progress.hidden = true;
    titleEl.textContent = t('webSession.unsupportedTitle');
    const code = state.workspace?.web_workspace?.reason_code;
    bodyEl.textContent = code ? resolveErrorInfo(code).userMessage : t('webSession.unsupportedBody');
    return;
  }

  if (state.overlayBlockers.size > 0) {
    placeholder.hidden = false;
    if (progress) progress.hidden = true;
    titleEl.textContent = t('webSession.pausedForOverlay');
    bodyEl.textContent = '';
    return;
  }

  if (state.mountedWebSessionId === state.activeAccount?.id) {
    placeholder.hidden = true;
    return;
  }

  placeholder.hidden = false;
  if (progress) progress.hidden = false;
  titleEl.textContent = t('webSession.loading');
  bodyEl.textContent = '';
}

function renderWebStage() {
  if (!state.activeAccount || state.activeAccount.kind !== 'web_isolated') return;
  const acc = (state.workspace?.accounts || []).find(
    (a) => a.id === state.activeAccount.id && a.kind === 'web_isolated'
  );
  if (!acc) {
    const nameEl = document.getElementById('web-stage-name');
    if (nameEl) nameEl.textContent = '';
    state.activeAccount = null;
    if (state.activeView === 'web-stage') {
      selectView('profiles');
    }
    return;
  }

  const avatar = document.getElementById('web-stage-avatar');
  const nameEl = document.getElementById('web-stage-name');
  const pinBtn = document.getElementById('btn-web-pin');
  if (avatar) {
    avatar.textContent = getAccountInitials(acc.display_name);
    avatar.style.setProperty('--avatar-color', getAccountColorVar({ id: acc.id, name: acc.display_name, color_token: acc.color_token }));
  }
  if (nameEl) nameEl.textContent = acc.display_name;
  if (pinBtn) pinBtn.textContent = acc.pinned ? t('rail.unpin') : t('rail.pin');

  updateWebViewportPlaceholder();
}

function renderDesktopStage() {
  if (!state.activeAccount || state.activeAccount.kind === 'web_isolated') return;

  const isPrimary = state.activeAccount.kind === 'desktop_primary';
  const acc = isPrimary
    ? {
      id: 'primary',
      kind: 'desktop_primary',
      display_name: t('accounts.primaryZalo.heading'),
      running: Boolean(state.workspace?.primary?.running),
      window_state: state.workspace?.primary?.window_state || 'none',
      window_title: state.workspace?.primary?.window_title || null,
      pinned: Boolean(state.workspace?.primary?.pinned)
    }
    : (state.workspace?.accounts || []).find((a) => a.id === state.activeAccount.id && a.kind === 'desktop_isolated');

  const prof = !isPrimary
    ? (state.status?.profiles || []).find((p) => p.id === state.activeAccount.id)
    : null;

  if (!acc && !isPrimary) {
    const nameEl = document.getElementById('desktop-stage-name');
    const kv = document.getElementById('desktop-stage-kv');
    if (nameEl) nameEl.textContent = '';
    if (kv) kv.replaceChildren();
    state.activeAccount = null;
    state.lastActivationResult = null;
    if (state.activeView === 'desktop-stage') {
      selectView('profiles');
    }
    return;
  }

  const avatar = document.getElementById('desktop-stage-avatar');
  const nameEl = document.getElementById('desktop-stage-name');
  const kindBadge = document.getElementById('desktop-stage-kind-badge');
  const stateRow = document.getElementById('desktop-stage-state-row');
  const stateLabel = document.getElementById('desktop-stage-state-label');
  const progress = document.getElementById('desktop-stage-progress');
  const activateBtn = document.getElementById('btn-desktop-activate');
  const stopBtn = document.getElementById('btn-desktop-stop');
  const primaryNotice = document.getElementById('desktop-primary-notice');
  const snapPanel = document.getElementById('desktop-snap-panel');
  const snapToggle = document.getElementById('toggle-companion-snap');
  const snapHint = document.getElementById('desktop-snap-hint');
  const secondaryTools = document.getElementById('desktop-secondary-tools');
  const remediationCard = document.getElementById('desktop-remediation-card');
  const remTitle = document.getElementById('desktop-remediation-title');
  const remBody = document.getElementById('desktop-remediation-body');
  const remActions = document.getElementById('desktop-remediation-actions');

  const displayName = acc?.display_name || prof?.name || '';
  const isPending = state.pendingProfileActions.has(state.activeAccount.id);
  const breadcrumbName = document.getElementById('desktop-breadcrumb-name');
  if (breadcrumbName) breadcrumbName.textContent = displayName;

  if (avatar) {
    avatar.className = isPrimary ? 'account-avatar account-avatar-lg avatar-protected' : 'account-avatar account-avatar-lg';
    if (isPrimary) {
      avatar.style.removeProperty('--avatar-color');
      avatar.textContent = 'Z';
    } else {
      avatar.style.setProperty('--avatar-color', getAccountColorVar({ id: acc.id, name: displayName, color_token: acc.color_token }));
      avatar.textContent = getAccountInitials(displayName);
    }
  }
  if (nameEl) nameEl.textContent = displayName;
  if (kindBadge) kindBadge.textContent = isPrimary ? t('accounts.primaryZalo.protectedBadge') : t('rail.kindDesktop');

  const resolvedState = isPrimary
    ? (acc.running ? { label: t('accounts.states.running'), kind: 'running' } : { label: t('accounts.states.stopped'), kind: 'stopped' })
    : resolveAccountState(prof || { id: acc.id, running: acc.state === 'running', lifecycle_state: acc.state });

  if (stateRow) stateRow.className = `account-state-row is-${resolvedState.kind}`;
  if (stateLabel) stateLabel.textContent = resolvedState.label;
  if (progress) progress.hidden = !isPending;

  if (activateBtn) {
    activateBtn.disabled = isPending;
    if (isPrimary) {
      activateBtn.hidden = !acc.running;
      activateBtn.textContent = t('desktopStage.bringToFront');
    } else {
      activateBtn.hidden = false;
      const lc = normalizeLifecycleState(prof || acc);
      if (lc === 'recovery_required') {
        activateBtn.textContent = t('accounts.actions.recover');
      } else if (prof?.running || acc.state === 'running') {
        activateBtn.textContent = t('desktopStage.bringToFront');
      } else {
        activateBtn.textContent = t('desktopStage.openAndFocus');
      }
    }
  }

  if (stopBtn) {
    stopBtn.hidden = isPrimary || !(prof?.running || acc?.state === 'running');
    stopBtn.disabled = isPending;
  }

  if (primaryNotice) primaryNotice.hidden = !isPrimary;

  if (snapPanel && snapToggle && snapHint) {
    snapPanel.hidden = isPrimary;
    const isRunning = Boolean(prof?.running || acc?.state === 'running');
    const platform = String(state.status?.platform || 'windows').toLowerCase();
    const isWindows = platform === 'windows';
    snapToggle.checked = Boolean(acc?.companion_snapped);
    snapToggle.disabled = isPrimary || !isWindows || !isRunning || isPending;
    snapHint.textContent = !isWindows
      ? t('desktopStage.snapOsUnsupported')
      : (isRunning ? t('desktopStage.companionSnapHint') : t('desktopStage.companionSnapStopped'));
  }

  if (secondaryTools) {
    secondaryTools.hidden = isPrimary;
    if (!isPrimary && (prof || acc)) {
      const pinBtn = document.getElementById('btn-desktop-pin');
      const backupBtn = document.getElementById('btn-desktop-backup');
      const delBtn = document.getElementById('btn-desktop-delete');
      const blockOffline = isProfileActiveOrUnclean(prof || acc);
      if (pinBtn) pinBtn.textContent = acc?.pinned ? t('desktopStage.unpinAction') : t('desktopStage.pinAction');
      if (backupBtn) {
        backupBtn.disabled = blockOffline || isPending;
        backupBtn.title = blockOffline ? t('accounts.actions.closeFirstHint') : '';
      }
      if (delBtn) {
        delBtn.disabled = blockOffline || isPending;
        delBtn.title = blockOffline ? t('accounts.actions.closeFirstHint') : '';
      }
      const kv = document.getElementById('desktop-stage-kv');
      if (kv) {
        kv.replaceChildren();
        appendKvRow(kv, t('accounts.details.directory'), prof?.directory_name || '—');
        appendKvRow(kv, t('accounts.details.pid'), prof?.main_pid != null ? String(prof.main_pid) : '—');
        appendKvRow(kv, t('accounts.details.windowTitle'), acc?.window_title || prof?.window_title || '—');
        appendKvRow(kv, t('accounts.details.workingSet'), prof?.total_working_set_bytes ? formatBytes(prof.total_working_set_bytes) : '—');
      }
    }
  }

  if (remediationCard && remTitle && remBody && remActions) {
    const profileErr = state.profileErrors.get(state.activeAccount.id);
    const errBehavior = profileErr ? classifyErrorBehavior(profileErr.errorCode) : null;
    const rem = profileErr
      ? {
          code: profileErr.errorCode,
          message: profileErr.rawDetail,
          suggested_action: errBehavior.action === 'open_diagnostics'
            ? 'open_diagnostics'
            : (errBehavior.retryable ? 'retry_after_stop' : 'dismiss')
        }
      : (state.lastActivationResult?.remediation || null);
    const outcome = profileErr
      ? (errBehavior.category === 'action_required' ? 'manual_action_required' : 'failed')
      : (state.lastActivationResult?.outcome || null);
    if (rem && (outcome === 'manual_action_required' || outcome === 'blocked' || outcome === 'failed')) {
      const remBehavior = classifyErrorBehavior(rem.code);
      remediationCard.hidden = false;
      remediationCard.setAttribute('role', remBehavior.liveRole);
      remTitle.textContent = outcome === 'manual_action_required'
        ? t('desktopStage.manualActionTitle')
        : t('desktopStage.remediationTitle');
      const info = resolveErrorInfo(rem.code, rem.message);
      remBody.textContent = info.userMessage;
      remActions.replaceChildren();
      if (rem.suggested_action === 'open_diagnostics' || (!remBehavior.retryable && remBehavior.action === 'open_diagnostics')) {
        remActions.appendChild(makeButton(t('desktopStage.openDiagnostics'), 'btn-secondary', (ev) => selectView('diagnostics', { triggerEl: ev.currentTarget, returnView: 'desktop-stage' })));
      } else if (rem.suggested_action === 'recover_profile' && remBehavior.retryable) {
        remActions.appendChild(makeButton(t('accounts.actions.recover'), 'btn-primary', () => {
          if (state.activeAccount) activateAccount(state.activeAccount.id, state.activeAccount.kind);
        }));
        remActions.appendChild(makeButton(t('desktopStage.openDiagnostics'), 'btn-secondary', (ev) => selectView('diagnostics', { triggerEl: ev.currentTarget, returnView: 'desktop-stage' })));
      } else if (rem.suggested_action === 'retry_after_stop' && remBehavior.retryable) {
        remActions.appendChild(makeButton(t('desktopStage.retryAction'), 'btn-secondary', () => {
          if (state.activeAccount) activateAccount(state.activeAccount.id, state.activeAccount.kind);
        }));
      } else if (rem.suggested_action === 'refresh_status') {
        remActions.appendChild(makeButton(t('desktopStage.refreshAction'), 'btn-secondary', () => refreshAll()));
      } else if (rem.suggested_action === 'use_desktop_isolated') {
        remActions.appendChild(makeButton(t('desktopStage.useDesktopAction'), 'btn-primary', () => {
          document.getElementById('btn-open-create')?.click();
        }));
      }
    } else {
      remediationCard.hidden = true;
      remediationCard.removeAttribute('role');
      remActions.replaceChildren();
    }
  }
}

function renderActiveStage() {
  if (state.activeView === 'web-stage') renderWebStage();
  else if (state.activeView === 'desktop-stage') renderDesktopStage();
}

function renderPrimaryZalo() {
  const section = document.getElementById('primary-zalo-section');
  const rowEl = document.getElementById('primary-zalo-state-row');
  const descEl = document.getElementById('primary-zalo-desc');
  if (!section || !descEl) return;

  section.hidden = false;
  const primary = state.status?.primary_zalo;
  if (!state.connectionOk || !primary || typeof primary.is_running !== 'boolean') {
    if (rowEl) rowEl.className = 'account-state-row is-unknown';
    descEl.textContent = t('accounts.primaryZalo.unknown');
  } else if (primary.is_running === true) {
    if (rowEl) rowEl.className = 'account-state-row is-running';
    descEl.textContent = t('accounts.primaryZalo.running');
  } else {
    if (rowEl) rowEl.className = 'account-state-row is-stopped';
    descEl.textContent = t('accounts.primaryZalo.stopped');
  }
}

function filterProfiles(profiles, query) {
  const q = String(query || '').trim().toLowerCase();
  if (!q) return profiles;
  return profiles.filter((p) => (
    String(p.name || '').toLowerCase().includes(q)
    || String(p.notes || '').toLowerCase().includes(q)
  ));
}

function positionMenu(trigger, menuList) {
  const rect = trigger.getBoundingClientRect();
  const menuWidth = menuList.offsetWidth || 200;
  const menuHeight = menuList.offsetHeight || 190;
  const left = Math.max(8, Math.min(rect.right - menuWidth, window.innerWidth - menuWidth - 8));
  let top;
  if (rect.bottom + 4 + menuHeight > window.innerHeight - 8 && rect.top > menuHeight + 8) {
    top = Math.max(8, rect.top - menuHeight - 4);
  } else {
    top = Math.min(rect.bottom + 4, Math.max(8, window.innerHeight - menuHeight - 8));
  }
  menuList.style.left = `${left}px`;
  menuList.style.top = `${top}px`;
}

function buildOverflowMenu(prof, isPending) {
  const menuId = `menu-${String(prof.id).replace(/[^a-zA-Z0-9_-]/g, '_')}`;
  const menuWrap = document.createElement('div');
  menuWrap.className = 'menu-wrap';

  const supportsPopover = typeof HTMLElement !== 'undefined' && Object.hasOwn(HTMLElement.prototype, 'popover');

  const moreBtn = document.createElement('button');
  moreBtn.type = 'button';
  moreBtn.className = 'btn btn-secondary btn-sm';
  moreBtn.textContent = '⋯';
  moreBtn.disabled = isPending;
  moreBtn.setAttribute('aria-label', t('accounts.actions.more', { name: prof.name }));
  moreBtn.setAttribute('aria-haspopup', 'menu');
  moreBtn.setAttribute('aria-expanded', 'false');
  moreBtn.setAttribute('aria-controls', menuId);

  const menuList = document.createElement('div');
  menuList.id = menuId;
  menuList.className = 'overflow-menu';
  menuList.setAttribute('role', 'menu');
  if (supportsPopover) {
    menuList.setAttribute('popover', 'auto');
  } else {
    menuList.hidden = true;
  }

  const getEnabledItems = () => Array.from(menuList.querySelectorAll('[role="menuitem"]:not(:disabled)'));
  const isOpen = () => (supportsPopover ? menuList.matches(':popover-open') : !menuList.hidden);

  const openMenu = (focusMode = 'first') => {
    if (isPending) return;
    state.openMenuProfileId = prof.id;
    addOverlayBlocker(`menu:${menuId}`);
    if (supportsPopover) {
      if (!isOpen()) menuList.showPopover();
      positionMenu(moreBtn, menuList);
    } else {
      menuList.hidden = false;
      menuList.classList.add('is-fallback-open');
    }
    moreBtn.setAttribute('aria-expanded', 'true');
    const items = getEnabledItems();
    if (items.length > 0 && focusMode !== 'none') {
      const target = focusMode === 'last' ? items[items.length - 1] : items[0];
      target.focus();
    }
  };

  const closeMenu = ({ restoreFocus = false } = {}) => {
    if (state.openMenuProfileId === prof.id) {
      state.openMenuProfileId = null;
    }
    removeOverlayBlocker(`menu:${menuId}`);
    if (supportsPopover) {
      if (isOpen()) {
        try {
          menuList.hidePopover();
        } catch (_err) {
          // Ignore
        }
      }
    } else {
      menuList.hidden = true;
      menuList.classList.remove('is-fallback-open');
    }
    moreBtn.setAttribute('aria-expanded', 'false');
    if (restoreFocus) {
      moreBtn.focus();
    }
  };

  if (supportsPopover) {
    menuList.addEventListener('toggle', (event) => {
      const openNow = event.newState === 'open';
      moreBtn.setAttribute('aria-expanded', openNow ? 'true' : 'false');
      if (openNow) {
        state.openMenuProfileId = prof.id;
        addOverlayBlocker(`menu:${menuId}`);
        positionMenu(moreBtn, menuList);
      } else {
        if (state.openMenuProfileId === prof.id) state.openMenuProfileId = null;
        removeOverlayBlocker(`menu:${menuId}`);
      }
    });
  } else {
    const onDocClick = (event) => {
      if (!menuWrap.isConnected) {
        document.removeEventListener('click', onDocClick);
        state.fallbackMenuCleanups.delete(cleanupDocClick);
        return;
      }
      if (isOpen() && !menuWrap.contains(event.target)) {
        closeMenu({ restoreFocus: false });
      }
    };
    const cleanupDocClick = () => {
      document.removeEventListener('click', onDocClick);
    };
    state.fallbackMenuCleanups.add(cleanupDocClick);
    document.addEventListener('click', onDocClick);
  }

  moreBtn.addEventListener('click', () => {
    if (isOpen()) closeMenu({ restoreFocus: false });
    else openMenu('first');
  });

  moreBtn.addEventListener('keydown', (event) => {
    if (event.key === 'ArrowDown' || event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      openMenu('first');
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      openMenu('last');
    }
  });

  menuList.addEventListener('keydown', (event) => {
    const items = getEnabledItems();
    if (event.key === 'Escape') {
      event.preventDefault();
      closeMenu({ restoreFocus: true });
      return;
    }
    if (event.key === 'Tab') {
      closeMenu({ restoreFocus: false });
      return;
    }
    if (items.length === 0) return;

    const currentIdx = items.indexOf(document.activeElement);
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      items[currentIdx === -1 ? 0 : (currentIdx + 1) % items.length].focus();
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      items[currentIdx <= 0 ? items.length - 1 : currentIdx - 1].focus();
    } else if (event.key === 'Home') {
      event.preventDefault();
      items[0].focus();
    } else if (event.key === 'End') {
      event.preventDefault();
      items[items.length - 1].focus();
    }
  });

  const addMenuItem = (label, onSelect, { disabled = false, danger = false, hint = '' } = {}) => {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = danger ? 'menu-item menu-item-danger' : 'menu-item';
    item.setAttribute('role', 'menuitem');
    item.tabIndex = -1;

    const labelSpan = document.createElement('span');
    labelSpan.textContent = label;
    item.appendChild(labelSpan);

    if (disabled && hint) {
      const hintSpan = document.createElement('span');
      hintSpan.className = 'menu-item-hint';
      hintSpan.textContent = hint;
      item.appendChild(hintSpan);
    }

    item.disabled = disabled || isPending;
    item.addEventListener('click', () => {
      closeMenu({ restoreFocus: false });
      onSelect();
    });
    menuList.appendChild(item);
  };

  const blockOfflineOps = isProfileActiveOrUnclean(prof);
  if (prof.account_kind === 'web_isolated') {
    addMenuItem(prof.pinned ? t('rail.unpin') : t('rail.pin'), () => toggleAccountPinned(prof.id, Boolean(prof.pinned)));
    addMenuItem(t('accounts.actions.rename'), () => openRenameDialog(prof, 'web_isolated'));
    addMenuItem(t('webSession.reset'), () => openResetWebDialog(prof));
    addMenuItem(
      t('accounts.actions.delete'),
      () => openDeleteDialog(prof, 'web_isolated'),
      { danger: true }
    );
  } else {
    addMenuItem(t('accounts.actions.rename'), () => openRenameDialog(prof, 'desktop_isolated'));
    addMenuItem(
      t('accounts.actions.backup'),
      () => runProfileAction('backup', prof),
      { disabled: blockOfflineOps, hint: t('accounts.actions.closeFirstHint') }
    );
    addMenuItem(t('accounts.actions.details'), () => openDetailsDialog(prof));
    addMenuItem(
      t('accounts.actions.delete'),
      () => openDeleteDialog(prof, 'desktop_isolated'),
      { disabled: blockOfflineOps, danger: true, hint: t('accounts.actions.closeFirstHint') }
    );
  }

  menuWrap.appendChild(moreBtn);
  menuWrap.appendChild(menuList);
  return menuWrap;
}

function renderProfiles() {
  const list = document.getElementById('profiles-list');
  const emptyEl = document.getElementById('profiles-empty');
  const emptyTitle = document.getElementById('profiles-empty-title');
  const emptyHint = document.getElementById('profiles-empty-hint');
  const emptyBtn = document.getElementById('btn-empty-create');
  const overviewCreateBtn = document.getElementById('btn-overview-create');
  const stopAllBtn = document.getElementById('btn-stop-all');
  const searchWrap = document.getElementById('search-wrap');
  const countSummary = document.getElementById('accounts-count-summary');
  if (!list || !emptyEl) return;

  const reopenMenuId = state.openMenuProfileId;
  const focusWasInMenu = Boolean(document.activeElement?.closest?.('.overflow-menu'));

  for (const cleanup of state.fallbackMenuCleanups) {
    cleanup();
  }
  state.fallbackMenuCleanups.clear();
  let removedMenuBlocker = false;
  for (const key of Array.from(state.overlayBlockers)) {
    if (key.startsWith('menu:')) {
      state.overlayBlockers.delete(key);
      removedMenuBlocker = true;
    }
  }

  list.replaceChildren();
  const desktopProfiles = (state.status?.profiles || []).map((p) => ({
    ...p,
    account_kind: 'desktop_isolated'
  }));
  const webProfiles = (state.workspace?.accounts || [])
    .filter((a) => a.kind === 'web_isolated')
    .map((w) => ({
      id: w.id,
      name: w.display_name,
      display_name: w.display_name,
      color_token: w.color_token,
      pinned: Boolean(w.pinned),
      notes: '',
      running: w.state === 'running',
      lifecycle_state: w.state || 'stopped',
      account_kind: 'web_isolated'
    }));
  const allProfiles = [...desktopProfiles, ...webProfiles];
  const filtered = filterProfiles(allProfiles, state.searchQuery);
  const hasQuery = state.searchQuery.trim().length > 0;
  const runningProfiles = desktopProfiles.filter((p) => Boolean(p.running) || normalizeLifecycleState(p) === 'running');

  if (overviewCreateBtn) {
    overviewCreateBtn.hidden = allProfiles.length === 0;
  }
  if (stopAllBtn) {
    stopAllBtn.hidden = runningProfiles.length < 2;
    stopAllBtn.disabled = state.pendingGlobalActions.has('stop-all');
  }

  try {
    const savedMode = typeof localStorage !== 'undefined' ? localStorage.getItem('khoang.viewMode') : null;
    if (savedMode === 'list' || savedMode === 'grid') {
      list.dataset.viewMode = savedMode;
    }
  } catch (_err) {
    // Ignore storage access errors
  }
  const activeMode = list.dataset.viewMode === 'list' ? 'list' : 'grid';
  document.getElementById('btn-view-grid')?.setAttribute('aria-pressed', String(activeMode === 'grid'));
  document.getElementById('btn-view-list')?.setAttribute('aria-pressed', String(activeMode === 'list'));

  if (searchWrap) {
    searchWrap.hidden = allProfiles.length < 2;
  }

  if (countSummary) {
    if (allProfiles.length > 0 && (allProfiles.length >= 6 || hasQuery)) {
      countSummary.textContent = t('accounts.countSummary', {
        shown: formatNumber(filtered.length),
        total: formatNumber(allProfiles.length)
      });
      countSummary.hidden = false;
    } else {
      countSummary.textContent = '';
      countSummary.hidden = true;
    }
  }

  if (allProfiles.length === 0) {
    emptyTitle.textContent = t('accounts.emptyTitle');
    emptyHint.textContent = t('accounts.emptyHint');
    if (emptyBtn) emptyBtn.hidden = false;
    emptyEl.hidden = false;
    list.hidden = true;
    return;
  }

  if (filtered.length === 0) {
    emptyTitle.textContent = t('accounts.noMatches', { query: state.searchQuery.trim() });
    emptyHint.textContent = '';
    if (emptyBtn) emptyBtn.hidden = true;
    emptyEl.hidden = false;
    list.hidden = true;
    return;
  }

  emptyEl.hidden = true;
  list.hidden = false;

  for (const prof of filtered) {
    const isWebAccount = prof.account_kind === 'web_isolated';
    const isPending = state.pendingProfileActions.has(prof.id);
    const accState = resolveAccountState(prof);
    const lcState = normalizeLifecycleState(prof);
    const isSelected = state.activeAccount?.id === prof.id
      && state.activeAccount?.kind === (isWebAccount ? 'web_isolated' : 'desktop_isolated');

    const li = document.createElement('li');
    li.className = `account-card${prof.running ? ' is-running' : ''}${isSelected ? ' is-selected' : ''}`;
    li.dataset.profileId = prof.id;
    li.dataset.accountKind = isWebAccount ? 'web_isolated' : 'desktop_isolated';
    li.dataset.lifecycle = lcState;
    li.addEventListener('contextmenu', (event) => {
      if (event.target?.closest?.('button, input, summary, a')) return;
      event.preventDefault();
      openAccountContextMenu(
        { ...prof, kind: isWebAccount ? 'web_isolated' : 'desktop_isolated', display_name: prof.name },
        li.querySelector('button[aria-haspopup="menu"]') || li,
        { clientX: event.clientX, clientY: event.clientY }
      );
    });

    const head = document.createElement('div');
    head.className = 'account-card-head';

    const avatar = document.createElement('div');
    avatar.className = 'account-avatar';
    avatar.style.setProperty('--avatar-color', getAccountColorVar(prof));
    avatar.setAttribute('aria-hidden', 'true');
    avatar.textContent = getAccountInitials(prof.name);

    const identity = document.createElement('div');
    identity.className = 'account-identity';

    const nameEl = document.createElement('h2');
    nameEl.className = 'account-name';
    nameEl.textContent = prof.name;
    if (isWebAccount) {
      const titleRow = document.createElement('div');
      titleRow.className = 'account-card-title-row';
      titleRow.appendChild(nameEl);
      titleRow.appendChild(makeBadge(t('rail.kindWeb'), 'warn'));
      identity.appendChild(titleRow);
    } else {
      identity.appendChild(nameEl);
    }

    const stateRow = document.createElement('div');
    stateRow.className = `account-state-row is-${accState.kind}`;
    stateRow.dataset.lifecycle = lcState;
    const dot = document.createElement('span');
    dot.className = 'state-dot';
    dot.setAttribute('aria-hidden', 'true');
    const stateText = document.createElement('span');
    stateText.textContent = accState.label;
    stateRow.appendChild(dot);
    stateRow.appendChild(stateText);
    identity.appendChild(stateRow);

    if (prof.notes) {
      const noteEl = document.createElement('p');
      noteEl.className = 'account-note';
      noteEl.textContent = prof.notes;
      identity.appendChild(noteEl);
    }

    head.appendChild(avatar);
    head.appendChild(identity);

    const actions = document.createElement('div');
    actions.className = 'account-card-actions';

    const primaryActions = document.createElement('div');
    primaryActions.className = 'account-actions-primary';

    const secondaryActions = document.createElement('div');
    secondaryActions.className = 'account-actions-secondary';

    if (isWebAccount) {
      primaryActions.appendChild(makeButton(
        t('webSession.openWeb'),
        'btn-primary',
        () => activateAccount(prof.id, 'web_isolated'),
        { disabled: isPending, ariaLabel: t('webSession.openWebAria', { name: prof.name }) }
      ));
    } else {
      const isActiveLifecycle = Boolean(prof.running) || lcState === 'starting' || lcState === 'running' || lcState === 'stopping';
      if (lcState === 'recovery_required') {
        primaryActions.appendChild(makeButton(
          t('accounts.actions.recover'),
          'btn-primary',
          () => activateAccount(prof.id, 'desktop_isolated'),
          { disabled: isPending, ariaLabel: t('accounts.actions.recoverAria', { name: prof.name }) }
        ));
      } else if (isActiveLifecycle) {
        primaryActions.appendChild(makeButton(
          t('desktopStage.bringToFront'),
          'btn-primary btn-tonal',
          () => activateAccount(prof.id, 'desktop_isolated'),
          {
            disabled: isPending || lcState === 'starting' || lcState === 'stopping',
            ariaLabel: t('accounts.actions.focusAria', { name: prof.name })
          }
        ));
        secondaryActions.appendChild(makeButton(
          t('accounts.actions.stop'),
          'btn-subtle-danger',
          () => runProfileAction('stop', prof),
          {
            disabled: isPending || lcState === 'stopping',
            ariaLabel: t('accounts.actions.stopAria', { name: prof.name })
          }
        ));
      } else {
        primaryActions.appendChild(makeButton(
          t('desktopStage.openAndFocus'),
          'btn-primary',
          () => activateAccount(prof.id, 'desktop_isolated'),
          { disabled: isPending, ariaLabel: t('accounts.actions.launchAria', { name: prof.name }) }
        ));
      }
    }

    secondaryActions.appendChild(buildOverflowMenu(prof, isPending));

    actions.appendChild(primaryActions);
    actions.appendChild(secondaryActions);

    li.appendChild(head);
    if (!isWebAccount && lcState === 'recovery_required') {
      const recHint = document.createElement('p');
      recHint.className = 'account-recovery-hint';
      recHint.textContent = t('accounts.recoveryHint');
      li.appendChild(recHint);
    }
    const inlineErr = state.profileErrors.get(prof.id);
    if (inlineErr) {
      const behavior = classifyErrorBehavior(inlineErr.errorCode);
      const errBox = document.createElement('div');
      errBox.className = 'account-inline-error' + (behavior.tone === 'info' ? ' is-info' : behavior.tone === 'warn' ? ' is-warn' : '');
      errBox.setAttribute('role', behavior.liveRole);
      errBox.dataset.profileError = inlineErr.errorCode;

      const info = resolveErrorInfo(inlineErr.errorCode, inlineErr.rawDetail);
      const errMsg = document.createElement('span');
      errMsg.className = 'account-inline-error-text';
      errMsg.textContent = info.userMessage;
      errBox.appendChild(errMsg);

      const errBtns = document.createElement('div');
      errBtns.className = 'account-inline-error-actions';
      if (behavior.retryable) {
        errBtns.appendChild(makeButton(t('messages.retry'), 'btn-secondary btn-xs', () => {
          const retryFn = inlineErr.retryFn;
          clearProfileError(prof.id);
          if (retryFn) retryFn();
          else activateAccount(prof.id, isWebAccount ? 'web_isolated' : 'desktop_isolated');
        }));
      } else if (behavior.action === 'open_diagnostics') {
        errBtns.appendChild(makeButton(t('desktopStage.openDiagnostics'), 'btn-secondary btn-xs', (ev) => {
          selectView('diagnostics', { triggerEl: ev.currentTarget, returnView: 'profiles' });
        }));
      }
      errBtns.appendChild(makeButton(t('dialogs.close'), 'btn-link btn-xs', () => {
        clearProfileError(prof.id);
        renderProfiles();
        const card = document.querySelector(`.account-card[data-profile-id="${CSS.escape(prof.id)}"]`);
        const targetBtn = card?.querySelector('.btn-primary, .btn-secondary, .account-overflow-btn');
        if (targetBtn instanceof HTMLElement) targetBtn.focus();
      }));
      errBox.appendChild(errBtns);
      li.appendChild(errBox);
    }
    li.appendChild(actions);
    list.appendChild(li);
  }

  let reopened = false;
  if (reopenMenuId) {
    const row = Array.from(list.querySelectorAll('.account-card')).find(
      (el) => el.dataset.profileId === reopenMenuId
    );
    const trigger = row?.querySelector('button[aria-haspopup="menu"]');
    const menu = row?.querySelector('.overflow-menu');
    if (trigger && menu && !trigger.disabled) {
      addOverlayBlocker(`menu:${menu.id}`);
      if (typeof HTMLElement !== 'undefined' && Object.hasOwn(HTMLElement.prototype, 'popover')) {
        try {
          menu.showPopover();
        } catch (_err) {
          // Ignore if already open
        }
        positionMenu(trigger, menu);
      } else {
        menu.hidden = false;
        menu.classList.add('is-fallback-open');
      }
      trigger.setAttribute('aria-expanded', 'true');
      state.openMenuProfileId = reopenMenuId;
      reopened = true;
      if (focusWasInMenu) {
        menu.querySelector('[role="menuitem"]:not(:disabled)')?.focus();
      }
    } else {
      state.openMenuProfileId = null;
    }
  }
  if (removedMenuBlocker && !reopened && state.overlayBlockers.size === 0) {
    syncNativeWebViewState();
  }
}

function renderBackups() {
  const tbody = document.getElementById('backups-tbody');
  const tableWrap = document.getElementById('backups-table-wrap');
  const emptyEl = document.getElementById('backups-empty');
  if (!tbody || !tableWrap || !emptyEl) return;

  tbody.replaceChildren();
  if (!state.backups || state.backups.length === 0) {
    tableWrap.hidden = true;
    emptyEl.hidden = false;
    return;
  }

  tableWrap.hidden = false;
  emptyEl.hidden = true;
  const isRestoring = state.pendingGlobalActions.has('restore');

  for (const bk of state.backups) {
    const tr = document.createElement('tr');

    const nameTh = document.createElement('th');
    nameTh.scope = 'row';
    const nameStrong = document.createElement('strong');
    nameStrong.textContent = bk.backup_name || bk.backup_id || '—';
    nameTh.appendChild(nameStrong);
    tr.appendChild(nameTh);

    const srcTd = document.createElement('td');
    srcTd.textContent = bk.source_profile_name || bk.source_profile?.name || '—';
    tr.appendChild(srcTd);

    const sizeTd = document.createElement('td');
    const filesText = t('backups.filesCount', { count: formatNumber(bk.file_count ?? 0) });
    sizeTd.textContent = `${filesText} · ${formatBytes(bk.total_bytes)}`;
    tr.appendChild(sizeTd);

    const createdTd = document.createElement('td');
    createdTd.textContent = formatDateTime(bk.created_at);
    tr.appendChild(createdTd);

    const statusTd = document.createElement('td');
    statusTd.appendChild(makeBadge(t('backups.statusReady'), 'pass'));
    tr.appendChild(statusTd);

    const actionTd = document.createElement('td');
    actionTd.appendChild(makeButton(t('backups.restoreAction'), 'btn-primary', () => openRestoreDialog(bk), { disabled: isRestoring }));
    tr.appendChild(actionTd);

    tbody.appendChild(tr);
  }
}

function renderDiagnostics() {
  const appList = document.getElementById('diag-app-list');
  const secList = document.getElementById('diag-security-list');
  const profList = document.getElementById('diag-profiles-list');
  const envList = document.getElementById('diag-env-list');
  const healthList = document.getElementById('diag-health-list');
  const isoSummaryList = document.getElementById('diag-isolation-summary-list');
  const limitsSummaryList = document.getElementById('diag-limits-summary-list');
  if (!appList || !secList || !profList || !envList) return;

  appList.replaceChildren();
  secList.replaceChildren();
  profList.replaceChildren();
  envList.replaceChildren();
  if (healthList) healthList.replaceChildren();
  if (isoSummaryList) isoSummaryList.replaceChildren();
  if (limitsSummaryList) limitsSummaryList.replaceChildren();

  const inst = state.status?.installation;
  const diag = state.diagnostics;
  const primary = state.status?.primary_zalo;
  const profiles = state.status?.profiles || [];
  const checks = state.isolation?.checks || [];

  if (healthList) {
    const installAnswer = inst?.valid === true
      ? t('diagnostics.summary.qInstallOk', { version: inst.version || '—' })
      : (inst?.valid === false ? t('diagnostics.summary.qInstallMissing') : t('diagnostics.labels.installUnknown'));
    appendKvRow(healthList, t('diagnostics.summary.qInstall'), installAnswer);

    const passCount = checks.filter((c) => resolveIsolationCheckResult(c).raw === 'PASS').length;
    const failCount = checks.filter((c) => resolveIsolationCheckResult(c).raw === 'FAIL').length;
    const unverifiedCount = checks.filter((c) => {
      const r = resolveIsolationCheckResult(c).raw;
      return r === 'NOT_TESTED' || r === 'BLOCKED' || r === 'UNKNOWN';
    }).length;
    const hasFail = failCount > 0;
    const hasUnverified = unverifiedCount > 0;
    let isolationAnswer;
    if (checks.length === 0) {
      isolationAnswer = t('diagnostics.summary.qIsolationNoData');
    } else if (hasFail) {
      isolationAnswer = t('diagnostics.summary.qIsolationWarn', {
        fail: formatNumber(failCount),
        pass: formatNumber(passCount),
        total: formatNumber(checks.length)
      });
    } else if (hasUnverified) {
      isolationAnswer = t('diagnostics.summary.qIsolationPartial', {
        pass: formatNumber(passCount),
        total: formatNumber(checks.length),
        unverified: formatNumber(unverifiedCount)
      });
    } else {
      isolationAnswer = t('diagnostics.summary.qIsolationOk', {
        pass: formatNumber(passCount),
        total: formatNumber(checks.length)
      });
    }
    appendKvRow(
      healthList,
      t('diagnostics.summary.qIsolation'),
      isolationAnswer
    );

    const nextStep = inst?.valid === false
      ? t('diagnostics.summary.qActionInstall')
      : (checks.length === 0 || hasUnverified || hasFail ? t('diagnostics.summary.qActionUnverified') : t('diagnostics.summary.qActionReady'));
    appendKvRow(healthList, t('diagnostics.summary.qAction'), nextStep);
  }

  if (isoSummaryList && limitsSummaryList) {
    for (const chk of checks) {
      const resInfo = resolveIsolationCheckResult(chk);
      const li = document.createElement('li');
      li.className = 'diag-check-card';
      const top = document.createElement('div');
      top.className = 'diag-check-card-head';
      const title = document.createElement('span');
      title.className = 'diag-check-card-name';
      title.textContent = chk.check_name || '—';
      top.appendChild(title);
      top.appendChild(makeBadge(`${resInfo.label} (${resInfo.raw})`, resInfo.variant));
      li.appendChild(top);
      if (chk.detail) {
        const det = document.createElement('p');
        det.className = 'diag-check-card-detail';
        det.textContent = chk.detail;
        li.appendChild(det);
      }
      if (resInfo.raw === 'PASS') isoSummaryList.appendChild(li);
      else limitsSummaryList.appendChild(li);
    }
  }

  const instText = inst?.valid === true
    ? t('diagnostics.labels.installDetected')
    : (inst?.valid === false ? t('diagnostics.labels.installMissing') : t('diagnostics.labels.installUnknown'));
  appendKvRow(appList, t('diagnostics.labels.installState'), instText);
  appendKvRow(appList, t('diagnostics.labels.versionMode'), inst?.version ? `${inst.version} · ${inst.mode || 'standard'}` : '—');
  appendKvRow(appList, t('diagnostics.labels.authenticode'), inst?.authenticode_status || '—');
  appendKvRow(appList, t('diagnostics.labels.dataDir'), state.status?.data_dir || '—');

  appendKvRow(secList, t('diagnostics.labels.elevation'), diag?.is_elevated ? t('diagnostics.labels.elevatedAdmin') : t('diagnostics.labels.elevatedStandard'));
  const sacState = diag?.smart_app_control_state;
  const sacText = sacState === 1
    ? t('diagnostics.labels.sacEnabled')
    : (sacState === 0 || sacState === 2 ? t('diagnostics.labels.sacDisabled') : t('diagnostics.labels.sacUnknown'));
  appendKvRow(secList, t('diagnostics.labels.smartAppControl'), sacText);
  appendKvRow(
    secList,
    t('diagnostics.labels.primaryProtection'),
    primary?.is_running
      ? t('diagnostics.labels.primaryProtected', { count: formatNumber(primary.locking_pids?.length || 1) })
      : t('diagnostics.labels.primaryIdle')
  );

  const runningCount = profiles.filter((p) => p.running).length;
  const staleCount = profiles.filter((p) => p.stale_lockfile_detected).length;
  appendKvRow(profList, t('diagnostics.labels.profileCount'), t('diagnostics.labels.profileSummary', {
    total: formatNumber(profiles.length),
    running: formatNumber(runningCount),
    stale: formatNumber(staleCount)
  }));

  const seenZaloIds = new Set();
  let dupCount = 0;
  for (const p of profiles) {
    if (p.zalo_user_id) {
      if (seenZaloIds.has(p.zalo_user_id)) dupCount += 1;
      else seenZaloIds.add(p.zalo_user_id);
    }
  }
  appendKvRow(
    profList,
    t('diagnostics.labels.duplicateZaloId'),
    dupCount > 0 ? t('diagnostics.labels.duplicateFound', { count: formatNumber(dupCount) }) : t('diagnostics.labels.duplicateNone')
  );

  appendKvRow(
    envList,
    t('diagnostics.labels.diskFree'),
    diag?.disk_free_bytes != null
      ? `${formatBytes(diag.disk_free_bytes)} (${diag.disk_free_ok ? t('diagnostics.labels.diskOk') : t('diagnostics.labels.diskLow')})`
      : '—'
  );
  appendKvRow(
    envList,
    t('diagnostics.labels.folderRedirection'),
    diag?.folder_redirection?.redirected
      ? `${t('diagnostics.labels.redirectedYes')}${diag.folder_redirection.warning ? ` · ${diag.folder_redirection.warning}` : ''}`
      : t('diagnostics.labels.redirectedNo')
  );

  const isoTbody = document.getElementById('diagnostics-tbody');
  if (isoTbody) {
    isoTbody.replaceChildren();
    const checks = state.isolation?.checks || [];
    if (checks.length === 0) {
      const tr = document.createElement('tr');
      const td = document.createElement('td');
      td.colSpan = 3;
      td.textContent = t('diagnostics.checks.noChecks');
      tr.appendChild(td);
      isoTbody.appendChild(tr);
    } else {
      for (const chk of checks) {
        const tr = document.createElement('tr');
        const nameTh = document.createElement('th');
        nameTh.scope = 'row';
        const strong = document.createElement('strong');
        strong.textContent = chk.check_name || '—';
        nameTh.appendChild(strong);
        tr.appendChild(nameTh);

        const resInfo = resolveIsolationCheckResult(chk);
        const resTd = document.createElement('td');
        resTd.appendChild(makeBadge(`${resInfo.label} (${resInfo.raw})`, resInfo.variant));
        tr.appendChild(resTd);

        const detailTd = document.createElement('td');
        detailTd.textContent = chk.detail || '—';
        tr.appendChild(detailTd);
        isoTbody.appendChild(tr);
      }
    }
  }

  const readyTbody = document.getElementById('readiness-tbody');
  if (readyTbody) {
    readyTbody.replaceChildren();
    const matrix = diag?.platform_support_matrix || [];
    if (matrix.length === 0) {
      const tr = document.createElement('tr');
      const td = document.createElement('td');
      td.colSpan = 4;
      td.textContent = t('diagnostics.readiness.empty');
      tr.appendChild(td);
      readyTbody.appendChild(tr);
    } else {
      for (const row of matrix) {
        const tr = document.createElement('tr');
        const platTh = document.createElement('th');
        platTh.scope = 'row';
        platTh.textContent = row.platform || '—';
        tr.appendChild(platTh);

        const { code, label } = translateSupportStatus(row.status);
        const variant = code === 'SUPPORTED_AND_TESTED' ? 'pass'
          : (code === 'PARTIALLY_SUPPORTED' || code === 'EXPERIMENTAL' ? 'warn'
            : (code === 'UNSUPPORTED' || code === 'BLOCKED' ? 'fail' : 'neutral'));
        const stTd = document.createElement('td');
        stTd.appendChild(makeBadge(`${label} (${code})`, variant));
        tr.appendChild(stTd);

        const stratTd = document.createElement('td');
        stratTd.textContent = row.strategy || '—';
        tr.appendChild(stratTd);

        const limTd = document.createElement('td');
        limTd.textContent = row.limits || '—';
        tr.appendChild(limTd);

        readyTbody.appendChild(tr);
      }
    }
  }

  const logBox = document.getElementById('sanitized-log-output');
  if (logBox) {
    const logs = diag?.sanitized_logs || [];
    logBox.textContent = logs.length > 0 ? logs.join('\n') : t('diagnostics.noLogs');
  }
}

function renderSettingsAbout() {
  const aboutList = document.getElementById('settings-about-list');
  if (!aboutList) return;
  aboutList.replaceChildren();

  appendKvRow(aboutList, t('settings.about.appName'), t('app.name'));
  appendKvRow(aboutList, t('settings.about.uiVersion'), state.status?.app_version || UI_VERSION);
  appendKvRow(aboutList, t('settings.about.os'), state.status?.platform || '—');

  const modeLabel = state.connectionMode === 'tauri' ? t('app.modeTauri') : t('app.modeBridge');
  const connText = state.connectionOk
    ? t('settings.about.connected', { mode: modeLabel })
    : t('settings.about.disconnected');
  appendKvRow(aboutList, t('settings.about.connection'), connText);
}

function renderColorPalette(containerId, radioName, selectedColorId = 'blue') {
  const palette = document.getElementById(containerId);
  if (!palette) return;
  const activeId = ACCOUNT_COLORS.some((c) => c.id === selectedColorId) ? selectedColorId : ACCOUNT_COLORS[0].id;
  palette.replaceChildren();

  for (const color of ACCOUNT_COLORS) {
    const label = document.createElement('label');
    label.className = 'color-swatch';
    const colorLabel = t(`colors.${color.id}`);
    label.title = colorLabel;

    const input = document.createElement('input');
    input.type = 'radio';
    input.name = radioName;
    input.value = color.id;
    input.checked = color.id === activeId;

    const dot = document.createElement('span');
    dot.className = 'color-swatch-dot';
    dot.style.setProperty('--swatch-color', `var(${color.cssVar})`);

    const srText = document.createElement('span');
    srText.className = 'visually-hidden';
    srText.textContent = colorLabel;

    label.appendChild(input);
    label.appendChild(dot);
    label.appendChild(srText);
    palette.appendChild(label);
  }
}

function renderCreateColorPalette(explicitColorId = null) {
  const palette = document.getElementById('create-color-palette');
  if (!palette) return;
  const createDialog = document.getElementById('create-profile-dialog');
  let selected = explicitColorId;
  if (!selected) {
    selected = createDialog?.open
      ? (palette.querySelector('input[name="create-account-color"]:checked')?.value || pickNextAvailableColorId(getAllColorTrackedAccounts()))
      : pickNextAvailableColorId(getAllColorTrackedAccounts());
  }
  renderColorPalette('create-color-palette', 'create-account-color', selected);
}

function renderRenameColorPalette(explicitColorId = null) {
  const palette = document.getElementById('rename-color-palette');
  if (!palette) return;
  const selected = explicitColorId
    || palette.querySelector('input[name="rename-account-color"]:checked')?.value
    || 'blue';
  renderColorPalette('rename-color-palette', 'rename-account-color', selected);
}

function updateCreateSessionModeControls() {
  const webRadio = document.getElementById('create-mode-web-radio');
  const webDesc = document.getElementById('create-mode-web-desc');
  if (!webRadio || !webDesc) return;
  const supported = Boolean(state.workspace?.web_workspace?.supported);
  const invoke = getTauriInvoke();
  if (invoke && !supported) {
    webRadio.disabled = true;
    const code = state.workspace?.web_workspace?.reason_code;
    webDesc.textContent = code ? resolveErrorInfo(code).userMessage : t('dialogs.create.modeWebUnsupported');
    const desktopRadio = document.querySelector('input[name="create-session-mode"][value="desktop_isolated"]');
    if (desktopRadio && webRadio.checked) desktopRadio.checked = true;
  } else {
    webRadio.disabled = false;
    webDesc.textContent = t('dialogs.create.modeWebDesc');
  }
}

function refreshOpenDialogTranslations() {
  const ctx = state.openDialogContext;
  if (!ctx) return;

  if (ctx.type === 'create-step2' && document.getElementById('create-profile-dialog')?.open) {
    document.getElementById('create-step2-title').textContent = t('dialogs.create.step2Title', {
      name: ctx.prof.name
    });
  } else if (ctx.type === 'rename' && document.getElementById('rename-profile-dialog')?.open) {
    document.getElementById('rename-profile-subtitle').textContent = t('dialogs.rename.subtitle', {
      name: ctx.prof.name || ctx.prof.display_name
    });
  } else if (ctx.type === 'delete' && document.getElementById('delete-profile-dialog')?.open) {
    document.getElementById('delete-profile-warning').textContent = t('dialogs.delete.warning', {
      name: ctx.prof.name || ctx.prof.display_name
    });
  } else if (ctx.type === 'restore' && document.getElementById('restore-backup-dialog')?.open) {
    document.getElementById('restore-backup-subtitle').textContent = t('dialogs.restore.subtitle', {
      backupName: ctx.bk.backup_name || ctx.bk.backup_id || ''
    });
  } else if (ctx.type === 'details' && document.getElementById('profile-details-dialog')?.open) {
    populateDetailsDialog(ctx.prof);
  }
}

function renderAll() {
  updateConnectionBadge();
  renderBanner();
  renderTopbarSummary();
  renderAccountRail();
  renderActiveStage();
  renderProfiles();
  renderPrimaryZalo();
  renderBackups();
  renderDiagnostics();
  renderSettingsAbout();
  renderCreateColorPalette();
  renderRenameColorPalette();
  updateCreateSessionModeControls();
  updateLanguageControls();
  updateAppearanceControls();
  updatePendingControls();
  updateDocumentTitle(state.activeView);
  refreshOpenDialogTranslations();
}

export async function refreshAll({ isVerify = false, quiet = false } = {}) {
  const key = isVerify ? 'verify' : 'refresh';
  if (state.pendingGlobalActions.has(key)) return;

  const seq = ++state.refreshSeq;
  state.pendingGlobalActions.add(key);
  if (!quiet) {
    updatePendingControls();
    renderTopbarSummary();
  }

  try {
    const toggleSizes = document.getElementById('toggle-sizes');
    const withSize = Boolean(toggleSizes?.checked);
    const invoke = getTauriInvoke();
    const [statusData, backupsData, diagData, isoData, wsEnv] = await Promise.all([
      apiCall(`/api/status${withSize ? '?withSize=1' : ''}`, 'GET', null, 'get_status', { withSize }),
      apiCall('/api/backups', 'GET', null, 'list_backups', {}),
      apiCall('/api/diagnostics', 'GET', null, 'get_diagnostics', {}),
      apiCall('/api/verify-isolation', 'GET', null, 'verify_isolation', {}),
      invoke ? tauriInvokeEnvelope('get_workspace_snapshot', {}).catch(() => null) : Promise.resolve(null)
    ]);
    if (seq !== state.refreshSeq) return;
    if (statusData && statusData.ok === false) {
      updateConnectionBadge(state.connectionMode || 'bridge', false);
      showErrorBanner(statusData.error_code || 'HOST_EXEC_FAILED', statusData.message || '');
    } else {
      state.status = statusData;
      if (state.banner?.kind === 'error') {
        clearBanner();
      }
    }
    if (wsEnv && wsEnv.ok && wsEnv.data) {
      state.workspace = wsEnv.data;
    } else {
      state.workspace = buildWorkspaceFallbackFromStatus(state.status);
    }
    if (backupsData && backupsData.ok !== false) {
      state.backups = backupsData.backups || [];
    }
    if (diagData && diagData.ok !== false) {
      state.diagnostics = diagData;
    }
    if (isoData && isoData.ok !== false) {
      state.isolation = isoData;
    }
  } catch (err) {
    if (seq !== state.refreshSeq) return;
    const errCode = err?.errorCode || 'HOST_EXEC_FAILED';
    const isInfraErr = errCode === 'HOST_EXEC_FAILED' || errCode === 'HOST_TIMEOUT' || errCode === 'UNAUTHORIZED';
    updateConnectionBadge(state.connectionMode || 'bridge', !isInfraErr);
    showErrorBanner(errCode, err?.message || '');
  } finally {
    state.pendingGlobalActions.delete(key);
    renderAll();
  }
}

async function refreshWorkspaceFast() {
  const invoke = getTauriInvoke();
  if (!invoke) {
    await refreshAccountStatusFast();
    return;
  }
  const env = await tauriInvokeEnvelope('get_workspace_snapshot', {}).catch(() => null);
  if (env && env.ok && env.data) {
    state.workspace = env.data;
    renderTopbarSummary();
    renderAccountRail();
    renderProfiles();
    renderActiveStage();
  }
}

async function refreshAccountStatusFast() {
  const seq = ++state.refreshSeq;
  try {
    const invoke = getTauriInvoke();
    const [statusData, wsEnv] = await Promise.all([
      apiCall('/api/status', 'GET', null, 'get_status', { withSize: false }),
      invoke ? tauriInvokeEnvelope('get_workspace_snapshot', {}).catch(() => null) : Promise.resolve(null)
    ]);
    if (seq !== state.refreshSeq) return;
    if (statusData && statusData.ok !== false) {
      state.status = statusData;
    }
    if (wsEnv && wsEnv.ok && wsEnv.data) {
      state.workspace = wsEnv.data;
    } else {
      state.workspace = buildWorkspaceFallbackFromStatus(state.status);
    }
  } catch (_err) {
    // Keep last known snapshot; full refreshAll surfaces infra banners.
  } finally {
    if (seq === state.refreshSeq) {
      renderTopbarSummary();
      renderAccountRail();
      renderProfiles();
      renderPrimaryZalo();
      renderActiveStage();
    }
  }
}

async function toggleAccountPinned(id, currentlyPinned) {
  const nextPinned = !currentlyPinned;
  const invoke = getTauriInvoke();
  if (invoke) {
    const env = await tauriInvokeEnvelope('set_account_pinned', { id, pinned: nextPinned });
    if (env && env.ok === false) {
      showErrorBanner(env.error_code, env.message);
      return;
    }
  } else {
    const list = loadJsonStorage(PINNED_STORAGE_KEY, []);
    const set = new Set(Array.isArray(list) ? list : []);
    if (nextPinned) set.add(id);
    else set.delete(id);
    saveJsonStorage(PINNED_STORAGE_KEY, Array.from(set));
  }
  await refreshWorkspaceFast();
  showToast(nextPinned ? 'messages.pinAdded' : 'messages.pinRemoved', {}, 'success', 'pin');
}

async function runProfileAction(action, prof) {
  if (state.pendingProfileActions.has(prof.id)) return;
  clearProfileError(prof.id);
  state.pendingProfileActions.set(prof.id, action);
  renderProfiles();
  renderAccountRail();
  renderActiveStage();

  try {
    let res;
    if (action === 'launch') {
      res = await apiCall(`/api/profiles/${encodeURIComponent(prof.id)}/launch`, 'POST', {}, 'launch_profile', { id: prof.id });
    } else if (action === 'focus') {
      res = await apiCall(`/api/profiles/${encodeURIComponent(prof.id)}/focus`, 'POST', {}, 'focus_profile', { id: prof.id });
    } else if (action === 'stop') {
      res = await apiCall(`/api/profiles/${encodeURIComponent(prof.id)}/stop`, 'POST', {}, 'stop_profile', { id: prof.id });
    } else if (action === 'backup') {
      res = await apiCall(`/api/profiles/${encodeURIComponent(prof.id)}/backup`, 'POST', {}, 'backup_profile', { id: prof.id });
    }
    await refreshAll({ quiet: true });
    if (res && res.ok === false) {
      const code = res.error_code;
      const isGlobalScope =
        code === 'HOST_EXEC_FAILED' ||
        code === 'HOST_TIMEOUT' ||
        code === 'UNAUTHORIZED' ||
        code === 'PROTECTED_PRIMARY_PROCESS' ||
        code === 'PRIMARY_PROTECTED';
      if (isGlobalScope) showErrorBanner(code, res.message);
      else showProfileError(prof.id, code, res.message, () => runProfileAction(action, prof));
    } else if (action === 'backup' && res?.ok !== false) {
      showKeyBanner('messages.backupCreated', {}, 'success');
    }
  } catch (err) {
    const code = err?.errorCode || 'HOST_EXEC_FAILED';
    const isGlobalScope =
      code === 'HOST_EXEC_FAILED' ||
      code === 'HOST_TIMEOUT' ||
      code === 'UNAUTHORIZED' ||
      code === 'PROTECTED_PRIMARY_PROCESS' ||
      code === 'PRIMARY_PROTECTED';
    if (isGlobalScope) {
      showErrorBanner(code, err?.message || '');
    } else {
      showProfileError(prof.id, code, err?.message || '', () => runProfileAction(action, prof));
    }
  } finally {
    state.pendingProfileActions.delete(prof.id);
    renderProfiles();
    renderAccountRail();
    renderActiveStage();
  }
}

function clearCreateStep1Error() {
  const errEl = document.getElementById('create-step1-error');
  const nameInput = document.getElementById('create-profile-name');
  if (errEl) {
    errEl.textContent = '';
    errEl.hidden = true;
  }
  if (nameInput) {
    nameInput.removeAttribute('aria-invalid');
    if (nameInput.getAttribute('aria-describedby') === 'create-step1-error') {
      nameInput.removeAttribute('aria-describedby');
    }
  }
}

function showCreateStep1Error(errorCode, rawDetail = '') {
  const errEl = document.getElementById('create-step1-error');
  const nameInput = document.getElementById('create-profile-name');
  if (!errEl) return;
  const info = resolveErrorInfo(errorCode, rawDetail);
  errEl.textContent = info.userMessage;
  errEl.hidden = false;
  if (nameInput) {
    nameInput.setAttribute('aria-invalid', 'true');
    nameInput.setAttribute('aria-describedby', 'create-step1-error');
  }
}

function needsFocusRestore(activeEl) {
  if (!activeEl || activeEl === document.body || activeEl === document.documentElement) return true;
  if (!activeEl.isConnected || activeEl.hidden || activeEl.closest?.('[hidden]')) return true;
  if (typeof getComputedStyle === 'function' && getComputedStyle(activeEl).display === 'none') return true;
  return false;
}

function resetCreateDialogToStep1() {
  state.createdAccountForStep2 = null;
  document.getElementById('create-step-1').hidden = false;
  document.getElementById('create-step-2').hidden = true;
  document.getElementById('create-profile-name').value = '';
  document.getElementById('create-profile-notes').value = '';
  const desktopRadio = document.querySelector('input[name="create-session-mode"][value="desktop_isolated"]');
  if (desktopRadio) desktopRadio.checked = true;
  const notesField = document.getElementById('create-notes-field');
  if (notesField) notesField.hidden = false;
  clearCreateStep1Error();
  renderCreateColorPalette(pickNextAvailableColorId(getAllColorTrackedAccounts()));
  updateCreateSessionModeControls();
  const submitBtn = document.getElementById('btn-submit-create');
  if (submitBtn) {
    submitBtn.disabled = false;
    submitBtn.textContent = t('dialogs.create.submit');
    submitBtn.removeAttribute('aria-busy');
  }
}

function showCreateStep2(prof) {
  state.createdAccountForStep2 = prof;
  state.openDialogContext = { type: 'create-step2', prof };
  const avatar = document.getElementById('create-step2-avatar');
  if (avatar) {
    avatar.textContent = getAccountInitials(prof.name);
    avatar.style.setProperty('--avatar-color', getAccountColorVar(prof));
  }
  document.getElementById('create-step2-title').textContent = t('dialogs.create.step2Title', {
    name: prof.name
  });
  document.getElementById('create-step-1').hidden = true;
  document.getElementById('create-step-2').hidden = false;
  document.getElementById('btn-create-launch')?.focus();
}

function showModalSafely(dialogEl) {
  if (!dialogEl || dialogEl.open) return;
  addOverlayBlocker(`dialog:${dialogEl.id}`);
  if (typeof dialogEl.showModal === 'function') {
    dialogEl.showModal();
  }
}

function openRenameDialog(prof, kind = 'desktop_isolated') {
  state.activeRenameId = prof.id;
  state.activeRenameKind = kind;
  const displayName = prof.name || prof.display_name || '';
  state.openDialogContext = { type: 'rename', prof: { ...prof, name: displayName } };
  document.getElementById('rename-profile-subtitle').textContent = t('dialogs.rename.subtitle', {
    name: displayName
  });
  const renameInput = document.getElementById('rename-profile-name');
  renameInput.value = displayName;
  renderRenameColorPalette(getAccountColorId(prof));
  showModalSafely(document.getElementById('rename-profile-dialog'));
  renameInput?.select();
}

function openDeleteDialog(prof, kind = 'desktop_isolated') {
  state.activeDeleteId = prof.id;
  state.activeDeleteKind = kind;
  const displayName = prof.name || prof.display_name || '';
  state.openDialogContext = { type: 'delete', prof: { ...prof, name: displayName } };
  document.getElementById('delete-profile-warning').textContent = t('dialogs.delete.warning', {
    name: displayName
  });
  const confirmEl = document.getElementById('delete-profile-confirm-text');
  if (confirmEl) confirmEl.hidden = kind === 'web_isolated';
  showModalSafely(document.getElementById('delete-profile-dialog'));
}

function openResetWebDialog(acc) {
  state.activeResetWebId = acc.id;
  state.openDialogContext = { type: 'reset-web', prof: acc };
  document.getElementById('reset-web-warning').textContent = t('dialogs.resetWeb.warning', {
    name: acc.display_name || acc.id
  });
  showModalSafely(document.getElementById('reset-web-dialog'));
}

function openRestoreDialog(bk) {
  state.activeRestoreBackupDir = bk.backup_dir;
  state.activeRestoreSourceId = bk.source_profile_id || bk.source_profile?.id || null;
  state.openDialogContext = { type: 'restore', bk };
  document.getElementById('restore-backup-subtitle').textContent = t('dialogs.restore.subtitle', {
    backupName: bk.backup_name || bk.backup_id || ''
  });
  document.getElementById('restore-profile-name').value = '';
  showModalSafely(document.getElementById('restore-backup-dialog'));
}

function populateDetailsDialog(prof) {
  const list = document.getElementById('profile-details-list');
  const advList = document.getElementById('profile-details-advanced-list');
  if (!list || !advList) return;
  list.replaceChildren();
  advList.replaceChildren();

  const accState = resolveAccountState(prof);
  appendKvRow(list, t('accounts.details.name'), prof.name);
  appendKvRow(list, t('accounts.details.state'), accState.label);
  appendKvRow(list, t('accounts.details.color'), t(`colors.${getAccountColorId(prof)}`));
  appendKvRow(list, t('accounts.details.createdAt'), formatDateTime(prof.created_at));
  appendKvRow(list, t('accounts.details.lastLaunched'), formatDateTime(prof.last_launched_at));
  appendKvRow(list, t('accounts.details.notes'), prof.notes || '—');

  appendKvRow(advList, t('accounts.details.id'), prof.id);
  appendKvRow(advList, t('accounts.details.directory'), prof.directory_name || '—');
  appendKvRow(advList, t('accounts.details.pid'), prof.main_pid != null ? String(prof.main_pid) : '—');
  appendKvRow(advList, t('accounts.details.processCount'), formatNumber(prof.process_count || 0));
  appendKvRow(advList, t('accounts.details.windowTitle'), prof.window_title || '—');
  appendKvRow(advList, t('accounts.details.workingSet'), prof.total_working_set_bytes ? formatBytes(prof.total_working_set_bytes) : '—');
  appendKvRow(advList, t('accounts.details.diskSize'), formatBytes(prof.data_size_bytes));
  appendKvRow(advList, t('accounts.details.zaloUserId'), prof.zalo_user_id || '—');
  appendKvRow(
    advList,
    t('accounts.details.staleLockfile'),
    prof.stale_lockfile_detected ? t('accounts.details.lockfileFound') : t('accounts.details.lockfileClean')
  );
}

function openDetailsDialog(prof) {
  state.openDialogContext = { type: 'details', prof };
  populateDetailsDialog(prof);
  showModalSafely(document.getElementById('profile-details-dialog'));
}

/* ========================================================================
 * Quick Switcher (Ctrl+K)
 * ======================================================================== */
function getFilteredSwitcherAccounts() {
  const q = state.switcherQuery.trim().toLowerCase();
  const all = getAllUnifiedAccounts();
  if (!q) return all;
  const profilesById = new Map((state.status?.profiles || []).map((p) => [p.id, p]));
  return all.filter((acc) => {
    const prof = acc.kind === 'desktop_isolated' ? profilesById.get(acc.id) : null;
    const kindLabel = acc.kind === 'desktop_primary'
      ? t('rail.kindPrimary')
      : (acc.kind === 'web_isolated' ? t('rail.kindWeb') : t('rail.kindDesktop'));
    return (
      String(acc.display_name || '').toLowerCase().includes(q)
      || String(prof?.notes || '').toLowerCase().includes(q)
      || String(acc.kind || '').toLowerCase().includes(q)
      || String(kindLabel || '').toLowerCase().includes(q)
    );
  });
}

function renderQuickSwitcherList() {
  const list = document.getElementById('quick-switcher-list');
  const empty = document.getElementById('quick-switcher-empty');
  const input = document.getElementById('quick-switcher-input');
  if (!list || !empty || !input) return;

  list.replaceChildren();
  const items = getFilteredSwitcherAccounts();
  if (items.length === 0) {
    empty.hidden = false;
    input.removeAttribute('aria-activedescendant');
    return;
  }

  empty.hidden = true;
  if (state.switcherActiveIndex >= items.length) state.switcherActiveIndex = 0;
  if (state.switcherActiveIndex < 0) state.switcherActiveIndex = items.length - 1;

  const allAccounts = getAllUnifiedAccounts();
  const profilesById = new Map((state.status?.profiles || []).map((p) => [p.id, p]));

  items.forEach((acc, idx) => {
    const opt = document.createElement('li');
    const optId = `qs-opt-${idx}`;
    opt.id = optId;
    opt.className = 'switcher-option';
    opt.setAttribute('role', 'option');
    opt.setAttribute('aria-selected', String(idx === state.switcherActiveIndex));

    const isPrimary = acc.kind === 'desktop_primary';
    const isWeb = acc.kind === 'web_isolated';
    const avatar = document.createElement('span');
    avatar.className = isPrimary ? 'account-avatar account-avatar-sm avatar-protected' : 'account-avatar account-avatar-sm';
    if (!isPrimary) {
      avatar.style.setProperty('--avatar-color', getAccountColorVar({ id: acc.id, name: acc.display_name, color_token: acc.color_token }));
    }
    avatar.setAttribute('aria-hidden', 'true');
    avatar.textContent = isPrimary ? 'Z' : getAccountInitials(acc.display_name);

    const meta = document.createElement('span');
    meta.className = 'switcher-option-meta';
    const nameSpan = document.createElement('span');
    nameSpan.className = 'switcher-option-name';
    nameSpan.textContent = acc.display_name;
    const subSpan = document.createElement('span');
    subSpan.className = 'switcher-option-sub';
    const kindText = isPrimary ? t('rail.kindPrimary') : (isWeb ? t('rail.kindWeb') : t('rail.kindDesktop'));
    const prof = acc.kind === 'desktop_isolated' ? profilesById.get(acc.id) : null;
    const stText = isPrimary
      ? (acc.state === 'running' ? t('accounts.states.running') : t('accounts.states.stopped'))
      : resolveAccountState(prof || { id: acc.id, running: acc.state === 'running', lifecycle_state: acc.state }).label;
    subSpan.textContent = `${kindText} · ${stText}`;
    meta.appendChild(nameSpan);
    meta.appendChild(subSpan);

    opt.appendChild(avatar);
    opt.appendChild(meta);

    const globalIdx = allAccounts.findIndex((a) => a.id === acc.id && a.kind === acc.kind);
    if (globalIdx >= 0 && globalIdx < 9) {
      const kbd = document.createElement('span');
      kbd.className = 'switcher-shortcut';
      kbd.setAttribute('aria-hidden', 'true');
      kbd.textContent = t('quickSwitcher.shortcutHint', { num: String(globalIdx + 1) });
      opt.appendChild(kbd);
    }

    opt.addEventListener('click', () => {
      document.getElementById('quick-switcher-dialog')?.close('select');
      activateAccount(acc.id, acc.kind);
    });

    list.appendChild(opt);
    if (idx === state.switcherActiveIndex) {
      input.setAttribute('aria-activedescendant', optId);
      opt.scrollIntoView?.({ block: 'nearest' });
    }
  });
}

function openQuickSwitcher() {
  const dlg = document.getElementById('quick-switcher-dialog');
  const input = document.getElementById('quick-switcher-input');
  if (!dlg || !input || dlg.open) return;
  state.switcherQuery = '';
  state.switcherActiveIndex = 0;
  input.value = '';
  renderQuickSwitcherList();
  showModalSafely(dlg);
  input.focus();
}

function wireLightDismissFallback() {
  if (typeof HTMLDialogElement === 'undefined' || 'closedBy' in HTMLDialogElement.prototype) return;
  const outside = (e, dialog) => {
    const r = dialog.getBoundingClientRect();
    return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
  };
  for (const dialog of document.querySelectorAll('dialog[closedby="any"]')) {
    let pressedOutside = false;
    dialog.addEventListener('pointerdown', (e) => {
      pressedOutside = outside(e, dialog);
    });
    dialog.addEventListener('pointerup', (e) => {
      if (pressedOutside && outside(e, dialog)) dialog.close('');
    });
  }
}

function wireDialogsAndFallbacks() {
  wireLightDismissFallback();

  for (const btnId of ['btn-open-create', 'btn-overview-create', 'btn-empty-create']) {
    document.getElementById(btnId)?.addEventListener('click', () => {
      clearCreateStep1Error();
      renderCreateColorPalette(pickNextAvailableColorId(getAllColorTrackedAccounts()));
      updateCreateSessionModeControls();
    });
  }

  document.getElementById('btn-quick-switcher')?.addEventListener('click', () => {
    openQuickSwitcher();
  });

  if (typeof HTMLButtonElement === 'undefined' || !('commandForElement' in HTMLButtonElement.prototype)) {
    for (const btn of document.querySelectorAll('button[command="show-modal"][commandfor]')) {
      btn.addEventListener('click', () => {
        const target = document.getElementById(btn.getAttribute('commandfor'));
        if (target && !target.open) {
          showModalSafely(target);
        }
      });
    }
    for (const btn of document.querySelectorAll('button[command="close"][commandfor]')) {
      btn.addEventListener('click', () => {
        document.getElementById(btn.getAttribute('commandfor'))?.close('close');
      });
    }
  } else {
    for (const btn of document.querySelectorAll('button[command="show-modal"][commandfor]')) {
      btn.addEventListener('click', () => {
        addOverlayBlocker(`dialog:${btn.getAttribute('commandfor')}`);
      });
    }
  }

  for (const dlg of document.querySelectorAll('dialog')) {
    if (dlg.id === 'delete-profile-dialog' || dlg.id === 'reset-web-dialog') continue;
    dlg.addEventListener('close', () => {
      removeOverlayBlocker(`dialog:${dlg.id}`);
    });
  }

  document.getElementById('btn-dismiss-banner')?.addEventListener('click', clearBanner);

  const createDialog = document.getElementById('create-profile-dialog');
  const createForm = document.getElementById('create-profile-form');

  for (const radio of document.querySelectorAll('input[name="create-session-mode"]')) {
    radio.addEventListener('change', (e) => {
      const notesField = document.getElementById('create-notes-field');
      if (notesField) notesField.hidden = e.target.value === 'web_isolated';
    });
  }

  document.getElementById('create-profile-name')?.addEventListener('input', () => {
    clearCreateStep1Error();
  });

  for (const chip of document.querySelectorAll('.chip-btn[data-suggest-key]')) {
    chip.addEventListener('click', () => {
      const input = document.getElementById('create-profile-name');
      if (input) {
        input.value = t(chip.getAttribute('data-suggest-key'));
        clearCreateStep1Error();
        input.focus();
      }
    });
  }

  document.getElementById('btn-cancel-create')?.addEventListener('click', () => {
    createDialog?.close('cancel');
  });

  document.getElementById('btn-create-later')?.addEventListener('click', () => {
    createDialog?.close('done');
  });

  document.getElementById('btn-create-launch')?.addEventListener('click', async () => {
    const prof = state.createdAccountForStep2;
    createDialog?.close('launch');
    if (prof) {
      await activateAccount(prof.id, 'desktop_isolated');
    }
  });

  createForm?.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (state.pendingGlobalActions.has('create')) return;

    const nameInput = document.getElementById('create-profile-name');
    const notesInput = document.getElementById('create-profile-notes');
    const colorInput = document.querySelector('input[name="create-account-color"]:checked');
    const modeInput = document.querySelector('input[name="create-session-mode"]:checked');
    const sessionMode = modeInput?.value === 'web_isolated' ? 'web_isolated' : 'desktop_isolated';
    const name = nameInput.value.trim();
    const notes = notesInput.value.trim();
    const chosenColor = colorInput?.value || pickNextAvailableColorId(getAllColorTrackedAccounts());
    if (!name) {
      nameInput?.focus();
      return;
    }

    clearCreateStep1Error();
    const submitBtn = document.getElementById('btn-submit-create');
    if (submitBtn) {
      submitBtn.disabled = true;
      submitBtn.textContent = t('dialogs.create.submitting');
      submitBtn.setAttribute('aria-busy', 'true');
    }
    state.pendingGlobalActions.add('create');
    updatePendingControls();

    try {
      if (sessionMode === 'web_isolated') {
        const env = await tauriInvokeEnvelope('web_session_create', {
          displayName: name,
          colorToken: chosenColor
        });
        if (!env || env.ok === false) {
          showCreateStep1Error(env?.error_code || 'WEB_SESSION_UNSUPPORTED', env?.message || '');
          nameInput?.focus();
          return;
        }
        const rec = env.data;
        if (rec?.id) saveAccountColor(rec.id, chosenColor);
        createDialog?.close('web-created');
        await refreshAll({ quiet: true });
        if (rec?.id) {
          await activateAccount(rec.id, 'web_isolated');
        }
        return;
      }

      const res = await apiCall('/api/profiles', 'POST', { name, notes }, 'create_profile', { name, notes });
      if (res && res.ok === false) {
        showCreateStep1Error(res.error_code, res.message);
        nameInput?.focus();
        return;
      }

      if (res?.data?.id) {
        saveAccountColor(res.data.id, chosenColor);
      }
      await refreshAll();
      const created = res?.data
        || (state.status?.profiles || []).find((p) => p.name === name)
        || { id: res?.id || name, name, notes };
      if (created?.id) {
        if (!state.status || typeof state.status !== 'object') {
          state.status = { profiles: [] };
        }
        if (!Array.isArray(state.status.profiles)) {
          state.status.profiles = [];
        }
        if (!state.status.profiles.some((p) => p.id === created.id)) {
          state.status.profiles.push(created);
        }
        saveAccountColor(created.id, chosenColor);
        renderProfiles();
        renderAccountRail();
        renderTopbarSummary();
      }
      showKeyBanner('messages.accountCreated', {}, 'success');
      showCreateStep2(created);
    } catch (err) {
      showCreateStep1Error(err?.errorCode || 'HOST_EXEC_FAILED', err?.message || '');
      nameInput?.focus();
    } finally {
      state.pendingGlobalActions.delete('create');
      if (submitBtn) {
        submitBtn.disabled = false;
        submitBtn.textContent = t('dialogs.create.submit');
        submitBtn.removeAttribute('aria-busy');
      }
      updatePendingControls();
    }
  });

  createDialog?.addEventListener('close', () => {
    const step2Id = state.createdAccountForStep2?.id || null;
    createDialog.returnValue = '';
    state.openDialogContext = null;
    resetCreateDialogToStep1();
    if (needsFocusRestore(document.activeElement)) {
      const targetCardBtn = step2Id
        ? Array.from(document.querySelectorAll('#profiles-list .account-card'))
          .find((el) => el.dataset.profileId === step2Id)
          ?.querySelector('.btn-primary')
        : null;
      const fallbackBtn = document.getElementById('btn-open-create');
      if (targetCardBtn && !targetCardBtn.disabled && !needsFocusRestore(targetCardBtn)) {
        targetCardBtn.focus();
      } else if (fallbackBtn && !needsFocusRestore(fallbackBtn)) {
        fallbackBtn.focus();
      }
    }
  });

  const renameDialog = document.getElementById('rename-profile-dialog');
  renameDialog?.addEventListener('close', async () => {
    const action = renameDialog.returnValue;
    renameDialog.returnValue = '';
    state.openDialogContext = null;
    const targetId = state.activeRenameId;
    const targetKind = state.activeRenameKind;
    state.activeRenameId = null;
    if (action !== 'rename' || !targetId || state.pendingProfileActions.has(targetId)) return;

    const newName = document.getElementById('rename-profile-name').value.trim();
    if (!newName) return;

    const chosenColor = document.querySelector('input[name="rename-account-color"]:checked')?.value;
    if (chosenColor) {
      saveAccountColor(targetId, chosenColor);
    }

    state.pendingProfileActions.set(targetId, 'rename');
    renderProfiles();
    renderAccountRail();
    try {
      let res;
      if (targetKind === 'web_isolated') {
        res = await tauriInvokeEnvelope('web_session_rename', { id: targetId, displayName: newName });
      } else {
        res = await apiCall(`/api/profiles/${encodeURIComponent(targetId)}/rename`, 'POST', { newName }, 'rename_profile', { id: targetId, newName });
      }
      if (res && res.ok === false) showErrorBanner(res.error_code, res.message);
      else showKeyBanner('messages.accountRenamed', {}, 'success');
      await refreshAll();
    } catch (err) {
      showErrorBanner(err?.errorCode || 'HOST_EXEC_FAILED', err?.message || '');
    } finally {
      state.pendingProfileActions.delete(targetId);
      renderProfiles();
      renderAccountRail();
      renderActiveStage();
    }
  });

  const deleteDialog = document.getElementById('delete-profile-dialog');
  deleteDialog?.addEventListener('close', async () => {
    const action = deleteDialog.returnValue;
    deleteDialog.returnValue = '';
    state.openDialogContext = null;
    const targetId = state.activeDeleteId;
    const targetKind = state.activeDeleteKind;
    state.activeDeleteId = null;
    if (action !== 'delete' || !targetId || state.pendingProfileActions.has(targetId)) {
      removeOverlayBlocker('dialog:delete-profile-dialog');
      return;
    }

    state.pendingProfileActions.set(targetId, 'delete');
    renderProfiles();
    renderAccountRail();
    try {
      let res;
      if (targetKind === 'web_isolated') {
        if (state.mountedWebSessionId === targetId) {
          state.mountedWebSessionId = null;
          state.lastWebBoundsKey = '';
        }
        res = await tauriInvokeEnvelope('web_session_delete', { id: targetId });
      } else {
        res = await apiCall(
          `/api/profiles/${encodeURIComponent(targetId)}`,
          'DELETE',
          { confirm: true },
          'delete_profile',
          { id: targetId, confirm: true }
        );
      }
      if (res && res.ok === false) {
        showErrorBanner(res.error_code, res.message);
      } else {
        clearProfileError(targetId);
        removeAccountColor(targetId);
        if (state.activeAccount?.id === targetId && state.activeAccount?.kind === targetKind) {
          state.activeAccount = null;
          selectView('profiles');
        }
        showKeyBanner('messages.accountDeleted', {}, 'success');
      }
      await refreshAll();
    } catch (err) {
      showErrorBanner(err?.errorCode || 'HOST_EXEC_FAILED', err?.message || '');
    } finally {
      state.pendingProfileActions.delete(targetId);
      renderProfiles();
      renderAccountRail();
      removeOverlayBlocker('dialog:delete-profile-dialog');
      if (needsFocusRestore(document.activeElement)) {
        const openCreateBtn = document.getElementById('btn-open-create');
        if (openCreateBtn && !needsFocusRestore(openCreateBtn)) {
          openCreateBtn.focus();
        }
      }
    }
  });

  const resetWebDialog = document.getElementById('reset-web-dialog');
  resetWebDialog?.addEventListener('close', async () => {
    const action = resetWebDialog.returnValue;
    resetWebDialog.returnValue = '';
    state.openDialogContext = null;
    const targetId = state.activeResetWebId;
    state.activeResetWebId = null;
    if (action !== 'reset' || !targetId) {
      removeOverlayBlocker('dialog:reset-web-dialog');
      return;
    }

    state.mountedWebSessionId = null;
    state.lastWebBoundsKey = '';
    try {
      const env = await tauriInvokeEnvelope('web_session_reset', { id: targetId }).catch((err) => ({
        ok: false,
        error_code: err?.errorCode || 'WEB_SESSION_ERROR',
        message: err?.message || ''
      }));
      if (env && env.ok === false) {
        showErrorBanner(env.error_code, env.message);
      } else {
        await refreshWorkspaceFast();
      }
    } finally {
      removeOverlayBlocker('dialog:reset-web-dialog');
    }
  });

  const restoreDialog = document.getElementById('restore-backup-dialog');
  restoreDialog?.addEventListener('close', async () => {
    const action = restoreDialog.returnValue;
    restoreDialog.returnValue = '';
    state.openDialogContext = null;
    const backupDir = state.activeRestoreBackupDir;
    const sourceId = state.activeRestoreSourceId;
    state.activeRestoreBackupDir = null;
    state.activeRestoreSourceId = null;
    if (action !== 'restore' || !backupDir || state.pendingGlobalActions.has('restore')) return;

    const preRestoreIds = new Set((state.status?.profiles || []).map((p) => p.id));
    const name = document.getElementById('restore-profile-name').value.trim();
    state.pendingGlobalActions.add('restore');
    updatePendingControls();
    renderBackups();
    try {
      const res = await apiCall('/api/backups/restore', 'POST', { backupDir, name }, 'restore_backup', { backupDir, name });
      if (res && res.ok === false) {
        const restoreErrCode = res.error_code === 'BACKUP_ERROR' ? 'RESTORE_ERROR' : res.error_code;
        showErrorBanner(restoreErrCode, res.message);
        await refreshAll();
      } else {
        showKeyBanner('messages.backupRestored', {}, 'success');
        await refreshAll();
        const restoredProfile = (state.status?.profiles || []).find((p) => !preRestoreIds.has(p.id));
        const restoredId = res?.data?.id || restoredProfile?.id || null;
        if (restoredId && sourceId) {
          saveAccountColor(restoredId, getAccountColorId({ id: sourceId }));
          renderProfiles();
          renderAccountRail();
        }
      }
    } catch (err) {
      showErrorBanner(err?.errorCode || 'HOST_EXEC_FAILED', err?.message || '');
    } finally {
      state.pendingGlobalActions.delete('restore');
      updatePendingControls();
      renderBackups();
    }
  });

  document.getElementById('profile-details-dialog')?.addEventListener('close', (e) => {
    e.currentTarget.returnValue = '';
    state.openDialogContext = null;
  });

  const qsDialog = document.getElementById('quick-switcher-dialog');
  const qsInput = document.getElementById('quick-switcher-input');
  qsDialog?.addEventListener('close', () => {
    qsDialog.returnValue = '';
  });
  qsInput?.addEventListener('input', (e) => {
    state.switcherQuery = e.target.value;
    state.switcherActiveIndex = 0;
    renderQuickSwitcherList();
  });
  qsInput?.addEventListener('keydown', (event) => {
    const items = getFilteredSwitcherAccounts();
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (items.length > 0) {
        state.switcherActiveIndex = (state.switcherActiveIndex + 1) % items.length;
        renderQuickSwitcherList();
      }
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (items.length > 0) {
        state.switcherActiveIndex = (state.switcherActiveIndex - 1 + items.length) % items.length;
        renderQuickSwitcherList();
      }
    } else if (event.key === 'Home') {
      event.preventDefault();
      state.switcherActiveIndex = 0;
      renderQuickSwitcherList();
    } else if (event.key === 'End') {
      event.preventDefault();
      state.switcherActiveIndex = Math.max(0, items.length - 1);
      renderQuickSwitcherList();
    } else if (event.key === 'Enter') {
      event.preventDefault();
      if (event.isComposing || event.keyCode === 229) return;
      const chosen = items[state.switcherActiveIndex];
      if (chosen) {
        qsDialog?.close('select');
        activateAccount(chosen.id, chosen.kind);
      }
    } else if (event.key === 'Escape' && qsInput.value) {
      event.preventDefault();
      event.stopPropagation();
      qsInput.value = '';
      state.switcherQuery = '';
      state.switcherActiveIndex = 0;
      renderQuickSwitcherList();
    }
  });
}

function wireWebHeaderControls() {
  const webInfoBtn = document.getElementById('btn-web-info');
  const webLimitsTip = document.getElementById('web-limits-tip');
  const webMenu = document.getElementById('web-session-menu');
  const webMoreBtn = document.getElementById('btn-web-more');

  const probe = typeof document !== 'undefined' ? document.createElement('div') : null;
  if (probe) {
    probe.popover = 'hint';
    if (probe.popover !== 'hint') {
      document.addEventListener('beforetoggle', ({ target, newState }) => {
        if (target?.getAttribute?.('popover') === 'hint'
          && document.querySelector(`[popovertarget="${CSS.escape(target.id)}"]`)
          && newState === 'open') {
          target.popover = 'auto';
        }
      }, true);
    }
  }

  document.addEventListener('beforetoggle', (event) => {
    const target = event.target;
    if (!target || target.id !== 'web-limits-tip') return;
    if (event.newState === 'open') {
      addOverlayBlocker('popover:web-limits-tip');
    }
  }, true);

  document.addEventListener('toggle', (event) => {
    const target = event.target;
    if (!target || target.id !== 'web-limits-tip') return;
    const isOpen = event.newState === 'open';
    webInfoBtn?.setAttribute('aria-expanded', String(isOpen));
    if (isOpen) addOverlayBlocker('popover:web-limits-tip');
    else removeOverlayBlocker('popover:web-limits-tip');
  }, true);

  if (webMenu && webMoreBtn) {
    webMenu.addEventListener('beforetoggle', (event) => {
      if (event.newState === 'open') {
        addOverlayBlocker('popover:web-session-menu');
      }
    });

    webMenu.addEventListener('toggle', (event) => {
      const isOpen = event.newState === 'open';
      webMoreBtn.setAttribute('aria-expanded', String(isOpen));
      if (isOpen) {
        addOverlayBlocker('popover:web-session-menu');
        if (!CSS.supports('position-area', 'block-end')) {
          positionMenu(webMoreBtn, webMenu);
        }
      } else {
        removeOverlayBlocker('popover:web-session-menu');
      }
    });

    if (typeof HTMLButtonElement === 'undefined' || !('commandForElement' in HTMLButtonElement.prototype)) {
      webMoreBtn.addEventListener('click', () => {
        if (webMenu.matches(':popover-open')) webMenu.hidePopover();
        else webMenu.showPopover();
      });
    }
  }

  document.getElementById('btn-web-reload')?.addEventListener('click', async () => {
    if (!state.activeAccount || state.activeAccount.kind !== 'web_isolated') return;
    try {
      const env = await tauriInvokeEnvelope('web_session_reload', { id: state.activeAccount.id });
      if (!env || env.ok === false) {
        showErrorBanner(env?.error_code || 'WEB_SESSION_ERROR', env?.message || '');
      }
    } catch (err) {
      showErrorBanner(err?.errorCode || 'WEB_SESSION_ERROR', err?.message || '');
    }
  });

  document.getElementById('btn-web-pin')?.addEventListener('click', () => {
    webMenu?.hidePopover?.();
    if (!state.activeAccount) return;
    const acc = (state.workspace?.accounts || []).find((a) => a.id === state.activeAccount.id);
    toggleAccountPinned(state.activeAccount.id, Boolean(acc?.pinned));
  });

  document.getElementById('btn-web-rename')?.addEventListener('click', () => {
    webMenu?.hidePopover?.();
    if (!state.activeAccount) return;
    const acc = (state.workspace?.accounts || []).find((a) => a.id === state.activeAccount.id);
    if (acc) openRenameDialog(acc, 'web_isolated');
  });

  document.getElementById('btn-web-reset')?.addEventListener('click', () => {
    webMenu?.hidePopover?.();
    if (!state.activeAccount) return;
    const acc = (state.workspace?.accounts || []).find((a) => a.id === state.activeAccount.id);
    if (acc) openResetWebDialog(acc);
  });

  document.getElementById('btn-web-delete')?.addEventListener('click', () => {
    webMenu?.hidePopover?.();
    if (!state.activeAccount) return;
    const acc = (state.workspace?.accounts || []).find((a) => a.id === state.activeAccount.id);
    if (acc) openDeleteDialog(acc, 'web_isolated');
  });
}

function wireDesktopStageControls() {
  document.getElementById('btn-desktop-activate')?.addEventListener('click', () => {
    if (!state.activeAccount) return;
    activateAccount(state.activeAccount.id, state.activeAccount.kind);
  });

  document.getElementById('btn-desktop-stop')?.addEventListener('click', () => {
    if (!state.activeAccount || state.activeAccount.kind !== 'desktop_isolated') return;
    const prof = (state.status?.profiles || []).find((p) => p.id === state.activeAccount.id);
    if (prof) runProfileAction('stop', prof);
  });

  document.getElementById('toggle-companion-snap')?.addEventListener('change', async (event) => {
    if (!state.activeAccount || state.activeAccount.kind !== 'desktop_isolated') {
      event.target.checked = false;
      return;
    }
    const enabled = Boolean(event.target.checked);
    const railRect = document.getElementById('account-rail')?.getBoundingClientRect();
    const dpr = typeof window !== 'undefined' && window.devicePixelRatio ? window.devicePixelRatio : 1;
    const railWidthPx = railRect ? Math.round(railRect.width * dpr) : 68;
    event.target.disabled = true;
    try {
      const env = await tauriInvokeEnvelope('set_companion_snap', {
        id: state.activeAccount.id,
        enabled,
        railWidthPx
      });
      if (env && env.ok === false) {
        event.target.checked = !enabled;
        showErrorBanner(env.error_code, env.message);
      } else {
        await refreshWorkspaceFast();
      }
    } catch (err) {
      event.target.checked = !enabled;
      showErrorBanner(err?.errorCode || 'COMPANION_SNAP_UNSUPPORTED', err?.message || '');
    } finally {
      event.target.disabled = false;
    }
  });

  document.getElementById('btn-desktop-pin')?.addEventListener('click', () => {
    if (!state.activeAccount || state.activeAccount.kind !== 'desktop_isolated') return;
    const acc = (state.workspace?.accounts || []).find((a) => a.id === state.activeAccount.id);
    toggleAccountPinned(state.activeAccount.id, Boolean(acc?.pinned));
  });

  document.getElementById('btn-desktop-rename')?.addEventListener('click', () => {
    if (!state.activeAccount) return;
    const prof = (state.status?.profiles || []).find((p) => p.id === state.activeAccount.id);
    if (prof) openRenameDialog(prof, 'desktop_isolated');
  });

  document.getElementById('btn-desktop-backup')?.addEventListener('click', () => {
    if (!state.activeAccount) return;
    const prof = (state.status?.profiles || []).find((p) => p.id === state.activeAccount.id);
    if (prof) runProfileAction('backup', prof);
  });

  document.getElementById('btn-desktop-details')?.addEventListener('click', () => {
    if (!state.activeAccount) return;
    const prof = (state.status?.profiles || []).find((p) => p.id === state.activeAccount.id);
    if (prof) openDetailsDialog(prof);
  });

  document.getElementById('btn-desktop-delete')?.addEventListener('click', () => {
    if (!state.activeAccount) return;
    const prof = (state.status?.profiles || []).find((p) => p.id === state.activeAccount.id);
    if (prof) openDeleteDialog(prof, 'desktop_isolated');
  });
}

function wireBoundsObservers() {
  const viewport = document.getElementById('web-session-viewport');
  if (!viewport || typeof ResizeObserver === 'undefined') return;
  let rafId = 0;
  const scheduleSync = () => {
    if (rafId) cancelAnimationFrame(rafId);
    rafId = requestAnimationFrame(() => {
      rafId = 0;
      if (isWebViewStageActive() && state.overlayBlockers.size === 0) {
        syncNativeWebViewState();
      }
    });
  };
  const ro = new ResizeObserver(scheduleSync);
  ro.observe(viewport);
  const alertsEl = document.getElementById('workspace-alerts');
  if (alertsEl) ro.observe(alertsEl);

  window.addEventListener('resize', scheduleSync);
  window.addEventListener('focus', scheduleSync);
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      syncNativeWebViewState({ forceHide: true });
    } else {
      scheduleSync();
    }
  });
  if (window.visualViewport) {
    window.visualViewport.addEventListener('resize', scheduleSync);
  }
  if (typeof window.matchMedia === 'function') {
    let dprMql = null;
    const onDprChange = () => {
      scheduleSync();
      bindDprMedia();
    };
    const bindDprMedia = () => {
      if (dprMql) {
        dprMql.removeEventListener('change', onDprChange);
      }
      const dpr = window.devicePixelRatio || 1;
      dprMql = window.matchMedia(`(resolution: ${dpr}dppx)`);
      dprMql.addEventListener('change', onDprChange);
    };
    bindDprMedia();
  }
}

function wireGlobalShortcuts() {
  if (typeof document === 'undefined') return;
  document.addEventListener('keydown', (event) => {
    if (document.querySelector('dialog[open]')) return;
    const mod = event.ctrlKey || event.metaKey;
    if (mod && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      openQuickSwitcher();
      return;
    }
    if (mod && event.key.toLowerCase() === 'b') {
      event.preventDefault();
      document.getElementById('btn-rail-toggle')?.click();
      return;
    }
    if (mod && event.key.toLowerCase() === 'n') {
      event.preventDefault();
      document.getElementById('btn-open-create')?.click();
      return;
    }
    if (mod && event.key === ',') {
      event.preventDefault();
      selectView('settings');
      return;
    }
    if (mod && /^[1-9]$/.test(event.key)) {
      const idx = Number(event.key) - 1;
      const items = getAllUnifiedAccounts();
      if (items[idx]) {
        event.preventDefault();
        activateAccount(items[idx].id, items[idx].kind);
      }
      return;
    }
    if (event.key === '/' && !mod && !event.altKey) {
      const active = document.activeElement;
      const inInput = active && active.matches?.('input, textarea, select, [contenteditable="true"]');
      if (!inInput) {
        event.preventDefault();
        const railFilter = document.getElementById('rail-filter-input');
        const shell = document.getElementById('workspace-shell');
        if (shell?.dataset.railExpanded === 'true' && railFilter) {
          railFilter.focus();
        } else if (state.activeView === 'profiles') {
          document.getElementById('profile-search')?.focus();
        } else {
          openQuickSwitcher();
        }
      }
    }
  });
}

export async function initApp() {
  if (state.initialized) return;
  state.initialized = true;
  await initI18n();
  applyDomTranslations();
  applyThemePreference(getSavedThemePreference());

  const shell = document.getElementById('workspace-shell');
  const railToggle = document.getElementById('btn-rail-toggle');
  try {
    if (typeof localStorage !== 'undefined' && localStorage.getItem(RAIL_EXPANDED_KEY) === 'true' && shell) {
      shell.dataset.railExpanded = 'true';
      railToggle?.setAttribute('aria-pressed', 'true');
    }
  } catch (_err) {
    // Ignore
  }

  railToggle?.addEventListener('click', () => {
    if (!shell) return;
    const expanded = shell.dataset.railExpanded === 'true';
    const next = !expanded;
    shell.dataset.railExpanded = String(next);
    railToggle.setAttribute('aria-pressed', String(next));
    railToggle.setAttribute('aria-label', next ? t('rail.collapse') : t('rail.expand'));
    railToggle.setAttribute('title', next ? t('rail.collapse') : t('rail.expand'));
    try {
      if (typeof localStorage !== 'undefined') localStorage.setItem(RAIL_EXPANDED_KEY, String(next));
    } catch (_err) {
      // Ignore
    }
  });

  renderCreateColorPalette();
  renderRenameColorPalette();
  updateLanguageControls();
  updateAppearanceControls();
  updateDocumentTitle(state.activeView);
  wireDialogsAndFallbacks();
  wireWebHeaderControls();
  wireDesktopStageControls();
  wireBoundsObservers();
  wireGlobalShortcuts();

  document.getElementById('btn-brand-home')?.addEventListener('click', () => {
    selectView('profiles');
  });

  for (const btn of document.querySelectorAll('.top-nav-btn[data-view]')) {
    btn.addEventListener('click', () => {
      selectView(btn.getAttribute('data-view'));
    });
  }

  for (const btn of document.querySelectorAll('.back-btn[data-back-to]')) {
    btn.addEventListener('click', () => {
      const targetView = btn.getAttribute('data-back-to') || 'profiles';
      const triggerEl = state.lastNavTriggerEl;
      state.lastNavTriggerEl = null;
      selectView(targetView);
      if (triggerEl && triggerEl.isConnected && !triggerEl.closest?.('[hidden]')) {
        triggerEl.focus();
      } else if (targetView === 'profiles') {
        const cardBtn = state.activeAccount
          ? document.querySelector(`#profiles-list .account-card[data-profile-id="${CSS.escape(state.activeAccount.id)}"] .btn-primary`)
          : null;
        if (cardBtn && !cardBtn.disabled) cardBtn.focus();
        else document.getElementById('btn-overview-create')?.focus();
      }
    });
  }

  document.getElementById('btn-goto-diagnostics')?.addEventListener('click', (event) => {
    selectView('diagnostics', { triggerEl: event.currentTarget, returnView: 'settings' });
  });

  document.getElementById('btn-help-diagnostics')?.addEventListener('click', (event) => {
    selectView('diagnostics', { triggerEl: event.currentTarget, returnView: 'help' });
  });

  document.getElementById('btn-copy-diagnostics')?.addEventListener('click', async () => {
    const installStateLabel = state.status?.installation?.valid
      ? t('diagnostics.summary.copyDetected')
      : t('diagnostics.summary.copyMissing');
    const lines = [
      t('diagnostics.summary.copyHeader', { appName: t('app.name') }),
      `${t('diagnostics.summary.copyPlatform')}: ${state.status?.platform || 'unknown'} | ${t('diagnostics.summary.copyVersion')}: ${state.status?.app_version || UI_VERSION}`,
      `${t('diagnostics.summary.copyDataDir')}: ${state.status?.data_dir || '—'}`,
      `${t('diagnostics.summary.copyInstall')}: ${installStateLabel} (${state.status?.installation?.version || '—'})`,
      `${t('diagnostics.summary.copyChecks')}:`,
      ...(state.isolation?.checks || []).map((c) => `  - ${c.check_name}: ${resolveIsolationCheckResult(c).raw} (${c.detail || ''})`),
      `${t('diagnostics.summary.copyLogs')}:`,
      ...(state.diagnostics?.sanitized_logs || [])
    ];
    try {
      if (!navigator.clipboard?.writeText) {
        showToast('errors.UNKNOWN', {}, 'error', 'diag-copy-err');
        return;
      }
      await navigator.clipboard.writeText(lines.join('\n'));
      showToast('messages.diagnosticsCopied', {}, 'success', 'diag-copy');
    } catch (_err) {
      showToast('errors.UNKNOWN', {}, 'error', 'diag-copy-err');
    }
  });

  const railFilterInput = document.getElementById('rail-filter-input');
  railFilterInput?.addEventListener('input', (event) => {
    state.railFilterQuery = event.target.value;
    renderAccountRail();
  });
  railFilterInput?.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && railFilterInput.value) {
      event.preventDefault();
      railFilterInput.value = '';
      state.railFilterQuery = '';
      renderAccountRail();
    }
  });

  const searchInput = document.getElementById('profile-search');
  searchInput?.addEventListener('input', (event) => {
    state.searchQuery = event.target.value;
    renderProfiles();
  });
  searchInput?.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && searchInput.value) {
      event.preventDefault();
      searchInput.value = '';
      state.searchQuery = '';
      renderProfiles();
    }
  });

  document.getElementById('btn-refresh')?.addEventListener('click', () => refreshAll());
  document.getElementById('btn-view-grid')?.addEventListener('click', () => {
    const list = document.getElementById('profiles-list');
    if (list) list.dataset.viewMode = 'grid';
    try { localStorage.setItem('khoang.viewMode', 'grid'); } catch (_err) {}
    renderProfiles();
  });
  document.getElementById('btn-view-list')?.addEventListener('click', () => {
    const list = document.getElementById('profiles-list');
    if (list) list.dataset.viewMode = 'list';
    try { localStorage.setItem('khoang.viewMode', 'list'); } catch (_err) {}
    renderProfiles();
  });
  document.getElementById('btn-stop-all')?.addEventListener('click', async () => {
    if (state.pendingGlobalActions.has('stop-all')) return;
    state.pendingGlobalActions.add('stop-all');
    updatePendingControls();
    try {
      const invoke = getTauriInvoke();
      if (invoke) {
        const res = await invoke('stop_all_profiles', {});
        if (res && res.ok === false) {
          const err = new Error(res.message || t('errors.UNKNOWN'));
          err.errorCode = res.error_code || 'UNKNOWN';
          throw err;
        }
      } else {
        const res = await apiCall('/api/profiles/stop-all', 'POST', {});
        if (res && res.ok === false) {
          const err = new Error(res.message || t('errors.UNKNOWN'));
          err.errorCode = res.error_code || 'UNKNOWN';
          throw err;
        }
      }
      await refreshAll();
    } catch (err) {
      showErrorBanner(err.errorCode || 'UNKNOWN', err.message || String(err));
    } finally {
      state.pendingGlobalActions.delete('stop-all');
      updatePendingControls();
    }
  });
  document.getElementById('btn-refresh-backups')?.addEventListener('click', () => refreshAll());
  document.getElementById('btn-run-isolation')?.addEventListener('click', () => refreshAll({ isVerify: true }));
  document.getElementById('btn-refresh-diagnostics')?.addEventListener('click', () => refreshAll());
  document.getElementById('toggle-sizes')?.addEventListener('change', () => refreshAll());

  for (const input of document.querySelectorAll('input[name="ui-locale"]')) {
    input.addEventListener('change', (event) => {
      if (event.target.checked) {
        setLocale(event.target.value);
      }
    });
  }

  onLocaleChange((nextLocale) => {
    applyDomTranslations();
    renderAll();
    const invoke = getTauriInvoke();
    if (invoke) {
      invoke('set_tray_locale', { locale: nextLocale }).catch(() => {});
    }
  });

  const keepInTrayEl = document.getElementById('toggle-keep-in-tray');
  const autostartEl = document.getElementById('toggle-autostart');
  const behaviorHintEl = document.getElementById('settings-behavior-hint');
  const tauriInvoke = getTauriInvoke();
  if (tauriInvoke) {
    if (behaviorHintEl) behaviorHintEl.hidden = true;
    tauriInvoke('get_desktop_preferences', {})
      .then((prefs) => {
        if (keepInTrayEl && prefs && typeof prefs.keep_in_tray === 'boolean') {
          keepInTrayEl.checked = prefs.keep_in_tray;
        }
        let savedLocalLocale = null;
        try {
          if (typeof localStorage !== 'undefined') {
            savedLocalLocale = localStorage.getItem(LOCALE_STORAGE_KEY);
          }
        } catch (_err) {
          // Ignore storage access errors
        }
        const backendLocale = prefs?.locale;
        if (!savedLocalLocale && backendLocale && SUPPORTED_LOCALES.includes(backendLocale) && backendLocale !== getLocale()) {
          setLocale(backendLocale);
        } else if (backendLocale !== getLocale()) {
          tauriInvoke('set_tray_locale', { locale: getLocale() }).catch(() => {});
        }
      })
      .catch(() => {
        tauriInvoke('set_tray_locale', { locale: getLocale() }).catch(() => {});
      });
    tauriInvoke('get_autostart_enabled', {})
      .then((enabled) => {
        if (autostartEl) autostartEl.checked = Boolean(enabled);
      })
      .catch(() => {});
    keepInTrayEl?.addEventListener('change', async (event) => {
      const next = Boolean(event.target.checked);
      keepInTrayEl.disabled = true;
      try {
        await tauriInvoke('set_keep_in_tray', { enabled: next });
      } catch (err) {
        keepInTrayEl.checked = !next;
        showErrorBanner(err.errorCode || 'UNKNOWN', err.message || String(err));
      } finally {
        keepInTrayEl.disabled = false;
      }
    });
    autostartEl?.addEventListener('change', async (event) => {
      const next = Boolean(event.target.checked);
      autostartEl.disabled = true;
      try {
        await tauriInvoke('set_autostart_enabled', { enabled: next });
      } catch (err) {
        autostartEl.checked = !next;
        showErrorBanner(err.errorCode || 'UNKNOWN', err.message || String(err));
      } finally {
        autostartEl.disabled = false;
      }
    });
    const tauriListen = typeof window !== 'undefined' ? window.__TAURI__?.event?.listen : null;
    if (typeof tauriListen === 'function') {
      tauriListen('khoang://navigate', (event) => {
        const target = event?.payload;
        if (target === 'profiles' || target === 'settings' || target === 'backups' || target === 'help' || target === 'diagnostics') {
          selectView(target);
        }
      }).catch(() => {});
      tauriListen('khoang://activate-request', (event) => {
        const payload = event?.payload;
        if (payload?.id && payload?.kind) {
          activateAccount(payload.id, payload.kind);
        }
      }).catch(() => {});
      tauriListen('khoang://snapshot-changed', () => {
        refreshWorkspaceFast();
      }).catch(() => {});
    }
  } else {
    if (keepInTrayEl) keepInTrayEl.disabled = true;
    if (autostartEl) autostartEl.disabled = true;
    if (behaviorHintEl) behaviorHintEl.hidden = false;
  }

  for (const input of document.querySelectorAll('input[name="ui-theme"]')) {
    input.addEventListener('change', (event) => {
      if (event.target.checked) {
        applyThemePreference(event.target.value);
      }
    });
  }

  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      updateAppearanceControls();
    });
  }

  if (typeof window !== 'undefined') {
    window.addEventListener('storage', (event) => {
      if (event.key === THEME_STORAGE_KEY) {
        applyThemePreference(getSavedThemePreference());
      }
    });
  }

  await refreshAll();
}

if (typeof document !== 'undefined') {
  initApp();
}
