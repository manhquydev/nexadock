import {
  initI18n,
  t,
  getLocale,
  setLocale,
  onLocaleChange,
  applyDomTranslations,
  resolveErrorInfo,
  translateSupportStatus,
  formatBytes,
  formatDateTime,
  formatNumber
} from './i18n/index.js';

const THEME_STORAGE_KEY = 'color-scheme';
const UI_VERSION = '0.1.0';

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
  openDialogContext: null, // { type: 'rename' | 'delete' | 'restore' | 'details', prof?, bk? }
  banner: null, // { kind, messageKey?, messageParams?, rawText?, errorCode?, rawDetail? }
  pendingProfileActions: new Map(), // Map<profileId, action>
  pendingGlobalActions: new Set() // Set<'create' | 'restore' | 'refresh' | 'verify'>
};

export function getTauriInvoke() {
  if (typeof window === 'undefined') return null;
  if (window.__TAURI__?.core?.invoke) return window.__TAURI__.core.invoke;
  if (window.__TAURI_INTERNALS__?.invoke) return window.__TAURI_INTERNALS__.invoke;
  return null;
}

function updateConnectionBadge(mode = state.connectionMode, ok = state.connectionOk) {
  state.connectionMode = mode;
  state.connectionOk = ok;
  const badge = document.getElementById('host-mode-badge');
  const dot = document.getElementById('connection-dot');
  if (badge) {
    if (!mode) {
      badge.textContent = t('app.connecting');
    } else {
      badge.textContent = mode === 'tauri' ? t('app.modeTauri') : t('app.modeBridge');
    }
  }
  if (dot) {
    dot.classList.toggle('is-online', Boolean(ok));
    dot.classList.toggle('is-error', mode !== null && !ok);
  }
}

export async function apiCall(route, method = 'GET', body = null, tauriCmd = null, tauriArgs = {}) {
  const invoke = getTauriInvoke();
  if (invoke && tauriCmd) {
    updateConnectionBadge('tauri', true);
    return await invoke(tauriCmd, tauriArgs);
  }

  updateConnectionBadge('bridge', true);
  const init = {
    method,
    headers: { Accept: 'application/json' }
  };
  if (body !== null) {
    init.headers['Content-Type'] = 'application/json';
    init.body = JSON.stringify(body);
  }
  const resp = await fetch(route, init);
  const payload = await resp.json();
  return payload;
}

function renderBanner() {
  const banner = document.getElementById('status-banner');
  const textEl = document.getElementById('status-banner-text');
  const detailsEl = document.getElementById('status-banner-details');
  const rawEl = document.getElementById('status-banner-raw');
  if (!banner || !textEl || !detailsEl || !rawEl) return;

  if (!state.banner) {
    banner.hidden = true;
    textEl.textContent = '';
    detailsEl.hidden = true;
    rawEl.textContent = '';
    return;
  }

  const b = state.banner;
  banner.className = 'status-banner';
  if (b.kind === 'error') banner.classList.add('is-error');
  else if (b.kind === 'success') banner.classList.add('is-success');

  if (b.errorCode) {
    const info = resolveErrorInfo(b.errorCode, b.rawDetail);
    textEl.textContent = info.userMessage;
    const rawParts = [`${t('errors.rawCodeLabel')} ${info.rawCode}`];
    if (info.rawDetail) {
      rawParts.push(`${t('errors.rawMessageLabel')} ${info.rawDetail}`);
    }
    rawEl.textContent = rawParts.join(' · ');
    detailsEl.hidden = false;
  } else if (b.messageKey) {
    textEl.textContent = t(b.messageKey, b.messageParams || {});
    detailsEl.hidden = true;
    rawEl.textContent = '';
  } else {
    textEl.textContent = b.rawText || '';
    detailsEl.hidden = true;
    rawEl.textContent = '';
  }

  banner.hidden = false;
}

function showKeyBanner(messageKey, messageParams = {}, kind = 'info') {
  state.banner = { kind, messageKey, messageParams };
  renderBanner();
}

function showErrorBanner(errorCode, rawDetail = '') {
  state.banner = { kind: 'error', errorCode: errorCode || 'UNKNOWN', rawDetail };
  renderBanner();
}

function showRawBanner(rawText, kind = 'info') {
  state.banner = { kind, rawText };
  renderBanner();
}

function clearBanner() {
  state.banner = null;
  renderBanner();
}

function makeBadge(text, variant) {
  const span = document.createElement('span');
  span.className = `badge ${variant}`;
  span.textContent = text;
  return span;
}

