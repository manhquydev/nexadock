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
  applyDomTranslations
} from './i18n/index.js';

const UI_VERSION = '0.1.0';
const THEME_STORAGE_KEY = 'multi-zalo.theme';
const ACCOUNT_COLORS_STORAGE_KEY = 'multi-zalo.accountColors';

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
  backups: [],
  diagnostics: null,
  isolation: null,
  activeView: 'profiles',
  searchQuery: '',
  connectionMode: null, // 'tauri' | 'bridge' | null
  connectionOk: false,
  activeRenameId: null,
  activeDeleteId: null,
  activeRestoreBackupDir: null,
  activeRestoreSourceId: null,
  createdAccountForStep2: null,
  openDialogContext: null,
  openMenuProfileId: null,
  refreshSeq: 0,
  banner: null,
  pendingProfileActions: new Map(),
  pendingGlobalActions: new Set()
};

export function getTauriInvoke() {
  if (typeof window === 'undefined') return null;
  if (window.__TAURI__?.core?.invoke) return window.__TAURI__.core.invoke;
  if (window.__TAURI_INTERNALS__?.invoke) return window.__TAURI_INTERNALS__.invoke;
  return null;
}

function loadAccountColorsMap() {
  try {
    if (typeof localStorage !== 'undefined') {
      const raw = localStorage.getItem(ACCOUNT_COLORS_STORAGE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw);
        if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
          return parsed;
        }
      }
    }
  } catch (_err) {
    // Ignore storage errors
  }
  return {};
}

export function saveAccountColor(profileId, colorId) {
  if (!profileId || !ACCOUNT_COLORS.some((c) => c.id === colorId)) return;
  try {
    if (typeof localStorage !== 'undefined') {
      const map = loadAccountColorsMap();
      map[profileId] = colorId;
      localStorage.setItem(ACCOUNT_COLORS_STORAGE_KEY, JSON.stringify(map));
    }
  } catch (_err) {
    // Ignore storage errors
  }
}

export function removeAccountColor(profileId) {
  if (!profileId) return;
  try {
    if (typeof localStorage !== 'undefined') {
      const map = loadAccountColorsMap();
      if (Object.hasOwn(map, profileId)) {
        delete map[profileId];
        localStorage.setItem(ACCOUNT_COLORS_STORAGE_KEY, JSON.stringify(map));
      }
    }
  } catch (_err) {
    // Ignore storage errors
  }
}

