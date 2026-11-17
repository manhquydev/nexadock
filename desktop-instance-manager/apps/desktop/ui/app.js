// Multi-Zalo Desktop UI Controller (zero third-party runtime dependencies)

const state = {
  status: null,
  backups: [],
  diagnostics: null,
  isolation: null,
  activeRenameId: null,
  activeDeleteId: null,
  activeRestoreBackupDir: null,
  busy: false
};

const tabsContainer = document.getElementById('main-tabs');
const tablist = tabsContainer.querySelector('[focusgroup]');
const tabs = Array.from(tablist.querySelectorAll('[role="tab"]'));
const panels = Array.from(tabsContainer.querySelectorAll('[role="tabpanel"]'));

function selectTab(selectedTab, { focus = false } = {}) {
  for (const tab of tabs) {
    const isSelected = tab === selectedTab;
    tab.setAttribute('aria-selected', isSelected ? 'true' : 'false');
    tab.toggleAttribute('focusgroupstart', isSelected);
    if (!('focusGroup' in HTMLElement.prototype)) {
      tab.tabIndex = isSelected ? 0 : -1;
    }
  }

  for (const panel of panels) {
    const matches = panel.getAttribute('aria-labelledby') === selectedTab.id;
    if (matches) {
      panel.removeAttribute('hidden');
      panel.setAttribute('tabindex', '0');
    } else {
      panel.setAttribute('hidden', 'until-found');
      panel.removeAttribute('tabindex');
    }
  }

  selectedTab.scrollIntoView({ block: 'nearest', inline: 'nearest', container: 'nearest' });
  if (focus) {
    selectedTab.focus();
  }
}

tablist.addEventListener('click', (event) => {
  const tab = event.target.closest('[role="tab"]');
  if (tab) {
    selectTab(tab, { focus: false });
  }
});

// Synchronize active tab when browser find-in-page reveals a hidden="until-found" panel
tabsContainer.addEventListener('beforematch', (event) => {
  const matchedPanel = event.target.closest('[role="tabpanel"]');
  if (!matchedPanel) return;
  const tabId = matchedPanel.getAttribute('aria-labelledby');
  const targetTab = tabs.find((t) => t.id === tabId);
  if (targetTab) {
    selectTab(targetTab, { focus: false });
  }
});

