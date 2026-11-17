import {
  t,
  getLocale,
  setLocale,
  onLocaleChange,
  applyDomTranslations,
  translateErrorCode,
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
  activeRestoreBackupDir: null
};

export function getTauriInvoke() {
  if (typeof window === 'undefined') return null;
  if (window.__TAURI__?.core?.invoke) return window.__TAURI__.core.invoke;
  if (window.__TAURI_INTERNALS__?.invoke) return window.__TAURI_INTERNALS__.invoke;
  return null;
}

function updateConnectionBadge(mode, ok = true) {
  state.connectionMode = mode;
  state.connectionOk = ok;
  const badge = document.getElementById('host-mode-badge');
  const dot = document.getElementById('connection-dot');
  if (badge) {
    badge.textContent = mode === 'tauri' ? t('app.modeTauri') : t('app.modeBridge');
  }
  if (dot) {
    dot.classList.toggle('is-online', ok);
    dot.classList.toggle('is-error', !ok);
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

function showBanner(message, kind = 'info') {
  const banner = document.getElementById('status-banner');
  if (!banner) return;
  banner.textContent = message;
  banner.className = 'status-banner';
  if (kind === 'error') banner.classList.add('is-error');
  else if (kind === 'success') banner.classList.add('is-success');
  banner.hidden = false;
}

function clearBanner() {
  const banner = document.getElementById('status-banner');
  if (!banner) return;
  banner.hidden = true;
  banner.textContent = '';
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

  const sysBtn = document.getElementById('theme-btn-system');
  if (sysBtn) {
    sysBtn.textContent = isSystemDark
      ? t('settings.appearance.systemCurrentDark')
      : t('settings.appearance.systemCurrentLight');
  }

  for (const btn of document.querySelectorAll('[data-theme-choice]')) {
    const choice = btn.getAttribute('data-theme-choice');
    btn.setAttribute('aria-checked', choice === active ? 'true' : 'false');
  }
}

function updateLanguageControls() {
  const active = getLocale();
  for (const btn of document.querySelectorAll('[data-locale]')) {
    const loc = btn.getAttribute('data-locale');
    btn.setAttribute('aria-checked', loc === active ? 'true' : 'false');
  }
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
  if (prof.status === 'starting') return { label: t('profiles.state.starting'), variant: 'badge-warn' };
  if (prof.status === 'stopping') return { label: t('profiles.state.stopping'), variant: 'badge-warn' };
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
    bar.textContent = t('statusBar.checking');
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
    const li = document.createElement('li');
    li.className = 'profile-row';

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

    if (!prof.zalo_user_id) {
      titleLine.appendChild(makeBadge(t('profiles.state.loginUnverified'), 'badge-neutral'));
    }

    main.appendChild(titleLine);

    const subParts = [];
    if (prof.notes) subParts.push(prof.notes);
    if (prof.running && prof.main_pid != null) {
      subParts.push(t('profiles.meta.pidSummary', {
        pid: String(prof.main_pid),
        count: formatNumber(prof.process_count ?? 1)
      }));
    }
    if (prof.zalo_user_id) {
      subParts.push(t('profiles.meta.userId', { id: prof.zalo_user_id }));
    }
    if (prof.data_size_bytes != null) {
      subParts.push(t('profiles.meta.disk', { size: formatBytes(prof.data_size_bytes) }));
    }
    if (subParts.length > 0) {
      const subEl = document.createElement('div');
      subEl.className = 'profile-subline';
      subEl.textContent = subParts.join(' · ');
      main.appendChild(subEl);
    }

    const actions = document.createElement('div');
    actions.className = 'profile-actions';

    if (prof.running) {
      actions.appendChild(makeButton(t('profiles.actions.focus'), 'btn-primary', () => runProfileAction('focus', prof)));
      actions.appendChild(makeButton(t('profiles.actions.stop'), 'btn-danger', () => runProfileAction('stop', prof)));
    } else {
      actions.appendChild(makeButton(t('profiles.actions.launch'), 'btn-primary', () => runProfileAction('launch', prof)));
    }

    // Overflow menu (popover)
    const menuId = `menu-${String(prof.id).replace(/[^a-zA-Z0-9_-]/g, '_')}`;
    const menuWrap = document.createElement('div');
    menuWrap.className = 'menu-wrap';

    const moreBtn = document.createElement('button');
    moreBtn.type = 'button';
    moreBtn.className = 'btn btn-secondary btn-sm';
    moreBtn.textContent = '⋯';
    moreBtn.setAttribute('aria-label', t('profiles.actions.more', { name: prof.name }));
    moreBtn.setAttribute('popovertarget', menuId);

    const menuList = document.createElement('div');
    menuList.id = menuId;
    menuList.className = 'overflow-menu';
    menuList.setAttribute('popover', 'auto');
    menuList.setAttribute('role', 'menu');

    const closeMenu = () => {
      if (typeof menuList.hidePopover === 'function') {
        try {
          menuList.hidePopover();
        } catch (_err) {
          // Ignore if already closed
        }
      } else {
        menuList.hidden = true;
      }
    };

    const addMenuItem = (label, onSelect, { disabled = false, danger = false } = {}) => {
      const item = document.createElement('button');
      item.type = 'button';
      item.className = danger ? 'menu-item menu-item-danger' : 'menu-item';
      item.setAttribute('role', 'menuitem');
      item.textContent = label;
      item.disabled = disabled;
      item.addEventListener('click', () => {
        closeMenu();
        onSelect();
      });
      menuList.appendChild(item);
    };

    addMenuItem(t('profiles.actions.rename'), () => openRenameDialog(prof));
    addMenuItem(t('profiles.actions.backup'), () => runProfileAction('backup', prof), { disabled: Boolean(prof.running) });
    addMenuItem(t('profiles.actions.details'), () => openDetailsDialog(prof));
    addMenuItem(t('profiles.actions.delete'), () => openDeleteDialog(prof), { disabled: Boolean(prof.running), danger: true });

    if (!Object.hasOwn(HTMLElement.prototype, 'popover')) {
      menuList.hidden = true;
      moreBtn.addEventListener('click', () => {
        menuList.hidden = !menuList.hidden;
      });
    }

    menuWrap.appendChild(moreBtn);
    menuWrap.appendChild(menuList);
    actions.appendChild(menuWrap);

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
    actTd.appendChild(makeButton(t('backups.restoreButton'), 'btn-primary', () => openRestoreDialog(bk)));
    tr.appendChild(actTd);

    tbody.appendChild(tr);
  }
}

export function resolveIsolationCheckResult(chk) {
  // Never convert NOT_TESTED / BLOCKED / UNKNOWN into PASS
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

  // 1. Application group
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

  // 2. Security & Isolation group
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

  // 3. Profiles group
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

  // 4. Environment & OS Readiness group
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

  // 5. Sanitized Logs group
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

function renderAll() {
  renderStatusBar();
  renderProfiles();
  renderBackups();
  renderDiagnostics();
  renderSettingsAbout();
  updateLanguageControls();
  updateAppearanceControls();
}

export async function refreshAll() {
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
    renderAll();
  } catch (err) {
    updateConnectionBadge(state.connectionMode || 'bridge', false);
    showBanner(t('statusBar.hostError', { message: err.message }), 'error');
    renderSettingsAbout();
  }
}

async function runProfileAction(action, prof) {
  clearBanner();
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
      showBanner(translateErrorCode(res.error_code, res.message), 'error');
    } else if (res?.message) {
      showBanner(res.message, 'success');
    }
    await refreshAll();
  } catch (err) {
    showBanner(err.message, 'error');
  }
}