function makeButton(label, className, onClick, { disabled = false } = {}) {
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = `btn btn-sm ${className}`;
  btn.textContent = label;
  btn.disabled = disabled;
  btn.addEventListener('click', onClick);
  return btn;
}

function appendKvRow(dl, label, value) {
  const dt = document.createElement('dt');
  dt.textContent = label;
  const dd = document.createElement('dd');
  dd.textContent = value;
  dl.appendChild(dt);
  dl.appendChild(dd);
}

// Theme management (system | light | dark)
export function getStoredThemePreference() {
  try {
    const saved = localStorage.getItem(THEME_STORAGE_KEY);
    if (saved === 'light' || saved === 'dark') return saved;
  } catch (_err) {
    // Ignore storage errors
  }
  return 'system';
}

export function applyThemePreference(choice) {
  const normalized = choice === 'light' || choice === 'dark' ? choice : 'system';
  const meta = document.querySelector('meta[name="color-scheme"]');
  if (normalized === 'system') {
    if (meta) meta.setAttribute('content', 'light dark');
    document.documentElement.setAttribute('data-theme', 'system');
    try {
      localStorage.removeItem(THEME_STORAGE_KEY);
    } catch (_err) {
      // Ignore storage errors
    }
  } else {
    if (meta) meta.setAttribute('content', normalized);
    document.documentElement.setAttribute('data-theme', normalized);
    try {
      localStorage.setItem(THEME_STORAGE_KEY, normalized);
    } catch (_err) {
      // Ignore storage errors
    }
  }
  updateAppearanceControls();
}

function updateAppearanceControls() {
  const active = getStoredThemePreference();
  const isSystemDark = typeof window !== 'undefined'
    && typeof window.matchMedia === 'function'
    && window.matchMedia('(prefers-color-scheme: dark)').matches;

  const sysLabel = document.getElementById('theme-label-system');
  if (sysLabel) {
    sysLabel.textContent = isSystemDark
      ? t('settings.appearance.systemCurrentDark')
      : t('settings.appearance.systemCurrentLight');
  }

  for (const input of document.querySelectorAll('input[name="ui-theme"]')) {
    input.checked = input.value === active;
  }
}

function updateLanguageControls() {
  const active = getLocale();
  for (const input of document.querySelectorAll('input[name="ui-locale"]')) {
    input.checked = input.value === active;
  }
}

function updatePendingControls() {
  const createBtn = document.querySelector('button[commandfor="create-profile-dialog"]');
  if (createBtn) createBtn.disabled = state.pendingGlobalActions.has('create');

  const restoreSubmit = document.querySelector('#restore-backup-form button[value="restore"]');
  if (restoreSubmit) restoreSubmit.disabled = state.pendingGlobalActions.has('restore');

  const createSubmit = document.querySelector('#create-profile-form button[value="create"]');
  if (createSubmit) createSubmit.disabled = state.pendingGlobalActions.has('create');

  const refreshBtn = document.getElementById('btn-refresh');
  if (refreshBtn) refreshBtn.disabled = state.pendingGlobalActions.has('refresh');

  const verifyBtn = document.getElementById('btn-run-isolation');
  if (verifyBtn) verifyBtn.disabled = state.pendingGlobalActions.has('verify');
}

// Navigation
function selectView(viewName) {
  state.activeView = viewName;
  for (const btn of document.querySelectorAll('.nav-item[data-view]')) {
    const isMatch = btn.getAttribute('data-view') === viewName;
    btn.classList.toggle('is-active', isMatch);
    if (isMatch) btn.setAttribute('aria-current', 'page');
    else btn.removeAttribute('aria-current');
  }
  for (const section of document.querySelectorAll('.view-section')) {
    const isMatch = section.id === `view-${viewName}`;
    section.hidden = !isMatch;
  }
}

// Profile status helpers
function resolveProfileState(prof) {
  const pendingAction = state.pendingProfileActions.get(prof.id);
  if (pendingAction === 'launch' || prof.status === 'starting') {
    return { label: t('profiles.state.starting'), variant: 'badge-warn' };
  }
  if (pendingAction === 'stop' || prof.status === 'stopping') {
    return { label: t('profiles.state.stopping'), variant: 'badge-warn' };
  }
  if (prof.status === 'error') return { label: t('profiles.state.error'), variant: 'badge-fail' };
  if (prof.status === 'unknown') return { label: t('profiles.state.unknown'), variant: 'badge-neutral' };
  if (prof.running) return { label: t('profiles.state.running'), variant: 'badge-running' };
  if (prof.stale_lockfile_detected) return { label: t('profiles.state.crashRecoverable'), variant: 'badge-warn' };
  return { label: t('profiles.state.stopped'), variant: 'badge-stopped' };
}

