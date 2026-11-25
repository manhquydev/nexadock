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
  'bot-continuation': 'vault.states.continuingBot',
  'session-missing': 'vault.states.sessionMissing',
};

function getInvoke() {
  if (typeof window === 'undefined') return null;
  return window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke || null;
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
  if (status.transport === 'bot' && status.botConfigured === false) return 'bot-continuation';
  if (status.transport !== 'bot' && status.sessionExists === false) return 'session-missing';
  if (status.engineRunning && (status.uploading || 0) > 0) return 'syncing';
  return status.enabled ? 'paired' : 'offline';
}

/** Stable identity used by tests to locate panel nodes. */
export const PANEL_ID = 'vault-panel';

let panelId = '';
let section = null;
let renderGeneration = 0;
let refreshGeneration = 0;

function isStale(gen, activeProfileId, rGen = null) {
  if (typeof document === 'undefined') return true;
  if (gen !== renderGeneration || panelId !== activeProfileId) return true;
  if (rGen !== null && rGen !== refreshGeneration) return true;
  return false;
}

// Pairing flow state: one tier per pairing — Telegram bot (default) or the
// advanced full-account login. Reset per profile in render().
let pairMode = 'bot';
let loginStep = '';
let botStep = 'token';
let botLink = '';
let botUsername = '';
let botWaiting = false;
let lastState = 'unpaired';
let lastStatus = null;
let lastMessage = '';

/** Mirrors a validation failure to assistive tech: aria-invalid plus a
 *  describedby target ('vault-error-msg'); restores the input's hint on clear. */
function setInvalid(input, on) {
  if (!input) return;
  if (on) {
    input.setAttribute('aria-invalid', 'true');
    input.setAttribute('aria-describedby', 'vault-error-msg');
  } else {
    input.removeAttribute('aria-invalid');
    const hintId = input.getAttribute('data-hint-id');
    if (hintId) input.setAttribute('aria-describedby', hintId);
    else input.removeAttribute('aria-describedby');
  }
}
/** Adds a native password visibility toggle while restoring focus to the input. */
function addPasswordToggle(input, button, showLabel, hideLabel) {
  button.type = 'button';
  button.setAttribute('aria-pressed', 'false');
  button.textContent = showLabel;
  button.addEventListener('click', () => {
    const visible = input.type === 'password';
    input.type = visible ? 'text' : 'password';
    button.setAttribute('aria-pressed', String(visible));
    button.textContent = visible ? hideLabel : showLabel;
    input.focus?.();
  });
}

function showError(message) {
  const node = section?.querySelector('.vault-error');
  if (!node) return;
  node.hidden = !message;
  node.textContent = message || '';
}

function setLoginStep(step) {
  loginStep = step;
  updatePairUi();
}

/** Shows exactly one pairing surface: mode chooser, bot form, bot link, or the advanced login. */
function updatePairUi() {
  const continuation = lastState === 'bot-continuation';
  const reauth = lastState === 'session-missing';
  const unpaired = lastState === 'unpaired';
  const chooser = section?.querySelector('.vault-mode-chooser');
  if (chooser) chooser.hidden = !unpaired;
  const botForm = section?.querySelector('.vault-form-bot');
  if (botForm) botForm.hidden = !(unpaired && pairMode === 'bot' && botStep === 'token');
  const botLinkNode = section?.querySelector('.vault-bot-link');
  if (botLinkNode) {
    botLinkNode.hidden = !(continuation || (unpaired && pairMode === 'bot' && botStep === 'link'));
    const anchor = botLinkNode.querySelector('.vault-bot-link-anchor');
    const username = continuation ? lastStatus?.botUsername : botUsername;
    const link = continuation
      ? (lastStatus?.botDeepLink || (username ? `https://t.me/${username}` : ''))
      : botLink;
    if (anchor) {
      anchor.hidden = !link;
      anchor.setAttribute('href', link || '');
      anchor.textContent = username || link || '';
    }
    const heading = botLinkNode.querySelector('.vault-bot-link-heading');
    if (heading) heading.textContent = t(continuation ? 'vault.bot.continuationHeading' : 'vault.bot.linkHeading');
    const startHint = botLinkNode.querySelector('.vault-bot-start-hint');
    if (startHint) startHint.textContent = t(continuation ? 'vault.bot.continuationHint' : 'vault.bot.startHint');
    const waitButton = botLinkNode.querySelector('.vault-bot-wait');
    if (waitButton && !botWaiting) {
      waitButton.textContent = t(continuation ? 'vault.bot.continuationConfirm' : 'vault.bot.startConfirm');
    }
  }
  const mtprotoVisible = (unpaired && pairMode === 'mtproto') || reauth;
  const hint = section?.querySelector('.vault-hint');
  if (hint) hint.hidden = !mtprotoVisible;
  const phoneForm = section?.querySelector('.vault-form-phone');
  if (phoneForm) {
    phoneForm.hidden = !(
      (unpaired && pairMode === 'mtproto' && loginStep === 'phone')
      || (reauth && loginStep !== 'code')
    );
  }
  const codeForm = section?.querySelector('.vault-form-code');
  if (codeForm) codeForm.hidden = !(mtprotoVisible && loginStep === 'code');
}