// Progressive enhancement fallback when native focusgroup is unsupported
if (!('focusGroup' in HTMLElement.prototype)) {
  for (const tab of tabs) {
    tab.tabIndex = tab.getAttribute('aria-selected') === 'true' ? 0 : -1;
  }

  tablist.addEventListener('keydown', (event) => {
    const current = event.target.closest('[role="tab"]');
    if (!current) return;
    const idx = tabs.indexOf(current);
    if (idx === -1) return;

    let nextIdx = null;
    if (event.key === 'ArrowRight') nextIdx = (idx + 1) % tabs.length;
    else if (event.key === 'ArrowLeft') nextIdx = (idx - 1 + tabs.length) % tabs.length;
    else if (event.key === 'Home') nextIdx = 0;
    else if (event.key === 'End') nextIdx = tabs.length - 1;

    if (nextIdx !== null) {
      event.preventDefault();
      selectTab(tabs[nextIdx], { focus: true });
    }
  });
}

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
    } else if (cmd === 'close' && target.open) {
      target.close('cancel');
    } else if (cmd === 'request-close' && target.open) {
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

function showBanner(message, kind = 'info') {
  const banner = document.getElementById('status-banner');
  banner.textContent = message;
  banner.className = 'status-banner';
  if (kind === 'error') banner.classList.add('is-error');
  else if (kind === 'success') banner.classList.add('is-success');
  banner.hidden = false;
}

function clearBanner() {
  const banner = document.getElementById('status-banner');
  banner.hidden = true;
  banner.textContent = '';
}

function formatBytes(bytes) {
  if (bytes == null || Number.isNaN(Number(bytes))) return '—';
  const n = Number(bytes);
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function getTauriInvoke() {
  if (window.__TAURI__?.core?.invoke) return window.__TAURI__.core.invoke;
  if (window.__TAURI_INTERNALS__?.invoke) return window.__TAURI_INTERNALS__.invoke;
  return null;
}

function getSessionToken() {
  return document.querySelector('meta[name="dim-session-token"]')?.content || '';
}

function setBusy(isBusy, statusText = null) {
  state.busy = isBusy;
  const controls = document.querySelectorAll('#btn-refresh, #btn-refresh-backups, #btn-run-isolation, .row-actions button, .header-actions button, .toolbar-controls button');
  for (const btn of controls) {
    btn.disabled = isBusy;
  }
  if (isBusy && statusText) {
    showBanner(statusText, 'info');
  }
}

async function apiCall(route, method = 'GET', body = null, tauriCmd = null, tauriArgs = {}) {
  const invoke = getTauriInvoke();
  if (invoke && tauriCmd) {
    document.getElementById('host-mode-badge').textContent = 'Tauri Native Core';
    return await invoke(tauriCmd, tauriArgs);
  }

  document.getElementById('host-mode-badge').textContent = 'Authenticated Local Bridge (127.0.0.1)';
  const headers = { 'Accept': 'application/json' };
  const token = getSessionToken();
  if (token) {
    headers['X-DIM-Session-Token'] = token;
  }
  const init = { method, headers };
  if (body !== null) {
    headers['Content-Type'] = 'application/json';
    init.body = JSON.stringify(body);
  }
  const resp = await fetch(route, init);
  const payload = await resp.json();
  return payload;
}

function makeBadge(text, variant) {
  const span = document.createElement('span');
  span.className = `badge ${variant}`;
  span.textContent = text;
  return span;
}

function makeActionButton(label, className, onClick) {
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = `btn btn-sm ${className}`;
  btn.textContent = label;
  btn.addEventListener('click', onClick);
  return btn;
}

function renderSummary() {
  const st = state.status;
  const diag = state.diagnostics;
  if (!st) return;

  const inst = st.installation || {};
  document.getElementById('stat-app').textContent = inst.valid
    ? `${inst.mode === 'node-fixture' ? 'Node Fixture' : 'Zalo'} ${inst.version || ''}`.trim()
    : 'Not found';
  document.getElementById('stat-app-meta').textContent = inst.authenticode_status || inst.error || 'Unavailable';

  const prim = st.primary_zalo || {};
  document.getElementById('stat-primary').textContent = prim.is_running
    ? `Running (${(prim.locking_pids || []).length} lock PID)`
    : 'Not running';
  document.getElementById('stat-primary-meta').textContent = `Protected · ${formatBytes(prim.total_working_set_bytes || 0)} RAM`;

  const profiles = st.profiles || [];
  const runningCount = profiles.filter((p) => p.running).length;
  document.getElementById('stat-profiles').textContent = `${runningCount} / ${profiles.length} running`;
  document.getElementById('stat-data-dir').textContent = st.data_dir || 'Default data dir';

  if (diag) {
    const elevatedText = diag.is_elevated === true
      ? 'Elevated'
      : (diag.is_elevated === false ? 'Standard User (asInvoker)' : 'Elevation Unknown');
    const sacText = diag.smart_app_control_state === 1
      ? 'SAC Enforced'
      : (diag.smart_app_control_state === 0 ? 'SAC=0 (Off)' : `SAC=${diag.smart_app_control_state ?? 'Unknown'}`);
    document.getElementById('stat-security').textContent = elevatedText;
    document.getElementById('stat-security-meta').textContent = `${sacText} · Disk free ${formatBytes(diag.disk_free_bytes)}`;
  }
}

function renderProfiles() {
  const tbody = document.getElementById('profiles-tbody');
  tbody.replaceChildren();

  const profiles = state.status?.profiles || [];
  if (profiles.length === 0) {
    const tr = document.createElement('tr');
    const th = document.createElement('th');
    th.scope = 'row';
    th.textContent = 'No profiles created';
    tr.appendChild(th);
    for (let i = 0; i < 5; i += 1) {
      const td = document.createElement('td');
      td.textContent = '—';
      tr.appendChild(td);
    }
    tbody.appendChild(tr);
    return;
  }

  for (const prof of profiles) {
    const tr = document.createElement('tr');

    const th = document.createElement('th');
    th.scope = 'row';
    th.textContent = prof.name;
    const sub = document.createElement('span');
    sub.className = 'row-subtext';
    sub.textContent = `${prof.directory_name} (${prof.id})`;
    th.appendChild(sub);
    tr.appendChild(th);

    const statusTd = document.createElement('td');
    if (prof.running) {
      statusTd.appendChild(makeBadge('Running', 'badge-running'));
    } else if (prof.stale_lockfile_detected) {
      statusTd.appendChild(makeBadge('Crash Recoverable', 'badge-warn'));
    } else {
      statusTd.appendChild(makeBadge('Stopped', 'badge-stopped'));
    }
    tr.appendChild(statusTd);

    const pidTd = document.createElement('td');
    pidTd.textContent = prof.running
      ? `PID ${prof.main_pid} · ${prof.process_count} proc(s)`
      : 'Idle';
    if (prof.window_title) {
      const wSub = document.createElement('span');
      wSub.className = 'row-subtext';
      wSub.textContent = prof.window_title;
      pidTd.appendChild(wSub);
    }
    tr.appendChild(pidTd);

    const uidTd = document.createElement('td');
    uidTd.textContent = prof.zalo_user_id || 'Unauthenticated';
    tr.appendChild(uidTd);

    const memTd = document.createElement('td');
    const ramText = prof.running ? `${formatBytes(prof.total_working_set_bytes)} RAM` : '0 B RAM';
    const diskText = prof.data_size_bytes != null ? ` · ${formatBytes(prof.data_size_bytes)} disk` : '';
    memTd.textContent = `${ramText}${diskText}`;
    tr.appendChild(memTd);

    const actionsTd = document.createElement('td');
    const actionBox = document.createElement('div');
    actionBox.className = 'row-actions';

    if (prof.running) {
      actionBox.appendChild(makeActionButton('Focus', 'btn-secondary', () => runProfileAction('focus', prof)));
      actionBox.appendChild(makeActionButton('Stop', 'btn-danger', () => runProfileAction('stop', prof)));
    } else {
      actionBox.appendChild(makeActionButton('Launch', 'btn-primary', () => runProfileAction('launch', prof)));
      actionBox.appendChild(makeActionButton('Backup', 'btn-secondary', () => runProfileAction('backup', prof)));
    }

    actionBox.appendChild(makeActionButton('Rename', 'btn-secondary', () => openRenameDialog(prof)));
    if (!prof.running) {
      actionBox.appendChild(makeActionButton('Delete', 'btn-secondary', () => openDeleteDialog(prof)));
    }

    actionsTd.appendChild(actionBox);
    tr.appendChild(actionsTd);
    tbody.appendChild(tr);
  }
}

function renderBackups() {
  const tbody = document.getElementById('backups-tbody');
  tbody.replaceChildren();

  const backups = state.backups || [];
  if (backups.length === 0) {
    const tr = document.createElement('tr');
    const th = document.createElement('th');
    th.scope = 'row';
    th.textContent = 'No offline backups found';
    tr.appendChild(th);
    for (let i = 0; i < 4; i += 1) {
      const td = document.createElement('td');
      td.textContent = '—';
      tr.appendChild(td);
    }
    tbody.appendChild(tr);
    return;
  }

  for (const bk of backups) {
    const tr = document.createElement('tr');
    const th = document.createElement('th');
    th.scope = 'row';
    th.textContent = bk.backup_name;
    tr.appendChild(th);

    const srcTd = document.createElement('td');
    const srcName = bk.source_profile_name || bk.source_profile?.name || 'Unknown';
    const srcId = bk.source_profile_id || bk.source_profile?.id || '';
    srcTd.textContent = srcId ? `${srcName} (${srcId})` : srcName;
    tr.appendChild(srcTd);

    const sizeTd = document.createElement('td');
    sizeTd.textContent = `${bk.file_count ?? 0} files · ${formatBytes(bk.total_bytes ?? 0)}`;
    tr.appendChild(sizeTd);

    const timeTd = document.createElement('td');
    timeTd.textContent = bk.created_at || '—';
    tr.appendChild(timeTd);

    const actTd = document.createElement('td');
    actTd.appendChild(makeActionButton('Restore as New', 'btn-primary', () => openRestoreDialog(bk)));
    tr.appendChild(actTd);

    tbody.appendChild(tr);
  }
}

function renderDiagnostics() {
  const tbody = document.getElementById('diagnostics-tbody');
  tbody.replaceChildren();

  const checks = state.isolation?.checks || [];
  for (const chk of checks) {
    const tr = document.createElement('tr');
    const th = document.createElement('th');
    th.scope = 'row';
    th.textContent = chk.check_name;
    tr.appendChild(th);

    const status = chk.status || (chk.passed ? 'PASS' : 'FAIL');
    const badgeClass = status === 'PASS' ? 'badge-pass'
      : (status === 'BLOCKED' ? 'badge-warn'
        : (status === 'NOT_TESTED' ? 'badge-neutral' : 'badge-fail'));
    const resTd = document.createElement('td');
    resTd.appendChild(makeBadge(status, badgeClass));
    tr.appendChild(resTd);

    const detTd = document.createElement('td');
    detTd.textContent = chk.detail || '';
    tr.appendChild(detTd);

    tbody.appendChild(tr);
  }

  const logBox = document.getElementById('sanitized-log-output');
  const logs = state.diagnostics?.sanitized_logs || [];
  logBox.textContent = logs.length > 0 ? logs.join('\n') : 'No diagnostic log events yet.';
}

function renderReadiness() {
  const tbody = document.getElementById('readiness-tbody');
  tbody.replaceChildren();

  const matrix = state.diagnostics?.platform_support_matrix || [];
  for (const row of matrix) {
    const tr = document.createElement('tr');
    const th = document.createElement('th');
    th.scope = 'row';
    th.textContent = row.platform;
    tr.appendChild(th);

    const stTd = document.createElement('td');
    const variant = row.status === 'SUPPORTED_AND_TESTED' ? 'badge-pass'
      : (row.status === 'PARTIALLY_SUPPORTED' ? 'badge-warn'
        : (row.status === 'UNSUPPORTED' ? 'badge-fail' : 'badge-neutral'));
    stTd.appendChild(makeBadge(row.status, variant));
    tr.appendChild(stTd);

    const stratTd = document.createElement('td');
    stratTd.textContent = row.strategy;
    tr.appendChild(stratTd);

    const limTd = document.createElement('td');
    limTd.textContent = row.limits;
    tr.appendChild(limTd);

    tbody.appendChild(tr);
  }
}

async function refreshAll({ quiet = false } = {}) {
  try {
    const withSize = document.getElementById('toggle-sizes').checked;
    const [statusData, backupsData, diagData, isoData] = await Promise.all([
      apiCall(`/api/status${withSize ? '?withSize=1' : ''}`, 'GET', null, 'get_status', { withSize }),
      apiCall('/api/backups', 'GET', null, 'list_backups', {}),
      apiCall('/api/diagnostics', 'GET', null, 'get_diagnostics', {}),
      apiCall('/api/verify-isolation', 'GET', null, 'verify_isolation', {})
    ]);
    if (statusData?.ok === false) {
      showBanner(`${statusData.error_code || 'ERROR'}: ${statusData.message}`, 'error');
      return;
    }
    state.status = statusData;
    state.backups = backupsData?.backups || [];
    state.diagnostics = diagData;
    state.isolation = isoData;

    renderSummary();
    renderProfiles();
    renderBackups();
    renderDiagnostics();
    renderReadiness();
    if (!quiet) {
      // Preserve explicit action feedback when invoked after a mutation
    }
  } catch (err) {
    showBanner(`Host communication failed: ${err.message}`, 'error');
  }
}

async function runProfileAction(action, prof) {
  if (state.busy) return;
  clearBanner();
  setBusy(true, `Running ${action} for "${prof.name}"...`);
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
      showBanner(`${res.error_code || 'ERROR'}: ${res.message}`, 'error');
    } else if (res?.message) {
      showBanner(res.message, 'success');
    }
  } catch (err) {
    showBanner(err.message, 'error');
  } finally {
    setBusy(false);
  }
}