function renderStatusBar() {
  const bar = document.getElementById('profiles-status-bar');
  const primEl = document.getElementById('primary-status-pill');
  const st = state.status;
  if (!bar || !primEl) return;

  if (!st) {
    bar.textContent = state.pendingGlobalActions.has('refresh')
      ? t('statusBar.loading')
      : t('statusBar.checking');
    primEl.textContent = '';
    return;
  }

  const inst = st.installation || {};
  const profiles = st.profiles || [];
  const runningCount = profiles.filter((p) => p.running).length;

  if (!inst.valid) {
    bar.textContent = t('statusBar.notFound');
  } else if (runningCount > 0) {
    bar.textContent = t('statusBar.detectedRunning', { count: formatNumber(runningCount) });
  } else {
    bar.textContent = t('statusBar.detectedIdle');
  }

  const prim = st.primary_zalo || {};
  primEl.textContent = prim.is_running
    ? t('statusBar.primaryRunning', { count: formatNumber((prim.locking_pids || []).length) })
    : t('statusBar.primaryIdle');
}

function filterProfiles(profiles, query) {
  const q = query.trim().toLowerCase();
  if (!q) return profiles;
  return profiles.filter((p) => {
    const name = (p.name || '').toLowerCase();
    const notes = (p.notes || '').toLowerCase();
    const dir = (p.directory_name || '').toLowerCase();
    const id = (p.id || '').toLowerCase();
    return name.includes(q) || notes.includes(q) || dir.includes(q) || id.includes(q);
  });
}

function positionMenu(trigger, menuList) {
  const rect = trigger.getBoundingClientRect();
  const menuWidth = 190;
  const left = Math.max(8, Math.min(rect.right - menuWidth, window.innerWidth - menuWidth - 8));
  const top = Math.min(rect.bottom + 4, window.innerHeight - 180);
  menuList.style.left = `${left}px`;
  menuList.style.top = `${top}px`;
}

function buildOverflowMenu(prof, isPending) {
  const menuId = `menu-${String(prof.id).replace(/[^a-zA-Z0-9_-]/g, '_')}`;
  const menuWrap = document.createElement('div');
  menuWrap.className = 'menu-wrap';

  const supportsPopover = Object.hasOwn(HTMLElement.prototype, 'popover');

  const moreBtn = document.createElement('button');
  moreBtn.type = 'button';
  moreBtn.className = 'btn btn-secondary btn-sm';
  moreBtn.textContent = '⋯';
  moreBtn.disabled = isPending;
  moreBtn.setAttribute('aria-label', t('profiles.actions.more', { name: prof.name }));
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
    if (supportsPopover) {
      positionMenu(moreBtn, menuList);
      if (!isOpen()) menuList.showPopover();
    } else {
      menuList.hidden = false;
      menuList.classList.add('is-fallback-open');
    }
    moreBtn.setAttribute('aria-expanded', 'true');
    const items = getEnabledItems();
    if (items.length > 0) {
      const target = focusMode === 'last' ? items[items.length - 1] : items[0];
      target.focus();
    }
  };

  const closeMenu = ({ restoreFocus = false } = {}) => {
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
        positionMenu(moreBtn, menuList);
      }
    });
  } else {
    document.addEventListener('click', (event) => {
      if (isOpen() && !menuWrap.contains(event.target)) {
        closeMenu({ restoreFocus: false });
      }
    });
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

  const addMenuItem = (label, onSelect, { disabled = false, danger = false } = {}) => {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = danger ? 'menu-item menu-item-danger' : 'menu-item';
    item.setAttribute('role', 'menuitem');
    item.tabIndex = -1;
    item.textContent = label;
    item.disabled = disabled || isPending;
    item.addEventListener('click', () => {
      closeMenu({ restoreFocus: false });
      onSelect();
    });
    menuList.appendChild(item);
  };

  addMenuItem(t('profiles.actions.rename'), () => openRenameDialog(prof));
  addMenuItem(t('profiles.actions.backup'), () => runProfileAction('backup', prof), { disabled: Boolean(prof.running) });
  addMenuItem(t('profiles.actions.details'), () => openDetailsDialog(prof));
  addMenuItem(t('profiles.actions.delete'), () => openDeleteDialog(prof), { disabled: Boolean(prof.running), danger: true });

  menuWrap.appendChild(moreBtn);
  menuWrap.appendChild(menuList);
  return menuWrap;
}