export function getAccountColorId(prof) {
  const map = loadAccountColorsMap();
  const saved = prof?.id ? map[prof.id] : null;
  const matched = ACCOUNT_COLORS.find((c) => c.id === saved);
  if (matched) return matched.id;

  const seed = String(prof?.id || prof?.name || 'z');
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

function renderBanner() {
  const banner = document.getElementById('status-banner');
  const textEl = document.getElementById('status-banner-text');
  const detailsEl = document.getElementById('status-banner-details');
  const rawEl = document.getElementById('status-banner-raw');
  if (!banner || !textEl || !detailsEl || !rawEl) return;

  if (!state.banner) {
    banner.hidden = true;
    banner.className = 'banner';
    textEl.textContent = '';
    detailsEl.hidden = true;
    detailsEl.open = false;
    rawEl.textContent = '';
    return;
  }

  banner.className = `banner banner-${state.banner.kind}`;
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

function showKeyBanner(messageKey, messageParams = {}, kind = 'info') {
  state.banner = { kind, messageKey, messageParams };
  renderBanner();
}

function showErrorBanner(errorCode, rawDetail = '') {
  state.banner = {
    kind: 'error',
    errorCode: errorCode || 'UNKNOWN',
    rawDetail: rawDetail ? String(rawDetail) : ''
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

  const btnEmptyCreate = document.getElementById('btn-empty-create');
  if (btnEmptyCreate) btnEmptyCreate.disabled = isCreating;
}

function updateDocumentTitle(viewName = state.activeView) {
  if (typeof document === 'undefined') return;
  const viewTitles = {
    profiles: t('nav.accounts'),
    backups: t('nav.backups'),
    settings: t('nav.settings'),
    diagnostics: t('diagnostics.title'),
    help: t('help.title')
  };
  const viewTitle = viewTitles[viewName] || t('nav.accounts');
  document.title = `${t('app.name')} — ${viewTitle}`;
}

function selectView(viewName) {
  state.activeView = viewName;
  if (state.banner && state.banner.kind !== 'error') {
    clearBanner();
  }
  for (const btn of document.querySelectorAll('.top-nav-btn[data-view]')) {
    const active = btn.getAttribute('data-view') === viewName;
    btn.classList.toggle('is-active', active);
    if (active) btn.setAttribute('aria-current', 'page');
    else btn.removeAttribute('aria-current');
  }
  for (const view of document.querySelectorAll('.view')) {
    view.hidden = view.id !== `view-${viewName}`;
  }
  const btnOpenCreate = document.getElementById('btn-open-create');
  if (btnOpenCreate) {
    btnOpenCreate.hidden = viewName !== 'profiles';
  }
  updateDocumentTitle(viewName);
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
  if (!prof || prof.lifecycle_state == null) return '';
  return String(prof.lifecycle_state)
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
  if (pendingAction === 'launch') return { label: t('accounts.states.starting'), kind: 'pending' };
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
    const runningCount = (state.status.profiles || []).filter((p) => p.running).length;
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
    if (supportsPopover) {
      if (isOpen()) {
        try {
          menuList.hidePopover();
        } catch (_err) {
          // Ignore if already closed
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
        positionMenu(moreBtn, menuList);
      } else if (state.openMenuProfileId === prof.id) {
        state.openMenuProfileId = null;
      }
    });
  } else {
    const onDocClick = (event) => {
      if (!menuWrap.isConnected) {
        document.removeEventListener('click', onDocClick);
        return;
      }
      if (isOpen() && !menuWrap.contains(event.target)) {
        closeMenu({ restoreFocus: false });
      }
    };
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
      const next = currentIdx === -1 ? 0 : (currentIdx + 1) % items.length;
      items[next].focus();
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      const prev = currentIdx <= 0 ? items.length - 1 : currentIdx - 1;
      items[prev].focus();
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
  addMenuItem(t('accounts.actions.rename'), () => openRenameDialog(prof));
  addMenuItem(
    t('accounts.actions.backup'),
    () => runProfileAction('backup', prof),
    { disabled: blockOfflineOps, hint: t('accounts.actions.closeFirstHint') }
  );
  addMenuItem(t('accounts.actions.details'), () => openDetailsDialog(prof));
  addMenuItem(
    t('accounts.actions.delete'),
    () => openDeleteDialog(prof),
    { disabled: blockOfflineOps, danger: true, hint: t('accounts.actions.closeFirstHint') }
  );

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
  const searchWrap = document.getElementById('search-wrap');
  const countSummary = document.getElementById('accounts-count-summary');
  if (!list || !emptyEl) return;

  const reopenMenuId = state.openMenuProfileId;
  const focusWasInMenu = Boolean(document.activeElement?.closest?.('.overflow-menu'));

  list.replaceChildren();
  const allProfiles = state.status?.profiles || [];
  const filtered = filterProfiles(allProfiles, state.searchQuery);
  const hasQuery = state.searchQuery.trim().length > 0;

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
    const isPending = state.pendingProfileActions.has(prof.id);
    const accState = resolveAccountState(prof);

    const li = document.createElement('li');
    li.className = prof.running ? 'account-card is-running' : 'account-card';
    li.dataset.profileId = prof.id;

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
    identity.appendChild(nameEl);

    const stateRow = document.createElement('div');
    stateRow.className = `account-state-row is-${accState.kind}`;
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

    const lcState = normalizeLifecycleState(prof);
    const isActiveLifecycle = Boolean(prof.running) || lcState === 'starting' || lcState === 'running' || lcState === 'stopping';
    if (isActiveLifecycle) {
      primaryActions.appendChild(makeButton(
        t('accounts.actions.focus'),
        'btn-primary',
        () => runProfileAction('focus', prof),
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
        t('accounts.actions.launch'),
        'btn-primary',
        () => runProfileAction('launch', prof),
        { disabled: isPending, ariaLabel: t('accounts.actions.launchAria', { name: prof.name }) }
      ));
    }

    secondaryActions.appendChild(buildOverflowMenu(prof, isPending));

    actions.appendChild(primaryActions);
    actions.appendChild(secondaryActions);

    li.appendChild(head);
    li.appendChild(actions);
    list.appendChild(li);
  }

  if (reopenMenuId) {
    const row = Array.from(list.querySelectorAll('.account-card')).find(
      (el) => el.dataset.profileId === reopenMenuId
    );
    const trigger = row?.querySelector('button[aria-haspopup="menu"]');
    const menu = row?.querySelector('.overflow-menu');
    if (trigger && menu && !trigger.disabled) {
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
      if (focusWasInMenu) {
        menu.querySelector('[role="menuitem"]:not(:disabled)')?.focus();
      }
    } else {
      state.openMenuProfileId = null;
    }
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
  if (!appList || !secList || !profList || !envList) return;

  appList.replaceChildren();
  secList.replaceChildren();
  profList.replaceChildren();
  envList.replaceChildren();

  const inst = state.status?.installation;
  const diag = state.diagnostics;
  const primary = state.status?.primary_zalo;
  const profiles = state.status?.profiles || [];

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
          : (code === 'PARTIALLY_SUPPORTED' ? 'warn'
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
      ? (palette.querySelector('input[name="create-account-color"]:checked')?.value || pickNextAvailableColorId(state.status?.profiles || []))
      : pickNextAvailableColorId(state.status?.profiles || []);
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

function refreshOpenDialogTranslations() {
  const ctx = state.openDialogContext;
  if (!ctx) return;

  if (ctx.type === 'create-step2' && document.getElementById('create-profile-dialog')?.open) {
    document.getElementById('create-step2-title').textContent = t('dialogs.create.step2Title', {
      name: ctx.prof.name
    });
  } else if (ctx.type === 'rename' && document.getElementById('rename-profile-dialog')?.open) {
    document.getElementById('rename-profile-subtitle').textContent = t('dialogs.rename.subtitle', {
      name: ctx.prof.name
    });
  } else if (ctx.type === 'delete' && document.getElementById('delete-profile-dialog')?.open) {
    document.getElementById('delete-profile-warning').textContent = t('dialogs.delete.warning', {
      name: ctx.prof.name
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
  renderProfiles();
  renderPrimaryZalo();
  renderBackups();
  renderDiagnostics();
  renderSettingsAbout();
  renderCreateColorPalette();
  renderRenameColorPalette();
  updateLanguageControls();
  updateAppearanceControls();
  updatePendingControls();
  updateDocumentTitle(state.activeView);
  refreshOpenDialogTranslations();
}

export async function refreshAll({ isVerify = false } = {}) {
  const key = isVerify ? 'verify' : 'refresh';
  if (state.pendingGlobalActions.has(key)) return;

  const seq = ++state.refreshSeq;
  state.pendingGlobalActions.add(key);
  updatePendingControls();
  renderTopbarSummary();

  try {
    const toggleSizes = document.getElementById('toggle-sizes');
    const withSize = Boolean(toggleSizes?.checked);
    const [statusData, backupsData, diagData, isoData] = await Promise.all([
      apiCall(`/api/status${withSize ? '?withSize=1' : ''}`, 'GET', null, 'get_status', { withSize }),
      apiCall('/api/backups', 'GET', null, 'list_backups', {}),
      apiCall('/api/diagnostics', 'GET', null, 'get_diagnostics', {}),
      apiCall('/api/verify-isolation', 'GET', null, 'verify_isolation', {})
    ]);
    if (seq !== state.refreshSeq) return;
    if (statusData && statusData.ok === false) {
      updateConnectionBadge(state.connectionMode || 'bridge', false);
      showErrorBanner(statusData.error_code || 'HOST_EXEC_FAILED', statusData.message || '');
    } else {
      state.status = statusData;
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

async function runProfileAction(action, prof) {
  if (state.pendingProfileActions.has(prof.id)) return;
  clearBanner();
  state.pendingProfileActions.set(prof.id, action);
  renderProfiles();

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
      showErrorBanner(res.error_code, res.message);
    } else if (action === 'backup' && res?.ok !== false) {
      showKeyBanner('messages.backupCreated', {}, 'success');
    }
  } catch (err) {
    showErrorBanner(err?.errorCode || 'HOST_EXEC_FAILED', err?.message || '');
  } finally {
    state.pendingProfileActions.delete(prof.id);
    renderProfiles();
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
  clearCreateStep1Error();
  renderCreateColorPalette(pickNextAvailableColorId(state.status?.profiles || []));
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

function openRenameDialog(prof) {
  state.activeRenameId = prof.id;
  state.openDialogContext = { type: 'rename', prof };
  document.getElementById('rename-profile-subtitle').textContent = t('dialogs.rename.subtitle', {
    name: prof.name
  });
  document.getElementById('rename-profile-name').value = prof.name;
  renderRenameColorPalette(getAccountColorId(prof));
  document.getElementById('rename-profile-dialog').showModal();
}

function openDeleteDialog(prof) {
  state.activeDeleteId = prof.id;
  state.openDialogContext = { type: 'delete', prof };
  document.getElementById('delete-profile-warning').textContent = t('dialogs.delete.warning', {
    name: prof.name
  });
  document.getElementById('delete-profile-dialog').showModal();
}

function openRestoreDialog(bk) {
  state.activeRestoreBackupDir = bk.backup_dir;
  state.activeRestoreSourceId = bk.source_profile_id || bk.source_profile?.id || null;
  state.openDialogContext = { type: 'restore', bk };
  document.getElementById('restore-backup-subtitle').textContent = t('dialogs.restore.subtitle', {
    backupName: bk.backup_name || bk.backup_id || ''
  });
  document.getElementById('restore-profile-name').value = '';
  document.getElementById('restore-backup-dialog').showModal();
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
  document.getElementById('profile-details-dialog').showModal();
}

function wireDialogsAndFallbacks() {
  for (const btnId of ['btn-open-create', 'btn-empty-create']) {
    document.getElementById(btnId)?.addEventListener('click', () => {
      clearCreateStep1Error();
      renderCreateColorPalette(pickNextAvailableColorId(state.status?.profiles || []));
    });
  }

  if (typeof HTMLButtonElement === 'undefined' || !('commandForElement' in HTMLButtonElement.prototype)) {
    for (const btn of document.querySelectorAll('button[command="show-modal"][commandfor]')) {
      btn.addEventListener('click', () => {
        const target = document.getElementById(btn.getAttribute('commandfor'));
        if (target && typeof target.showModal === 'function') {
          target.showModal();
        }
      });
    }
  }

  document.getElementById('btn-dismiss-banner')?.addEventListener('click', clearBanner);

  const createDialog = document.getElementById('create-profile-dialog');
  const createForm = document.getElementById('create-profile-form');

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
      await runProfileAction('launch', prof);
    }
  });

  createForm?.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (state.pendingGlobalActions.has('create')) return;

    const nameInput = document.getElementById('create-profile-name');
    const notesInput = document.getElementById('create-profile-notes');
    const colorInput = document.querySelector('input[name="create-account-color"]:checked');
    const name = nameInput.value.trim();
    const notes = notesInput.value.trim();
    const chosenColor = colorInput?.value || pickNextAvailableColorId(state.status?.profiles || []);
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
    try {
      const res = await apiCall(`/api/profiles/${encodeURIComponent(targetId)}/rename`, 'POST', { newName }, 'rename_profile', { id: targetId, newName });
      if (res && res.ok === false) showErrorBanner(res.error_code, res.message);
      else showKeyBanner('messages.accountRenamed', {}, 'success');
      await refreshAll();
    } catch (err) {
      showErrorBanner(err?.errorCode || 'HOST_EXEC_FAILED', err?.message || '');
    } finally {
      state.pendingProfileActions.delete(targetId);
      renderProfiles();
    }
  });

  const deleteDialog = document.getElementById('delete-profile-dialog');
  deleteDialog?.addEventListener('close', async () => {
    const action = deleteDialog.returnValue;
    deleteDialog.returnValue = '';
    state.openDialogContext = null;
    const targetId = state.activeDeleteId;
    state.activeDeleteId = null;
    if (action !== 'delete' || !targetId || state.pendingProfileActions.has(targetId)) return;

    state.pendingProfileActions.set(targetId, 'delete');
    renderProfiles();
    try {
      const res = await apiCall(
        `/api/profiles/${encodeURIComponent(targetId)}`,
        'DELETE',
        { confirm: true },
        'delete_profile',
        { id: targetId, confirm: true }
      );
      if (res && res.ok === false) {
        showErrorBanner(res.error_code, res.message);
      } else {
        removeAccountColor(targetId);
        showKeyBanner('messages.accountDeleted', {}, 'success');
      }
      await refreshAll();
    } catch (err) {
      showErrorBanner(err?.errorCode || 'HOST_EXEC_FAILED', err?.message || '');
    } finally {
      state.pendingProfileActions.delete(targetId);
      renderProfiles();
      if (needsFocusRestore(document.activeElement)) {
        const firstCardBtn = document.querySelector('#profiles-list .account-card .btn-primary');
        const emptyCreateBtn = document.getElementById('btn-empty-create');
        const openCreateBtn = document.getElementById('btn-open-create');
        if (firstCardBtn && !firstCardBtn.disabled && !needsFocusRestore(firstCardBtn)) {
          firstCardBtn.focus();
        } else if (emptyCreateBtn && !needsFocusRestore(emptyCreateBtn)) {
          emptyCreateBtn.focus();
        } else if (openCreateBtn && !needsFocusRestore(openCreateBtn)) {
          openCreateBtn.focus();
        }
      }
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
}

export async function initApp() {
  await initI18n();
  applyDomTranslations();
  applyThemePreference(getSavedThemePreference());
  renderCreateColorPalette();
  renderRenameColorPalette();
  updateLanguageControls();
  updateAppearanceControls();
  updateDocumentTitle(state.activeView);
  wireDialogsAndFallbacks();

  document.getElementById('btn-brand-home')?.addEventListener('click', () => {
    selectView('profiles');
  });

  for (const btn of document.querySelectorAll('.top-nav-btn[data-view]')) {
    btn.addEventListener('click', () => {
      selectView(btn.getAttribute('data-view'));
    });
  }

  for (const btn of document.querySelectorAll('.back-link[data-back-to]')) {
    btn.addEventListener('click', () => {
      selectView(btn.getAttribute('data-back-to'));
    });
  }

  document.getElementById('btn-goto-diagnostics')?.addEventListener('click', () => {
    selectView('diagnostics');
  });

  document.getElementById('btn-help-diagnostics')?.addEventListener('click', () => {
    selectView('diagnostics');
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

  onLocaleChange(() => {
    applyDomTranslations();
    renderAll();
  });

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