function openRenameDialog(prof) {
  state.activeRenameId = prof.id;
  document.getElementById('rename-profile-subtitle').textContent = t('dialogs.rename.subtitle', {
    name: prof.name,
    id: prof.id
  });
  document.getElementById('rename-profile-name').value = prof.name;
  document.getElementById('rename-profile-dialog').showModal();
}

function openDeleteDialog(prof) {
  state.activeDeleteId = prof.id;
  document.getElementById('delete-profile-warning').textContent = t('dialogs.delete.warning', {
    name: prof.name,
    dir: prof.directory_name || prof.id
  });
  document.getElementById('delete-profile-dialog').showModal();
}

function openRestoreDialog(bk) {
  state.activeRestoreBackupDir = bk.backup_dir;
  document.getElementById('restore-backup-subtitle').textContent = t('dialogs.restore.subtitle', {
    backupName: bk.backup_name || bk.backup_id || ''
  });
  document.getElementById('restore-profile-name').value = '';
  document.getElementById('restore-backup-dialog').showModal();
}

function openDetailsDialog(prof) {
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

  document.getElementById('profile-details-dialog').showModal();
}

function wireDialogsAndFallbacks() {
  // Progressive enhancement fallback when Invoker Commands (command/commandfor) are unsupported
  if (!('commandForElement' in HTMLButtonElement.prototype)) {
    document.addEventListener('click', (event) => {
      const btn = event.target.closest('button[command][commandfor]');
      if (!btn) return;
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

  // Progressive enhancement fallback when <dialog closedby="any"> is unsupported
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
    if (action !== 'create') return;

    const nameInput = document.getElementById('create-profile-name');
    const notesInput = document.getElementById('create-profile-notes');
    const name = nameInput.value.trim();
    const notes = notesInput.value.trim();
    nameInput.value = '';
    notesInput.value = '';
    if (!name) return;

    const res = await apiCall('/api/profiles', 'POST', { name, notes }, 'create_profile', { name, notes });
    if (res && res.ok === false) showBanner(translateErrorCode(res.error_code, res.message), 'error');
    else showBanner(res?.message || t('messages.profileCreated'), 'success');
    await refreshAll();
  });

  const renameDialog = document.getElementById('rename-profile-dialog');
  renameDialog.addEventListener('close', async () => {
    const action = renameDialog.returnValue;
    renameDialog.returnValue = '';
    const targetId = state.activeRenameId;
    state.activeRenameId = null;
    if (action !== 'rename' || !targetId) return;

    const newName = document.getElementById('rename-profile-name').value.trim();
    if (!newName) return;

    const res = await apiCall(`/api/profiles/${encodeURIComponent(targetId)}/rename`, 'POST', { newName }, 'rename_profile', { id: targetId, newName });
    if (res && res.ok === false) showBanner(translateErrorCode(res.error_code, res.message), 'error');
    else showBanner(res?.message || t('messages.profileRenamed'), 'success');
    await refreshAll();
  });

  const deleteDialog = document.getElementById('delete-profile-dialog');
  deleteDialog.addEventListener('close', async () => {
    const action = deleteDialog.returnValue;
    deleteDialog.returnValue = '';
    const targetId = state.activeDeleteId;
    state.activeDeleteId = null;
    if (action !== 'delete' || !targetId) return;

    const res = await apiCall(`/api/profiles/${encodeURIComponent(targetId)}`, 'DELETE', { confirm: true }, 'delete_profile', { id: targetId });
    if (res && res.ok === false) showBanner(translateErrorCode(res.error_code, res.message), 'error');
    else showBanner(res?.message || t('messages.profileDeleted'), 'success');
    await refreshAll();
  });

  const restoreDialog = document.getElementById('restore-backup-dialog');
  restoreDialog.addEventListener('close', async () => {
    const action = restoreDialog.returnValue;
    restoreDialog.returnValue = '';
    const backupDir = state.activeRestoreBackupDir;
    state.activeRestoreBackupDir = null;
    if (action !== 'restore' || !backupDir) return;

    const name = document.getElementById('restore-profile-name').value.trim();
    const res = await apiCall('/api/backups/restore', 'POST', { backupDir, name }, 'restore_backup', { backupDir, name });
    if (res && res.ok === false) showBanner(translateErrorCode(res.error_code, res.message), 'error');
    else showBanner(res?.message || t('messages.backupRestored'), 'success');
    await refreshAll();
  });

  for (const id of ['profile-details-dialog', 'safety-dialog']) {
    const dlg = document.getElementById(id);
    dlg.addEventListener('close', () => {
      dlg.returnValue = '';
    });
  }
}

export function initApp() {
  applyDomTranslations();
  updateLanguageControls();
  updateAppearanceControls();
  wireDialogsAndFallbacks();

  // Sidebar navigation
  for (const btn of document.querySelectorAll('.nav-item[data-view]')) {
    btn.addEventListener('click', () => {
      selectView(btn.getAttribute('data-view'));
    });
  }

  document.getElementById('btn-goto-diagnostics')?.addEventListener('click', () => {
    selectView('diagnostics');
  });

  // Search & Toolbar
  document.getElementById('profile-search')?.addEventListener('input', (event) => {
    state.searchQuery = event.target.value;
    renderProfiles();
  });

  document.getElementById('btn-refresh')?.addEventListener('click', () => refreshAll());
  document.getElementById('btn-refresh-backups')?.addEventListener('click', () => refreshAll());
  document.getElementById('btn-run-isolation')?.addEventListener('click', () => refreshAll());
  document.getElementById('btn-refresh-diagnostics')?.addEventListener('click', () => refreshAll());
  document.getElementById('toggle-sizes')?.addEventListener('change', () => refreshAll());

  // Settings: Language
  for (const btn of document.querySelectorAll('[data-locale]')) {
    btn.addEventListener('click', () => {
      setLocale(btn.getAttribute('data-locale'));
    });
  }

  onLocaleChange(() => {
    applyDomTranslations();
    renderAll();
  });

  // Settings: Appearance
  for (const btn of document.querySelectorAll('[data-theme-choice]')) {
    btn.addEventListener('click', () => {
      applyThemePreference(btn.getAttribute('data-theme-choice'));
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

  refreshAll();
}

if (typeof document !== 'undefined') {
  initApp();
}