function renderProfiles() {
  const list = document.getElementById('profiles-list');
  const emptyEl = document.getElementById('profiles-empty');
  const emptyTitle = document.getElementById('profiles-empty-title');
  const emptyHint = document.getElementById('profiles-empty-hint');
  if (!list || !emptyEl) return;

  list.replaceChildren();
  const allProfiles = state.status?.profiles || [];
  const filtered = filterProfiles(allProfiles, state.searchQuery);

  if (allProfiles.length === 0) {
    emptyTitle.textContent = t('profiles.emptyTitle');
    emptyHint.textContent = t('profiles.emptyHint');
    emptyEl.hidden = false;
    list.hidden = true;
    return;
  }

  if (filtered.length === 0) {
    emptyTitle.textContent = t('profiles.noMatches', { query: state.searchQuery.trim() });
    emptyHint.textContent = '';
    emptyEl.hidden = false;
    list.hidden = true;
    return;
  }

  emptyEl.hidden = true;
  list.hidden = false;

  for (const prof of filtered) {
    const isPending = state.pendingProfileActions.has(prof.id);
    const li = document.createElement('li');
    li.className = 'profile-row';
    li.dataset.profileId = prof.id;

    const main = document.createElement('div');
    main.className = 'profile-main';

    const titleLine = document.createElement('div');
    titleLine.className = 'profile-title-line';

    const nameEl = document.createElement('span');
    nameEl.className = 'profile-name';
    nameEl.textContent = prof.name;
    titleLine.appendChild(nameEl);

    const pState = resolveProfileState(prof);
    titleLine.appendChild(makeBadge(pState.label, pState.variant));
    main.appendChild(titleLine);

    if (prof.notes) {
      const subEl = document.createElement('div');
      subEl.className = 'profile-subline';
      subEl.textContent = prof.notes;
      main.appendChild(subEl);
    }

    const actions = document.createElement('div');
    actions.className = 'profile-actions';

    if (prof.running) {
      actions.appendChild(makeButton(t('profiles.actions.focus'), 'btn-primary', () => runProfileAction('focus', prof), { disabled: isPending }));
      actions.appendChild(makeButton(t('profiles.actions.stop'), 'btn-danger', () => runProfileAction('stop', prof), { disabled: isPending }));
    } else {
      actions.appendChild(makeButton(t('profiles.actions.launch'), 'btn-primary', () => runProfileAction('launch', prof), { disabled: isPending }));
    }

    actions.appendChild(buildOverflowMenu(prof, isPending));

    li.appendChild(main);
    li.appendChild(actions);
    list.appendChild(li);
  }
}

function renderBackups() {
  const tbody = document.getElementById('backups-tbody');
  const emptyEl = document.getElementById('backups-empty');
  const tableWrap = document.getElementById('backups-table-wrap');
  if (!tbody || !emptyEl || !tableWrap) return;

  tbody.replaceChildren();
  const backups = state.backups || [];

  if (backups.length === 0) {
    emptyEl.hidden = false;
    tableWrap.hidden = true;
    return;
  }

  emptyEl.hidden = true;
  tableWrap.hidden = false;
  const restorePending = state.pendingGlobalActions.has('restore');

  for (const bk of backups) {
    const tr = document.createElement('tr');

    const th = document.createElement('th');
    th.scope = 'row';
    th.textContent = bk.backup_name || bk.backup_id || '—';
    tr.appendChild(th);

    const srcTd = document.createElement('td');
    const srcName = bk.source_profile_name || bk.source_profile?.name || t('backups.unknownSource');
    const srcId = bk.source_profile_id || bk.source_profile?.id || '';
    srcTd.textContent = srcId ? `${srcName} (${srcId})` : srcName;
    tr.appendChild(srcTd);

    const sizeTd = document.createElement('td');
    sizeTd.textContent = t('backups.fileCount', {
      count: formatNumber(bk.file_count ?? 0),
      size: formatBytes(bk.total_bytes ?? 0)
    });
    tr.appendChild(sizeTd);

    const timeTd = document.createElement('td');
    timeTd.textContent = formatDateTime(bk.created_at);
    tr.appendChild(timeTd);

    const verTd = document.createElement('td');
    const isVerifiable = Boolean(bk.backup_dir) && (bk.file_count ?? 0) > 0;
    verTd.appendChild(makeBadge(
      isVerifiable ? t('backups.verifiable') : t('backups.unverified'),
      isVerifiable ? 'badge-pass' : 'badge-neutral'
    ));
    tr.appendChild(verTd);

    const actTd = document.createElement('td');
    actTd.appendChild(makeButton(t('backups.restoreButton'), 'btn-primary', () => openRestoreDialog(bk), { disabled: restorePending }));
    tr.appendChild(actTd);

    tbody.appendChild(tr);
  }
}

