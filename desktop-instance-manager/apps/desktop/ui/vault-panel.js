//! Telegram Vault panel for the account-details dialog.
//!
//! Calm Precision: DOM is built with createElement/textContent only, colors
//! reuse existing CSS tokens, and every visible string resolves through the
//! vi-VN/en-US dictionaries. The app calls window.__nexadockVault.render
//! with the profile each time the details dialog is populated.

import { t, formatDateTime } from './i18n/index.js';

const STATE_KEYS = {
  unpaired: 'vault.states.unpaired',
  paired: 'vault.states.paired',
  syncing: 'vault.states.syncing',
  offline: 'vault.states.offline',
};

function getInvoke() {
  if (typeof window === 'undefined') return null;
  return window.__TAURI_INTERNALS__?.invoke || null;
}

function make(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** Resolves the panel lifecycle state key from a status envelope payload. */
export function stateFor(status) {
  if (!status || !status.paired) return 'unpaired';
  if (status.engineRunning && (status.uploading || 0) > 0) return 'syncing';
  return status.enabled ? 'paired' : 'offline';
}

/** Stable identity used by tests to locate panel nodes. */
export const PANEL_ID = 'vault-panel';

let panelId = '';
let section = null;

function showError(message) {
  const node = section?.querySelector('.vault-error');
  if (!node) return;
  node.hidden = !message;
  node.textContent = message || '';
}

function setLoginStep(step) {
  const phoneForm = section?.querySelector('.vault-form-phone');
  const codeForm = section?.querySelector('.vault-form-code');
  if (phoneForm) phoneForm.hidden = step !== 'phone';
  if (codeForm) codeForm.hidden = step !== 'code';
}

function refresh() {
  const invoke = getInvoke();
  if (!invoke || !panelId || !section) return;
  invoke('vault_get_status', { profileId: panelId })
    .then((envelope) => {
      if (!envelope?.ok) {
        applyStatus(null, envelope?.message || t('vault.errors.requestFailed'));
        return null;
      }
      applyStatus(envelope.data, '');
      return invoke('vault_get_log', { profileId: panelId, limit: 5 });
    })
    .then((logEnvelope) => {
      if (logEnvelope?.ok) renderLog(logEnvelope.data || []);
    })
    .catch(() => applyStatus(null, t('vault.errors.requestFailed')));
}

function run(command, args) {
  const invoke = getInvoke();
  if (!invoke || !panelId) return;
  invoke(command, { profileId: panelId, ...args })
    .then((envelope) => {
      if (!envelope?.ok) {
        showError(envelope?.message || t('vault.errors.requestFailed'));
      } else {
        showError('');
      }
      refresh();
    })
    .catch(() => showError(t('vault.errors.requestFailed')));
}

/** Applies one status snapshot (or null) to the rendered panel. */
export function applyStatus(status, message) {
  if (!section) return;
  showError(message);
  const state = stateFor(status);
  const stateNode = section.querySelector('.vault-state');
  if (stateNode) stateNode.textContent = t(STATE_KEYS[state]);
  const statsNode = section.querySelector('.vault-stats');
  if (statsNode) {
    statsNode.textContent = status?.paired
      ? t('vault.stats.summary', {
        pending: String(status.pending || 0),
        failed: String(status.failed || 0),
        done: String(status.done || 0),
      })
      : '';
  }
  const actions = section.querySelector('.vault-actions');
  if (!actions) return;
  actions.replaceChildren();
  if (state === 'unpaired') {
    const pairButton = make('button', 'btn btn-primary', t('vault.actions.pair'));
    pairButton.type = 'button';
    pairButton.addEventListener('click', () => {
      const invoke = getInvoke();
      if (!invoke || !panelId) return;
      invoke('vault_pair', { profileId: panelId })
        .then((envelope) => {
          if (!envelope?.ok) {
            showError(envelope?.message || t('vault.errors.requestFailed'));
            return;
          }
          if (envelope.data?.requiresLogin) setLoginStep('phone');
          refresh();
        })
        .catch(() => showError(t('vault.errors.requestFailed')));
    });
    actions.appendChild(pairButton);
    return;
  }
  const syncButton = make('button', 'btn btn-secondary', t('vault.actions.syncNow'));
  syncButton.type = 'button';
  syncButton.addEventListener('click', () => run('vault_sync_now', {}));
  const toggleButton = make(
    'button',
    'btn btn-secondary',
    status.enabled ? t('vault.actions.disable') : t('vault.actions.enable')
  );
  toggleButton.type = 'button';
  toggleButton.addEventListener('click', () => run('vault_set_enabled', { enabled: !status.enabled }));
  const deleteLabel = make('label', 'vault-delete-session');
  const checkbox = document.createElement('input');
  checkbox.type = 'checkbox';
  checkbox.className = 'vault-delete-session-check';
  deleteLabel.appendChild(checkbox);
  deleteLabel.appendChild(make('span', undefined, t('vault.actions.deleteSession')));
  const unpairButton = make('button', 'btn btn-subtle-danger', t('vault.actions.unpair'));
  unpairButton.type = 'button';
  unpairButton.addEventListener('click', () => {
    run('vault_unpair', { deleteSession: Boolean(checkbox.checked) });
  });
  actions.append(syncButton, toggleButton, unpairButton, deleteLabel);
}

function renderLog(rows) {
  const body = section?.querySelector('.vault-log-body');
  const empty = section?.querySelector('.vault-log-empty');
  if (!body || !empty) return;
  body.replaceChildren();
  empty.hidden = rows.length > 0;
  for (const row of rows) {
    const tr = document.createElement('tr');
    const fileCell = document.createElement('td');
    const baseName = String(row.file).split(/[\\/]/).pop() || String(row.file);
    fileCell.textContent = baseName;
    fileCell.title = row.file;
    const timeCell = document.createElement('td');
    timeCell.textContent = formatDateTime(new Date(row.ts * 1000).toISOString());
    const resultCell = document.createElement('td');
    const ok = row.outcome === 'done';
    resultCell.textContent = ok ? t('vault.log.done') : t('vault.log.failed');
    if (!ok && row.error) resultCell.title = row.error;
    tr.append(fileCell, timeCell, resultCell);
    body.appendChild(tr);
  }
}

function buildForms() {
  const hint = make('p', 'vault-hint', t('vault.login.credentialsHint'));

  const phoneForm = document.createElement('form');
  phoneForm.className = 'vault-form vault-form-phone';
  phoneForm.hidden = true;
  const phoneLabel = make('label', undefined, t('vault.login.phoneLabel'));
  phoneLabel.setAttribute('for', 'vault-phone-input');
  const phoneInput = document.createElement('input');
  phoneInput.type = 'tel';
  phoneInput.id = 'vault-phone-input';
  phoneInput.className = 'vault-input';
  phoneInput.placeholder = t('vault.login.phonePlaceholder');
  phoneInput.autocomplete = 'tel';
  phoneInput.required = true;
  const phoneSubmit = make('button', 'btn btn-primary', t('vault.login.phoneSubmit'));
  phoneSubmit.type = 'submit';
  phoneForm.append(phoneLabel, phoneInput, phoneSubmit);
  phoneForm.addEventListener('submit', (event) => {
    event.preventDefault();
    const invoke = getInvoke();
    if (!invoke || !panelId) return;
    invoke('vault_login_phone', { profileId: panelId, phone: phoneInput.value.trim() })
      .then((envelope) => {
        if (envelope?.ok) setLoginStep('code');
        else showError(envelope?.message || t('vault.errors.requestFailed'));
      })
      .catch(() => showError(t('vault.errors.requestFailed')));
  });

  const codeForm = document.createElement('form');
  codeForm.className = 'vault-form vault-form-code';
  codeForm.hidden = true;
  const codeLabel = make('label', undefined, t('vault.login.codeLabel'));
  codeLabel.setAttribute('for', 'vault-code-input');
  const codeInput = document.createElement('input');
  codeInput.type = 'text';
  codeInput.id = 'vault-code-input';
  codeInput.className = 'vault-input';
  codeInput.inputMode = 'numeric';
  codeInput.autocomplete = 'one-time-code';
  codeInput.placeholder = t('vault.login.codePlaceholder');
  codeInput.required = true;
  const passwordLabel = make('label', undefined, t('vault.login.passwordLabel'));
  passwordLabel.setAttribute('for', 'vault-password-input');
  const passwordInput = document.createElement('input');
  passwordInput.type = 'password';
  passwordInput.id = 'vault-password-input';
  passwordInput.className = 'vault-input';
  passwordInput.autocomplete = 'current-password';
  const codeSubmit = make('button', 'btn btn-primary', t('vault.login.codeSubmit'));
  codeSubmit.type = 'submit';
  codeForm.append(codeLabel, codeInput, passwordLabel, passwordInput, codeSubmit);
  codeForm.addEventListener('submit', (event) => {
    event.preventDefault();
    const invoke = getInvoke();
    if (!invoke || !panelId) return;
    const password = passwordInput.value;
    invoke('vault_login_code', {
      profileId: panelId,
      code: codeInput.value.trim(),
      password: password ? password : null,
    })
      .then((envelope) => {
        if (envelope?.ok) {
          setLoginStep('');
          passwordInput.value = '';
        } else {
          showError(envelope?.message || t('vault.errors.requestFailed'));
        }
        refresh();
      })
      .catch(() => showError(t('vault.errors.requestFailed')));
  });
  return [hint, phoneForm, codeForm];
}

function buildLogTable() {
  const heading = make('h4', 'vault-log-heading', t('vault.log.heading'));
  const table = document.createElement('table');
  table.className = 'vault-log-table';
  const caption = document.createElement('caption');
  caption.textContent = t('vault.log.caption');
  table.appendChild(caption);
  const thead = document.createElement('thead');
  const headRow = document.createElement('tr');
  for (const key of ['vault.log.colFile', 'vault.log.colTime', 'vault.log.colResult']) {
    const th = document.createElement('th');
    th.scope = 'col';
    th.textContent = t(key);
    headRow.appendChild(th);
  }
  thead.appendChild(headRow);
  const tbody = document.createElement('tbody');
  tbody.className = 'vault-log-body';
  table.append(thead, tbody);
  const empty = make('p', 'vault-log-empty', t('vault.log.empty'));
  empty.hidden = true;
  return [heading, table, empty];
}

function buildSection() {
  const previous = document.getElementById(PANEL_ID);
  if (previous) previous.remove();
  const node = make('section', 'vault-panel');
  node.id = PANEL_ID;
  const heading = make('h3', 'vault-heading', t('vault.title'));
  const stateNode = make('strong', 'vault-state');
  const statsNode = make('span', 'vault-stats');
  const stateLine = make('p', 'vault-state-line');
  stateLine.append(stateNode, statsNode);
  const errorNode = make('p', 'vault-error');
  errorNode.setAttribute('role', 'alert');
  errorNode.hidden = true;
  const actions = make('div', 'vault-actions');
  node.append(heading, stateLine, errorNode, actions, ...buildForms(), ...buildLogTable());
  return node;
}

/** Renders the vault panel into the details dialog for one profile. */
export function render(profile) {
  if (typeof document === 'undefined' || !profile?.id) return;
  panelId = profile.id;
  const dialog = document.getElementById('profile-details-dialog');
  if (!dialog) return;
  section = buildSection();
  const anchor = dialog.querySelector('form[method="dialog"]');
  if (anchor?.parentElement) {
    dialog.insertBefore(section, anchor.parentElement === dialog ? anchor : anchor.parentElement);
  } else {
    dialog.appendChild(section);
  }
  refresh();
}

const api = { render, applyStatus, refresh };

if (typeof window !== 'undefined') {
  window.__nexadockVault = api;
}

export default api;