function refresh() {
  const invoke = getInvoke();
  if (!invoke || !panelId || !section) return;
  const gen = renderGeneration;
  const activeProfileId = panelId;
  const rGen = ++refreshGeneration;
  invoke('vault_get_status', { profileId: activeProfileId })
    .then((envelope) => {
      if (isStale(gen, activeProfileId, rGen)) return;
      if (!envelope?.ok) {
        applyStatus(null, envelope?.message || t('vault.errors.requestFailed'));
        return;
      }
      applyStatus(envelope.data, '');
      invoke('vault_get_log', { profileId: activeProfileId, limit: 5 })
        .then((logEnvelope) => {
          if (isStale(gen, activeProfileId, rGen)) return;
          if (logEnvelope?.ok) renderLog(logEnvelope.data || []);
        })
        .catch(() => {
          // Keep the loaded paired status intact if log retrieval fails.
        });
    })
    .catch(() => {
      if (isStale(gen, activeProfileId, rGen)) return;
      applyStatus(null, t('vault.errors.requestFailed'));
    });
}

function run(command, args) {
  const invoke = getInvoke();
  if (!invoke || !panelId) return;
  const gen = renderGeneration;
  const activeProfileId = panelId;
  invoke(command, { profileId: activeProfileId, ...args })
    .then((envelope) => {
      if (isStale(gen, activeProfileId)) return;
      if (!envelope?.ok) {
        showError(envelope?.message || t('vault.errors.requestFailed'));
        return;
      }
      showError('');
      refresh();
    })
    .catch(() => {
      if (isStale(gen, activeProfileId)) return;
      showError(t('vault.errors.requestFailed'));
    });
}