// Dialog wiring with mandatory dialog.returnValue read + reset
const createDialog = document.getElementById('create-profile-dialog');
createDialog.addEventListener('close', async () => {
  const action = createDialog.returnValue;
  createDialog.returnValue = '';
  if (action !== 'create' || state.busy) return;

  const nameInput = document.getElementById('create-profile-name');
  const notesInput = document.getElementById('create-profile-notes');
  const name = nameInput.value.trim();
  const notes = notesInput.value.trim();
  nameInput.value = '';
  notesInput.value = '';
  if (!name) return;

  setBusy(true, `Creating profile "${name}"...`);
  try {
    const res = await apiCall('/api/profiles', 'POST', { name, notes }, 'create_profile', { name, notes });
    await refreshAll({ quiet: true });
    showBanner(res.message || 'Profile created', res.ok ? 'success' : 'error');
  } catch (err) {
    showBanner(err.message, 'error');
  } finally {
    setBusy(false);
  }
});

const renameDialog = document.getElementById('rename-profile-dialog');
function openRenameDialog(prof) {
  state.activeRenameId = prof.id;
  document.getElementById('rename-profile-subtitle').textContent = `Current name: ${prof.name} (${prof.id})`;
  document.getElementById('rename-profile-name').value = prof.name;
  renameDialog.showModal();
}
renameDialog.addEventListener('close', async () => {
  const action = renameDialog.returnValue;
  renameDialog.returnValue = '';
  const targetId = state.activeRenameId;
  state.activeRenameId = null;
  if (action !== 'rename' || !targetId || state.busy) return;

  const newName = document.getElementById('rename-profile-name').value.trim();
  if (!newName) return;

  setBusy(true, `Renaming profile to "${newName}"...`);
  try {
    const res = await apiCall(`/api/profiles/${encodeURIComponent(targetId)}/rename`, 'POST', { newName }, 'rename_profile', { id: targetId, newName });
    await refreshAll({ quiet: true });
    showBanner(res.message || 'Renamed profile', res.ok ? 'success' : 'error');
  } catch (err) {
    showBanner(err.message, 'error');
  } finally {
    setBusy(false);
  }
});