export function resolveIsolationCheckResult(chk) {
  if (typeof chk.status === 'string' && chk.status.trim()) {
    const code = chk.status.trim().toUpperCase();
    if (code === 'PASS' || code === 'FAIL' || code === 'NOT_TESTED' || code === 'BLOCKED' || code === 'UNKNOWN') {
      return code;
    }
  }
  if (chk.passed === true) return 'PASS';
  if (chk.passed === false) return 'FAIL';
  return 'UNKNOWN';
}

function renderDiagnostics() {
  const st = state.status;
  const diag = state.diagnostics;
  const iso = state.isolation;

  const appList = document.getElementById('diag-app-list');
  if (appList) {
    appList.replaceChildren();
    const inst = st?.installation || {};
    appendKvRow(
      appList,
      t('diagnostics.fields.zaloInstall'),
      inst.valid ? t('diagnostics.values.installed') : (inst.error || t('diagnostics.values.notInstalled'))
    );
    appendKvRow(
      appList,
      t('diagnostics.fields.installVersion'),
      [inst.version, inst.mode].filter(Boolean).join(' · ') || '—'
    );
    appendKvRow(appList, t('diagnostics.fields.authenticode'), inst.authenticode_status || '—');
    appendKvRow(appList, t('diagnostics.fields.dataDir'), st?.data_dir || '—');
  }

  const secList = document.getElementById('diag-security-list');
  if (secList) {
    secList.replaceChildren();
    if (diag) {
      appendKvRow(
        secList,
        t('diagnostics.fields.elevation'),
        diag.is_elevated ? t('diagnostics.values.elevated') : t('diagnostics.values.standardUser')
      );
      const sacLabel = diag.smart_app_control_state === 1
        ? t('diagnostics.values.sacEnforced')
        : t('diagnostics.values.sacCode', { code: String(diag.smart_app_control_state ?? 'UNKNOWN') });
      appendKvRow(secList, t('diagnostics.fields.smartAppControl'), sacLabel);
    }
    const prim = st?.primary_zalo || {};
    appendKvRow(
      secList,
      t('diagnostics.fields.primaryProtection'),
      prim.is_running
        ? t('diagnostics.values.primaryLocked', { count: formatNumber((prim.locking_pids || []).length) })
        : t('diagnostics.values.primaryIdle')
    );
  }

  const diagTbody = document.getElementById('diagnostics-tbody');
  if (diagTbody) {
    diagTbody.replaceChildren();
    const checks = iso?.checks || [];
    if (checks.length === 0) {
      const tr = document.createElement('tr');
      const td = document.createElement('td');
      td.colSpan = 3;
      td.textContent = t('diagnostics.noChecks');
      tr.appendChild(td);
      diagTbody.appendChild(tr);
    } else {
      for (const chk of checks) {
        const tr = document.createElement('tr');
        const th = document.createElement('th');
        th.scope = 'row';
        th.textContent = chk.check_name || '—';
        tr.appendChild(th);

        const code = resolveIsolationCheckResult(chk);
        const variant = code === 'PASS' ? 'badge-pass'
          : (code === 'FAIL' ? 'badge-fail'
            : (code === 'BLOCKED' ? 'badge-warn' : 'badge-neutral'));
        const resTd = document.createElement('td');
        resTd.appendChild(makeBadge(`${t(`diagnostics.result.${code}`)} (${code})`, variant));
        tr.appendChild(resTd);

        const detTd = document.createElement('td');
        detTd.textContent = chk.detail || '';
        tr.appendChild(detTd);

        diagTbody.appendChild(tr);
      }
    }
  }

  const profList = document.getElementById('diag-profiles-list');
  if (profList) {
    profList.replaceChildren();
    const profiles = st?.profiles || [];
    const running = profiles.filter((p) => p.running).length;
    const stale = profiles.filter((p) => p.stale_lockfile_detected).length;
    appendKvRow(
      profList,
      t('diagnostics.fields.profileSummary'),
      t('diagnostics.values.profileCounts', {
        total: formatNumber(profiles.length),
        running: formatNumber(running),
        stale: formatNumber(stale)
      })
    );

    const uidCounts = new Map();
    for (const p of profiles) {
      if (p.running && p.zalo_user_id) {
        uidCounts.set(p.zalo_user_id, (uidCounts.get(p.zalo_user_id) || 0) + 1);
      }
    }
    const dupIds = Array.from(uidCounts.entries()).filter(([, c]) => c > 1).map(([id]) => id);
    appendKvRow(
      profList,
      t('diagnostics.fields.duplicateUid'),
      dupIds.length > 0 ? t('diagnostics.values.duplicatesFound', { ids: dupIds.join(', ') }) : t('diagnostics.values.noDuplicates')
    );
  }

  const envList = document.getElementById('diag-env-list');
  if (envList) {
    envList.replaceChildren();
    appendKvRow(envList, t('diagnostics.fields.platform'), st?.platform || '—');
    if (diag) {
      const diskFormatted = formatBytes(diag.disk_free_bytes);
      appendKvRow(
        envList,
        t('diagnostics.fields.diskFree'),
        diag.disk_free_ok
          ? t('diagnostics.values.diskOk', { size: diskFormatted })
          : t('diagnostics.values.diskLow', { size: diskFormatted })
      );
      const redir = diag.folder_redirection || {};
      appendKvRow(
        envList,
        t('diagnostics.fields.folderRedirection'),
        redir.redirected
          ? t('diagnostics.values.redirected', { warning: redir.warning || '—' })
          : t('diagnostics.values.notRedirected')
      );
    }
  }

  const readyTbody = document.getElementById('readiness-tbody');
  if (readyTbody) {
    readyTbody.replaceChildren();
    const matrix = diag?.platform_support_matrix || [];
    for (const row of matrix) {
      const tr = document.createElement('tr');
      const th = document.createElement('th');
      th.scope = 'row';
      th.textContent = row.platform || '—';
      tr.appendChild(th);

      const { code, label } = translateSupportStatus(row.status);
      const variant = code === 'SUPPORTED_AND_TESTED' ? 'badge-pass'
        : (code === 'PARTIALLY_SUPPORTED' ? 'badge-warn'
          : (code === 'UNSUPPORTED' || code === 'BLOCKED' ? 'badge-fail' : 'badge-neutral'));
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

function refreshOpenDialogTranslations() {
  const ctx = state.openDialogContext;
  if (!ctx) return;

  if (ctx.type === 'rename' && document.getElementById('rename-profile-dialog')?.open) {
    document.getElementById('rename-profile-subtitle').textContent = t('dialogs.rename.subtitle', {
      name: ctx.prof.name,
      id: ctx.prof.id
    });
  } else if (ctx.type === 'delete' && document.getElementById('delete-profile-dialog')?.open) {
    document.getElementById('delete-profile-warning').textContent = t('dialogs.delete.warning', {
      name: ctx.prof.name,
      dir: ctx.prof.directory_name || ctx.prof.id
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
  renderStatusBar();
  renderProfiles();
  renderBackups();
  renderDiagnostics();
  renderSettingsAbout();
  updateLanguageControls();
  updateAppearanceControls();
  updatePendingControls();
  refreshOpenDialogTranslations();
}

export async function refreshAll({ isVerify = false } = {}) {
  const key = isVerify ? 'verify' : 'refresh';
  if (state.pendingGlobalActions.has(key)) return;

  state.pendingGlobalActions.add(key);
  updatePendingControls();
  renderStatusBar();

  try {
    const toggleSizes = document.getElementById('toggle-sizes');
    const withSize = Boolean(toggleSizes?.checked);
    const [statusData, backupsData, diagData, isoData] = await Promise.all([
      apiCall(`/api/status${withSize ? '?withSize=1' : ''}`, 'GET', null, 'get_status', { withSize }),
      apiCall('/api/backups', 'GET', null, 'list_backups', {}),
      apiCall('/api/diagnostics', 'GET', null, 'get_diagnostics', {}),
      apiCall('/api/verify-isolation', 'GET', null, 'verify_isolation', {})
    ]);
    state.status = statusData;
    state.backups = backupsData?.backups || [];
    state.diagnostics = diagData;
    state.isolation = isoData;
  } catch (err) {
    updateConnectionBadge(state.connectionMode || 'bridge', false);
    showKeyBanner('statusBar.hostError', { message: err.message }, 'error');
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
    if (res && res.ok === false) {
      showErrorBanner(res.error_code, res.message);
    } else if (res?.message) {
      showKeyBanner('messages.actionSuccess', {}, 'success');
    }
    await refreshAll();
  } catch (err) {
    showRawBanner(err.message, 'error');
  } finally {
    state.pendingProfileActions.delete(prof.id);
    renderProfiles();
  }
}

function openRenameDialog(prof) {
  state.activeRenameId = prof.id;
  state.openDialogContext = { type: 'rename', prof };
  document.getElementById('rename-profile-subtitle').textContent = t('dialogs.rename.subtitle', {
    name: prof.name,
    id: prof.id
  });
  document.getElementById('rename-profile-name').value = prof.name;
  document.getElementById('rename-profile-dialog').showModal();
}

function openDeleteDialog(prof) {
  state.activeDeleteId = prof.id;
  state.openDialogContext = { type: 'delete', prof };
  document.getElementById('delete-profile-warning').textContent = t('dialogs.delete.warning', {
    name: prof.name,
    dir: prof.directory_name || prof.id
  });
  document.getElementById('delete-profile-dialog').showModal();
}

function openRestoreDialog(bk) {
  state.activeRestoreBackupDir = bk.backup_dir;
  state.openDialogContext = { type: 'restore', bk };
  document.getElementById('restore-backup-subtitle').textContent = t('dialogs.restore.subtitle', {
    backupName: bk.backup_name || bk.backup_id || ''
  });
  document.getElementById('restore-profile-name').value = '';
  document.getElementById('restore-backup-dialog').showModal();
}

function populateDetailsDialog(prof) {
  const dl = document.getElementById('profile-details-list');
  dl.replaceChildren();
  const pState = resolveProfileState(prof);

  appendKvRow(dl, t('profiles.details.id'), prof.id || t('profiles.details.none'));
  appendKvRow(dl, t('profiles.details.directory'), prof.directory_name || t('profiles.details.none'));
  appendKvRow(dl, t('profiles.details.status'), pState.label);
  appendKvRow(dl, t('profiles.details.zaloUserId'), prof.zalo_user_id || t('profiles.state.loginUnverified'));
  appendKvRow(dl, t('profiles.details.pid'), prof.main_pid != null ? String(prof.main_pid) : t('profiles.details.none'));
  appendKvRow(dl, t('profiles.details.processCount'), prof.process_count != null ? formatNumber(prof.process_count) : '0');
  appendKvRow(dl, t('profiles.details.windowTitle'), prof.window_title || t('profiles.details.none'));
  appendKvRow(dl, t('profiles.details.ram'), prof.running ? formatBytes(prof.total_working_set_bytes) : formatBytes(0));
  appendKvRow(dl, t('profiles.details.disk'), prof.data_size_bytes != null ? formatBytes(prof.data_size_bytes) : t('profiles.details.notMeasured'));
  appendKvRow(dl, t('profiles.details.staleLockfile'), prof.stale_lockfile_detected ? t('profiles.details.yes') : t('profiles.details.no'));
  appendKvRow(dl, t('profiles.details.createdAt'), formatDateTime(prof.created_at));
  appendKvRow(dl, t('profiles.details.lastLaunchedAt'), formatDateTime(prof.last_launched_at));
  appendKvRow(dl, t('profiles.details.notes'), prof.notes || t('profiles.details.none'));
}

function openDetailsDialog(prof) {
  state.openDialogContext = { type: 'details', prof };
  populateDetailsDialog(prof);
  document.getElementById('profile-details-dialog').showModal();
}

function wireDialogsAndFallbacks() {
  if (!('commandForElement' in HTMLButtonElement.prototype)) {
    document.addEventListener('click', (event) => {
      const btn = event.target.closest('button[command][commandfor]');
      if (!btn || btn.disabled) return;
      const target = document.getElementById(btn.getAttribute('commandfor'));
      if (!target || typeof target.showModal !== 'function') return;
      const cmd = btn.getAttribute('command');
      if (cmd === 'show-modal' && !target.open) {
        target.showModal();
      } else if ((cmd === 'close' || cmd === 'request-close') && target.open) {
        target.close('cancel');
      }
    });
  }

  if (!('closedBy' in HTMLDialogElement.prototype)) {
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
        if (pressedOutside && outside(e, dialog)) {
          dialog.close('');
        }
        pressedOutside = false;
      });
    }
  }

  const createDialog = document.getElementById('create-profile-dialog');
  createDialog.addEventListener('close', async () => {
    const action = createDialog.returnValue;
    createDialog.returnValue = '';
    if (action !== 'create' || state.pendingGlobalActions.has('create')) return;

    const nameInput = document.getElementById('create-profile-name');
    const notesInput = document.getElementById('create-profile-notes');
    const name = nameInput.value.trim();
    const notes = notesInput.value.trim();
    nameInput.value = '';
    notesInput.value = '';
    if (!name) return;

    state.pendingGlobalActions.add('create');
    updatePendingControls();
    try {
      const res = await apiCall('/api/profiles', 'POST', { name, notes }, 'create_profile', { name, notes });
      if (res && res.ok === false) showErrorBanner(res.error_code, res.message);
      else showKeyBanner('messages.profileCreated', {}, 'success');
      await refreshAll();
    } catch (err) {
      showRawBanner(err.message, 'error');
    } finally {
      state.pendingGlobalActions.delete('create');
      updatePendingControls();
    }
  });

  const renameDialog = document.getElementById('rename-profile-dialog');
  renameDialog.addEventListener('close', async () => {
    const action = renameDialog.returnValue;
    renameDialog.returnValue = '';
    state.openDialogContext = null;
    const targetId = state.activeRenameId;
    state.activeRenameId = null;
    if (action !== 'rename' || !targetId || state.pendingProfileActions.has(targetId)) return;

    const newName = document.getElementById('rename-profile-name').value.trim();
    if (!newName) return;

    state.pendingProfileActions.set(targetId, 'rename');
    renderProfiles();
    try {
      const res = await apiCall(`/api/profiles/${encodeURIComponent(targetId)}/rename`, 'POST', { newName }, 'rename_profile', { id: targetId, newName });
      if (res && res.ok === false) showErrorBanner(res.error_code, res.message);
      else showKeyBanner('messages.profileRenamed', {}, 'success');
      await refreshAll();
    } catch (err) {
      showRawBanner(err.message, 'error');
    } finally {
      state.pendingProfileActions.delete(targetId);
      renderProfiles();
    }
  });

  const deleteDialog = document.getElementById('delete-profile-dialog');
  deleteDialog.addEventListener('close', async () => {
    const action = deleteDialog.returnValue;
    deleteDialog.returnValue = '';
    state.openDialogContext = null;
    const targetId = state.activeDeleteId;
    state.activeDeleteId = null;
    if (action !== 'delete' || !targetId || state.pendingProfileActions.has(targetId)) return;

    state.pendingProfileActions.set(targetId, 'delete');
    renderProfiles();
    try {
      const res = await apiCall(`/api/profiles/${encodeURIComponent(targetId)}`, 'DELETE', { confirm: true }, 'delete_profile', { id: targetId });
      if (res && res.ok === false) showErrorBanner(res.error_code, res.message);
      else showKeyBanner('messages.profileDeleted', {}, 'success');
      await refreshAll();
    } catch (err) {
      showRawBanner(err.message, 'error');
    } finally {
      state.pendingProfileActions.delete(targetId);
      renderProfiles();
    }
  });

  const restoreDialog = document.getElementById('restore-backup-dialog');
  restoreDialog.addEventListener('close', async () => {
    const action = restoreDialog.returnValue;
    restoreDialog.returnValue = '';
    state.openDialogContext = null;
    const backupDir = state.activeRestoreBackupDir;
    state.activeRestoreBackupDir = null;
    if (action !== 'restore' || !backupDir || state.pendingGlobalActions.has('restore')) return;

    const name = document.getElementById('restore-profile-name').value.trim();
    state.pendingGlobalActions.add('restore');
    updatePendingControls();
    renderBackups();
    try {
      const res = await apiCall('/api/backups/restore', 'POST', { backupDir, name }, 'restore_backup', { backupDir, name });
      if (res && res.ok === false) showErrorBanner(res.error_code, res.message);
      else showKeyBanner('messages.backupRestored', {}, 'success');
      await refreshAll();
    } catch (err) {
      showRawBanner(err.message, 'error');
    } finally {
      state.pendingGlobalActions.delete('restore');
      updatePendingControls();
      renderBackups();
    }
  });

  document.getElementById('profile-details-dialog').addEventListener('close', (e) => {
    e.currentTarget.returnValue = '';
    state.openDialogContext = null;
  });

  document.getElementById('safety-dialog').addEventListener('close', (e) => {
    e.currentTarget.returnValue = '';
  });
}

export async function initApp() {
  await initI18n();
  applyDomTranslations();
  updateLanguageControls();
  updateAppearanceControls();
  wireDialogsAndFallbacks();

  for (const btn of document.querySelectorAll('.nav-item[data-view]')) {
    btn.addEventListener('click', () => {
      selectView(btn.getAttribute('data-view'));
    });
  }

  document.getElementById('btn-goto-diagnostics')?.addEventListener('click', () => {
    selectView('diagnostics');
  });

  document.getElementById('profile-search')?.addEventListener('input', (event) => {
    state.searchQuery = event.target.value;
    renderProfiles();
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
        updateAppearanceControls();
      }
    });
  }

  await refreshAll();
}

if (typeof document !== 'undefined') {
  initApp();
}