/** Applies one status snapshot (or null) to the rendered panel. */
export function applyStatus(status, message) {
  if (!section || !section.querySelector('.vault-state')) return;
  lastStatus = status;
  lastMessage = message || '';
  showError(message);
  const state = stateFor(status);
  lastState = state;
  if (state === 'session-missing') {
    pairMode = 'mtproto';
  }
  const stateNode = section.querySelector('.vault-state');
  if (stateNode) stateNode.textContent = t(STATE_KEYS[state]);
  const transportNode = section.querySelector('.vault-transport');
  if (transportNode) {
    transportNode.hidden = !status?.paired;
    transportNode.textContent = status?.paired
      ? t(status?.transport === 'bot' ? 'vault.transport.bot' : 'vault.transport.mtproto')
      : '';
  }
  updatePairUi();
  const credentialsFields = section.querySelector('.vault-credentials-fields');
  if (credentialsFields) credentialsFields.hidden = Boolean(status?.credentialsAvailable);
  const apiIdInput = section.querySelector('.vault-api-id');
  const apiHashInput = section.querySelector('.vault-api-hash');
  if (apiIdInput) apiIdInput.required = !status?.credentialsAvailable;
  if (apiHashInput) apiHashInput.required = !status?.credentialsAvailable;
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
    if (pairMode === 'mtproto') {
      const pairButton = make('button', 'btn btn-primary', t('vault.actions.pair'));
      pairButton.type = 'button';
      pairButton.addEventListener('click', () => {
        const invoke = getInvoke();
        if (!invoke || !panelId) return;
        const gen = renderGeneration;
        const activeProfileId = panelId;
        invoke('vault_pair', { profileId: activeProfileId })
          .then((envelope) => {
            if (isStale(gen, activeProfileId)) return;
            if (!envelope?.ok) {
              showError(envelope?.message || t('vault.errors.requestFailed'));
              return;
            }
            if (envelope.data?.requiresLogin) setLoginStep('phone');
            refresh();
          })
          .catch(() => {
            if (isStale(gen, activeProfileId)) return;
            showError(t('vault.errors.requestFailed'));
          });
      });
      actions.appendChild(pairButton);
    }
    return;
  }
  if (state === 'bot-continuation' || state === 'session-missing') {
    const unpairButton = make('button', 'btn btn-subtle-danger', t('vault.actions.unpair'));
    unpairButton.type = 'button';
    unpairButton.addEventListener('click', () => {
      run('vault_unpair', { deleteSession: false });
    });
    actions.appendChild(unpairButton);
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
  let deleteLabel = null;
  let checkbox = null;
  if (status.transport !== 'bot') {
    deleteLabel = make('label', 'vault-delete-session');
    checkbox = document.createElement('input');
    checkbox.type = 'checkbox';
    checkbox.className = 'vault-delete-session-check';
    deleteLabel.appendChild(checkbox);
    deleteLabel.appendChild(make('span', undefined, t('vault.actions.deleteSession')));
  }
  const unpairButton = make('button', 'btn btn-subtle-danger', t('vault.actions.unpair'));
  unpairButton.type = 'button';
  unpairButton.addEventListener('click', () => {
    run('vault_unpair', { deleteSession: Boolean(checkbox?.checked) });
  });
  actions.append(syncButton, toggleButton, unpairButton);
  if (deleteLabel) actions.appendChild(deleteLabel);
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

function buildPairModeChooser() {
  const chooser = document.createElement('fieldset');
  chooser.className = 'vault-mode-chooser';
  chooser.appendChild(make('legend', 'vault-mode-legend', t('vault.mode.legend')));
  const options = [
    {
      value: 'bot',
      optionClass: 'vault-mode-option-bot',
      radioClass: 'vault-mode-radio-bot',
      titleKey: 'vault.mode.botTitle',
      descKey: 'vault.mode.botDesc',
    },
    {
      value: 'mtproto',
      optionClass: 'vault-mode-option-mtp',
      radioClass: 'vault-mode-radio-mtp',
      titleKey: 'vault.mode.mtpTitle',
      descKey: 'vault.mode.mtpDesc',
    },
  ];
  for (const option of options) {
    const label = make('label', `vault-mode-option ${option.optionClass}`);
    const radio = document.createElement('input');
    radio.type = 'radio';
    radio.name = 'vault-pair-mode';
    radio.value = option.value;
    radio.className = option.radioClass;
    radio.checked = pairMode === option.value;
    radio.addEventListener('change', () => {
      if (!radio.checked) return;
      pairMode = option.value;
      botStep = 'token';
      loginStep = '';
      // The actions row depends on the tier (pair button only for the advanced tier).
      applyStatus(lastStatus, lastMessage);
    });
    label.append(radio, make('span', 'vault-mode-name', t(option.titleKey)), make('span', 'vault-mode-desc', t(option.descKey)));
    chooser.appendChild(label);
  }
  return [chooser];
}

function buildBotForms() {
  const tokenForm = document.createElement('form');
  tokenForm.className = 'vault-form vault-form-bot';
  tokenForm.hidden = true;
  const tokenLabel = make('label', undefined, t('vault.bot.tokenLabel'));
  tokenLabel.setAttribute('for', 'vault-bot-token');
  const tokenInput = document.createElement('input');
  tokenInput.type = 'password';
  tokenInput.id = 'vault-bot-token';
  tokenInput.className = 'vault-input vault-bot-token';
  tokenInput.autocomplete = 'new-password';
  tokenInput.spellcheck = false;
  tokenInput.setAttribute('data-hint-id', 'vault-bot-token-hint');
  tokenInput.setAttribute('aria-describedby', 'vault-bot-token-hint');
  tokenInput.placeholder = t('vault.bot.tokenPlaceholder');
  tokenInput.required = true;
  const tokenHint = make('p', 'vault-bot-hint', t('vault.bot.tokenHint'));
  tokenHint.setAttribute('id', 'vault-bot-token-hint');
  const tokenVisibility = make('button', 'btn btn-subtle vault-password-toggle');
  addPasswordToggle(tokenInput, tokenVisibility, t('vault.bot.show'), t('vault.bot.hide'));
  const pairButton = make('button', 'btn btn-primary vault-bot-pair-button', t('vault.bot.pairButton'));
  pairButton.type = 'submit';
  tokenForm.append(tokenLabel, tokenInput, tokenVisibility, tokenHint, pairButton);
  tokenForm.addEventListener('submit', (event) => {
    event.preventDefault();
    const invoke = getInvoke();
    if (!invoke || !panelId) return;
    const token = String(tokenInput.value || '').trim();
    if (!token) return;
    const gen = renderGeneration;
    const activeProfileId = panelId;
    invoke('vault_pair_bot', { profileId: activeProfileId, token })
      .then((envelope) => {
        if (isStale(gen, activeProfileId)) return;
        if (!envelope?.ok) {
          setInvalid(tokenInput, true);
          showError(envelope?.message || t('vault.errors.requestFailed'));
          return;
        }
        setInvalid(tokenInput, false);
        showError('');
        botLink = envelope.data?.deepLink || '';
        botUsername = envelope.data?.botUsername || '';
        botStep = 'link';
        tokenInput.value = '';
        updatePairUi();
      })
      .catch(() => {
        if (isStale(gen, activeProfileId)) return;
        setInvalid(tokenInput, true);
        showError(t('vault.errors.requestFailed'));
      });
  });

  const linkStep = make('div', 'vault-bot-link');
  linkStep.hidden = true;
  const linkHeading = make('p', 'vault-bot-link-heading', t('vault.bot.linkHeading'));
  const anchor = document.createElement('a');
  anchor.className = 'vault-bot-link-anchor';
  anchor.setAttribute('target', '_blank');
  anchor.setAttribute('rel', 'noopener');
  anchor.hidden = true;
  const startHint = make('p', 'vault-bot-hint vault-bot-start-hint', t('vault.bot.startHint'));
  const waitButton = make('button', 'btn btn-primary vault-bot-wait', t('vault.bot.startConfirm'));
  waitButton.type = 'button';
  waitButton.addEventListener('click', () => {
    if (botWaiting) return;
    const invoke = getInvoke();
    if (!invoke || !panelId) return;
    const gen = renderGeneration;
    const activeProfileId = panelId;
    botWaiting = true;
    waitButton.disabled = true;
    waitButton.textContent = t('vault.bot.waiting');
    invoke('vault_pair_bot_wait', { profileId: activeProfileId })
      .then((envelope) => {
        if (isStale(gen, activeProfileId)) return;
        botWaiting = false;
        waitButton.disabled = false;
        if (!envelope?.ok || envelope.data?.chatCaptured === false) {
          showError(envelope?.data?.chatCaptured === false
            ? t('vault.bot.notCaptured')
            : envelope?.message || t('vault.errors.requestFailed'));
          waitButton.textContent = t('vault.bot.waitAgain');
          return;
        }
        showError('');
        waitButton.textContent = t('vault.bot.startConfirm');
        botStep = 'token';
        botLink = '';
        botUsername = '';
        updatePairUi();
        refresh();
      })
      .catch(() => {
        if (isStale(gen, activeProfileId)) return;
        botWaiting = false;
        waitButton.disabled = false;
        waitButton.textContent = t('vault.bot.waitAgain');
        showError(t('vault.errors.requestFailed'));
      });
  });
  linkStep.append(linkHeading, anchor, startHint, waitButton);

  return [tokenForm, linkStep];
}

function buildForms() {
  const hint = make('p', 'vault-hint', t('vault.login.credentialsHint'));
  hint.hidden = true;

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
  const credentialsFields = make('div', 'vault-credentials-fields');
  credentialsFields.hidden = true;
  const apiIdLabel = make('label', undefined, t('vault.login.apiIdLabel'));
  apiIdLabel.setAttribute('for', 'vault-api-id');
  const apiIdInput = document.createElement('input');
  apiIdInput.type = 'text';
  apiIdInput.id = 'vault-api-id';
  apiIdInput.className = 'vault-input vault-api-id';
  apiIdInput.inputMode = 'numeric';
  apiIdInput.autocomplete = 'off';
  apiIdInput.required = true;
  const apiHashLabel = make('label', undefined, t('vault.login.apiHashLabel'));
  apiHashLabel.setAttribute('for', 'vault-api-hash');
  const apiHashInput = document.createElement('input');
  apiHashInput.type = 'password';
  apiHashInput.id = 'vault-api-hash';
  apiHashInput.className = 'vault-input vault-api-hash';
  apiHashInput.autocomplete = 'new-password';
  apiHashInput.required = true;
  const apiHashVisibility = make('button', 'btn btn-subtle vault-password-toggle');
  addPasswordToggle(apiHashInput, apiHashVisibility, t('vault.login.show'), t('vault.login.hide'));
  credentialsFields.append(apiIdLabel, apiIdInput, apiHashLabel, apiHashInput, apiHashVisibility);
  const phoneSubmit = make('button', 'btn btn-primary', t('vault.login.phoneSubmit'));
  phoneSubmit.type = 'submit';
  phoneForm.append(phoneLabel, phoneInput, credentialsFields, phoneSubmit);
  phoneForm.addEventListener('submit', (event) => {
    event.preventDefault();
    const invoke = getInvoke();
    if (!invoke || !panelId) return;
    const phone = String(phoneInput.value || '').trim().replace(/[\s()-]/g, '');
    const apiIdValue = String(apiIdInput.value || '').trim();
    const apiId = apiIdValue ? Number(apiIdValue) : null;
    const apiHash = String(apiHashInput.value || '').trim() || null;
    const gen = renderGeneration;
    const activeProfileId = panelId;
    invoke('vault_login_phone', {
      profileId: activeProfileId,
      phone,
      apiId: Number.isSafeInteger(apiId) ? apiId : null,
      apiHash,
    })
      .then((envelope) => {
        if (isStale(gen, activeProfileId)) return;
        if (envelope?.ok) {
          setInvalid(phoneInput, false);
          setInvalid(apiIdInput, false);
          setInvalid(apiHashInput, false);
          showError('');
          setLoginStep('code');
        } else {
          setInvalid(phoneInput, true);
          setInvalid(apiIdInput, true);
          setInvalid(apiHashInput, true);
          showError(envelope?.message || t('vault.errors.requestFailed'));
        }
      })
      .catch(() => {
        if (isStale(gen, activeProfileId)) return;
        setInvalid(phoneInput, true);
        setInvalid(apiIdInput, true);
        setInvalid(apiHashInput, true);
        showError(t('vault.errors.requestFailed'));
      });
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
    const password = String(passwordInput.value || '');
    const gen = renderGeneration;
    const activeProfileId = panelId;
    invoke('vault_login_code', {
      profileId: activeProfileId,
      code: String(codeInput.value || '').trim(),
      password: password ? password : null,
    })
      .then((envelope) => {
        if (isStale(gen, activeProfileId)) return;
        if (envelope?.ok) {
          setInvalid(codeInput, false);
          showError('');
          setLoginStep('');
          passwordInput.value = '';
          refresh();
        } else {
          setInvalid(codeInput, true);
          showError(envelope?.message || t('vault.errors.requestFailed'));
        }
      })
      .catch(() => {
        if (isStale(gen, activeProfileId)) return;
        setInvalid(codeInput, true);
        showError(t('vault.errors.requestFailed'));
      });
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
  const transportNode = make('span', 'vault-transport');
  transportNode.hidden = true;
  const stateLine = make('p', 'vault-state-line');
  stateLine.append(stateNode, statsNode, transportNode);
  const errorNode = make('p', 'vault-error');
  errorNode.setAttribute('role', 'alert');
  errorNode.setAttribute('id', 'vault-error-msg');
  errorNode.hidden = true;
  const actions = make('div', 'vault-actions');
  node.append(
    heading,
    stateLine,
    errorNode,
    actions,
    ...buildPairModeChooser(),
    ...buildBotForms(),
    ...buildForms(),
    ...buildLogTable()
  );
  return node;
}

function buildBridgeNoticeSection() {
  const previous = document.getElementById(PANEL_ID);
  if (previous) previous.remove();
  const node = make('section', 'vault-panel vault-panel-bridge');
  node.id = PANEL_ID;
  const heading = make('h3', 'vault-heading', t('vault.title'));
  const notice = make('p', 'vault-hint vault-bridge-notice', t('vault.bridge.desktopOnly'));
  node.append(heading, notice);
  return node;
}

/** Renders the vault panel into the details dialog for one profile. */
export function render(profile) {
  if (typeof document === 'undefined' || !profile?.id) return;
  renderGeneration += 1;
  refreshGeneration += 1;
  panelId = profile.id;
  pairMode = 'bot';
  loginStep = '';
  botStep = 'token';
  botLink = '';
  botUsername = '';
  botWaiting = false;
  lastState = 'unpaired';
  lastStatus = null;
  lastMessage = '';
  const dialog = document.getElementById('profile-details-dialog');
  if (!dialog) return;
  const invoke = getInvoke();
  section = invoke ? buildSection() : buildBridgeNoticeSection();
  const anchor = dialog.querySelector('form[method="dialog"]');
  if (anchor?.parentElement) {
    dialog.insertBefore(section, anchor.parentElement === dialog ? anchor : anchor.parentElement);
  } else {
    dialog.appendChild(section);
  }
  if (invoke) {
    refresh();
  }
}

const api = { render, applyStatus, refresh };

if (typeof window !== 'undefined') {
  window.__nexadockVault = api;
}

export default api;