const deleteDialog = document.getElementById('delete-profile-dialog');
function openDeleteDialog(prof) {
  state.activeDeleteId = prof.id;
  document.getElementById('delete-profile-warning').textContent =
    `Delete profile "${prof.name}" (${prof.directory_name})? Only isolated profile files are deleted; primary Zalo data is untouched.`;
  deleteDialog.showModal();
}
deleteDialog.addEventListener('close', async () => {
  const action = deleteDialog.returnValue;
  deleteDialog.returnValue = '';
  const targetId = state.activeDeleteId;
  state.activeDeleteId = null;
  if (action !== 'delete' || !targetId || state.busy) return;

  setBusy(true, 'Deleting isolated profile...');
  try {
    const res = await apiCall(`/api/profiles/${encodeURIComponent(targetId)}`, 'DELETE', { confirm: true }, 'delete_profile', { id: targetId, confirm: true });
    await refreshAll({ quiet: true });
    showBanner(res.message || 'Deleted profile', res.ok ? 'success' : 'error');
  } catch (err) {
    showBanner(err.message, 'error');
  } finally {
    setBusy(false);
  }
});

const restoreDialog = document.getElementById('restore-backup-dialog');
function openRestoreDialog(bk) {
  state.activeRestoreBackupDir = bk.backup_dir;
  document.getElementById('restore-backup-subtitle').textContent =
    `Verify SHA-256 manifest in "${bk.backup_name}" and restore into a brand-new isolated profile.`;
  document.getElementById('restore-profile-name').value = '';
  restoreDialog.showModal();
}
restoreDialog.addEventListener('close', async () => {
  const action = restoreDialog.returnValue;
  restoreDialog.returnValue = '';
  const backupDir = state.activeRestoreBackupDir;
  state.activeRestoreBackupDir = null;
  if (action !== 'restore' || !backupDir || state.busy) return;

  const name = document.getElementById('restore-profile-name').value.trim();
  setBusy(true, 'Verifying SHA-256 manifest and restoring backup...');
  try {
    const res = await apiCall('/api/backups/restore', 'POST', { backupDir, name }, 'restore_backup', { backupDir, name });
    await refreshAll({ quiet: true });
    showBanner(res.message || 'Backup restored', res.ok ? 'success' : 'error');
  } catch (err) {
    showBanner(err.message, 'error');
  } finally {
    setBusy(false);
  }
});

const safetyDialog = document.getElementById('safety-dialog');
safetyDialog.addEventListener('close', () => {
  const _action = safetyDialog.returnValue;
  safetyDialog.returnValue = '';
});

document.addEventListener('keydown', (event) => {
  if (event.isComposing || state.busy || document.querySelector('dialog[open]')) return;
  const tag = event.target?.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA') return;
  if (event.altKey && !event.ctrlKey && !event.metaKey) {
    const key = event.key.toLowerCase();
    if (key === 'n') {
      event.preventDefault();
      createDialog.showModal();
    } else if (key === 'r') {
      event.preventDefault();
      refreshAll();
    }
  }
});

document.getElementById('btn-refresh').addEventListener('click', () => refreshAll());
document.getElementById('btn-refresh-backups').addEventListener('click', () => refreshAll());
document.getElementById('btn-run-isolation').addEventListener('click', () => refreshAll());
document.getElementById('toggle-sizes').addEventListener('change', () => refreshAll());

refreshAll();
