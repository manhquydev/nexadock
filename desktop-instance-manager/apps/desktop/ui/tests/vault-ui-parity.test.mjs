import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { initI18n } from '../i18n/index.js';

await initI18n();

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const uiDir = path.resolve(__dirname, '..');

const viLocale = JSON.parse(fs.readFileSync(path.join(uiDir, 'i18n', 'vi-VN.json'), 'utf8'));
const enLocale = JSON.parse(fs.readFileSync(path.join(uiDir, 'i18n', 'en-US.json'), 'utf8'));

// ---- Minimal DOM mock, shared pattern with vault-ui-contracts.test.mjs.

class FakeNode {
  constructor(tag) {
    this.tagName = String(tag).toUpperCase();
    this.children = [];
    this.attributes = new Map();
    this.className = '';
    this.id = '';
    this.hidden = false;
    this.textContent = '';
    this.title = '';
    this.listeners = new Map();
  }

  appendChild(child) {
    this.children.push(child);
    return child;
  }

  append(...kids) {
    for (const kid of kids) this.appendChild(kid);
  }

  replaceChildren() {
    this.children = [];
  }

  setAttribute(name, value) {
    this.attributes.set(name, value);
  }

  getAttribute(name) {
    return this.attributes.has(name) ? this.attributes.get(name) : null;
  }

  removeAttribute(name) {
    this.attributes.delete(name);
  }
  focus() {
    this.focused = true;
  }

  addEventListener(name, listener) {
    this.listeners.set(name, listener);
  }

  insertBefore(node) {
    this.children.unshift(node);
    return node;
  }

  querySelector(selector) {
    return this.#find(selector.split('.').filter(Boolean).slice(0, 2));
  }

  #find(classes) {
    for (const child of this.children) {
      const childClasses = String(child.className || '').split(/\s+/);
      if (classes.every((wanted) => childClasses.includes(wanted.replace('.', '')))) {
        return child;
      }
      const nested = child.querySelector?.(classes.join(' '));
      if (nested) return nested;
    }
    return null;
  }
}

const registry = new Map();
const dialog = new FakeNode('dialog');
dialog.id = 'profile-details-dialog';
registry.set('profile-details-dialog', dialog);

const documentMock = {
  createElement: (tag) => new FakeNode(tag),
  getElementById: (id) => registry.get(id) || null,
};

const { default: panel, PANEL_ID } = await import(
  pathToFileURL(path.join(uiDir, 'vault-panel.js')).href
);

const profile = { id: 'work_1', name: 'Work' };
const stubDesktopInvoke = () => new Promise(() => {});

function pairedStatus(transport) {
  return {
    paired: true,
    enabled: true,
    engineRunning: false,
    pending: 1,
    failed: 0,
    done: 4,
    transport,
  };
}

function renderSection() {
  panel.render(profile);
  const section = dialog.children.at(-1);
  assert.ok(section, 'vault panel section must be attached to the details dialog');
  assert.equal(section.id, PANEL_ID);
  return section;
}

/** Action row fingerprint: ordered list of visible button labels + checkbox presence. */
function actionFingerprint(section) {
  const actions = section.querySelector('.vault-actions');
  const fingerprint = { buttons: [], deleteSessionCheckbox: null };
  for (const child of actions.children) {
    if (child.tagName === 'BUTTON') fingerprint.buttons.push(child.textContent);
    if (child.querySelector?.('.vault-delete-session-check')) {
      fingerprint.deleteSessionCheckbox = child;
    }
  }
  return fingerprint;
}

// 1. Action parity: paired bot and paired mtproto must expose the same managed
//    surface; only the tier label (vault.transport.*) may differ.
test('paired bot and mtproto tiers render the same action surface', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    const fingerprints = {};
    const transports = {};
    for (const transport of ['bot', 'mtproto']) {
      const section = renderSection();
      panel.applyStatus(pairedStatus(transport), '');
      fingerprints[transport] = actionFingerprint(section);
      transports[transport] = section.querySelector('.vault-transport')?.textContent;

      // Log table + stats line exist for both tiers.
      assert.ok(section.querySelector('.vault-log-table'), `${transport}: log table must render`);
      assert.ok(section.querySelector('.vault-log-body'), `${transport}: log body must render`);
      const stats = section.querySelector('.vault-stats')?.textContent || '';
      assert.notEqual(stats, '', `${transport}: stats summary must render while paired`);
    }

    assert.deepEqual(
      fingerprints.bot.buttons,
      [
        viLocale.vault.actions.syncNow,
        viLocale.vault.actions.disable,
        viLocale.vault.actions.unpair,
      ],
      'both tiers must expose sync now, disable, unpair with identical labels'
    );
    assert.deepEqual(fingerprints.bot.buttons, fingerprints.mtproto.buttons, 'button actions must match across tiers');
    assert.equal(fingerprints.bot.deleteSessionCheckbox, null, 'bot tier has no Telegram session to delete');
    assert.ok(fingerprints.mtproto.deleteSessionCheckbox, 'delete-session checkbox must render for mtproto');

    // The only allowed difference is the tier label itself.
    assert.equal(transports.bot, viLocale.vault.transport.bot);
    assert.equal(transports.mtproto, viLocale.vault.transport.mtproto);
    assert.notEqual(transports.bot, transports.mtproto);
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

// 2. Bot error flow: a rejected token and a missed chat capture must surface
//    error text without crashing, keeping retry/confirm buttons clickable.
test('bot pairing errors show messages and keep retry controls usable', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  const invoke = (command) => {
    if (command === 'vault_get_status') {
      return Promise.resolve({ ok: true, data: { paired: false, enabled: false, engineRunning: false, pending: 0, failed: 0, done: 0 } });
    }
    if (command === 'vault_get_log') return Promise.resolve({ ok: true, data: [] });
    if (command === 'vault_pair_bot') {
      return Promise.resolve({ ok: false, message: 'token rejected' });
    }
    if (command === 'vault_pair_bot_wait') {
      return Promise.resolve({ ok: false, message: 'timeout' });
    }
    return Promise.resolve({ ok: false, message: `unexpected command: ${command}` });
  };
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    const section = renderSection();
    await flush();
    panel.applyStatus(null, '');

    // Wrong token: vault_pair_bot returns ok:false.
    const botForm = section.querySelector('.vault-form-bot');
    const tokenInput = section.querySelector('.vault-bot-token');
    assert.equal(botForm?.hidden, false, 'bot token form must stay visible after a failed pair');
    tokenInput.value = '123456789:wrong';
    botForm.listeners.get('submit')({ preventDefault() {} });
    await flush();

    const errorNode = section.querySelector('.vault-error');
    assert.equal(errorNode?.hidden, false, 'error node must be revealed');
    assert.notEqual(errorNode?.textContent, '', 'error text must be non-empty');
    assert.ok(section.querySelector('.vault-bot-pair-button'), 'pair button must survive the failure');
    assert.equal(botForm.hidden, false, 'token form must remain for a retry');
    assert.notEqual(tokenInput.value, '', 'token input keeps its value so the user can fix it');

    // Retry still works end to end (the handler stays attached).
    tokenInput.value = '';
    assert.doesNotThrow(() => botForm.listeners.get('submit')({ preventDefault() {} }));

    // Missed Start: reach the link step with a valid pair, then a failed wait.
    const okSection = renderSection();
    const pairInvoke = (command) => {
      if (command === 'vault_pair_bot') {
        return Promise.resolve({ ok: true, data: { botUsername: 'retry_bot', deepLink: 'https://t.me/retry_bot?start=vault' } });
      }
      if (command === 'vault_get_status') return invoke(command);
      if (command === 'vault_get_log') return invoke(command);
      return Promise.resolve({ ok: false, message: 'timeout' });
    };
    globalThis.window = { __TAURI_INTERNALS__: { invoke: pairInvoke } };
    const okForm = okSection.querySelector('.vault-form-bot');
    okSection.querySelector('.vault-bot-token').value = '123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ';
    okForm.listeners.get('submit')({ preventDefault() {} });
    await flush();

    const waitButton = okSection.querySelector('.vault-bot-wait');
    assert.ok(waitButton, 'confirm button must exist on the link step');
    waitButton.listeners.get('click')();
    await flush();
    const okError = okSection.querySelector('.vault-error');
    assert.equal(okError?.hidden, false, 'wait failure must reveal the error node');
    assert.notEqual(okError?.textContent, '', 'wait failure must show an error message');
    assert.equal(waitButton.disabled, false, 'confirm button must stay enabled for retry');
    assert.equal(okSection.querySelector('.vault-bot-link')?.hidden, false, 'link step must survive a missed Start');
    // Retry affordance remains clickable.
    assert.doesNotThrow(() => waitButton.listeners.get('click')());
    await flush();
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

// 3. Mid-flow mode switch: bot -> token entry -> advanced -> back to bot must
//    restore the token form and clear stale errors.
test('switching tiers mid-flow never strands a stale error or broken form', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const invoke = (command) => {
    if (command === 'vault_get_status') {
      return Promise.resolve({ ok: true, data: { paired: false, enabled: false, engineRunning: false, pending: 0, failed: 0, done: 0 } });
    }
    if (command === 'vault_get_log') return Promise.resolve({ ok: true, data: [] });
    if (command === 'vault_pair_bot') return Promise.resolve({ ok: false, message: 'bad token' });
    return Promise.resolve({ ok: false, message: `unexpected command: ${command}` });
  };
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    const section = renderSection();
    const botRadio = section.querySelector('.vault-mode-radio-bot');
    const mtpRadio = section.querySelector('.vault-mode-radio-mtp');

    // Bot tier: enter a token, submit, get an error.
    const tokenInput = section.querySelector('.vault-bot-token');
    tokenInput.value = '123456789:wrong';
    section.querySelector('.vault-form-bot').listeners.get('submit')({ preventDefault() {} });
    await flush();
    assert.equal(section.querySelector('.vault-error')?.hidden, false, 'error must show after the failed pair');

    // Switch to the advanced tier: mtproto api fields must be the surface.
    mtpRadio.checked = true;
    mtpRadio.listeners.get('change')();
    assert.equal(section.querySelector('.vault-form-bot')?.hidden, true, 'bot form must hide in the advanced tier');
    assert.equal(section.querySelector('.vault-api-id')?.required, true, 'api id field must be present in mtproto tier');
    assert.equal(section.querySelector('.vault-api-hash')?.type, 'password', 'api hash field must be present in mtproto tier');
    assert.equal(section.querySelector('.vault-hint')?.hidden, false, 'credentials hint must show in mtproto tier');
    assert.equal(section.querySelector('.vault-actions').children.length, 1, 'mtproto tier exposes its pair action');

    // Switch back to bot: token form returns and the stale error is cleared.
    botRadio.checked = true;
    botRadio.listeners.get('change')();
    const botForm = section.querySelector('.vault-form-bot');
    assert.equal(botForm?.hidden, false, 'bot token form must return after switching back');
    assert.equal(section.querySelector('.vault-error')?.textContent, '', 'stale error must be cleared on mode switch');
    assert.equal(section.querySelector('.vault-error')?.hidden, true, 'error node must hide again after the mode switch');
    assert.equal(section.querySelector('.vault-form-phone')?.hidden, true, 'mtproto forms must hide in bot tier');
    assert.equal(section.querySelector('.vault-actions').children.length, 0, 'bot tier keeps the pair action inside its own form');
    assert.ok(botForm.querySelector('.vault-bot-pair-button'), 'bot pair button must remain usable');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

// 4. Legacy paired status (pre-upgrade, no transport field) labels mtproto.
test('legacy paired status without a transport field labels mtproto', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    const section = renderSection();
    panel.applyStatus({ paired: true, enabled: true, engineRunning: false, pending: 0, failed: 0, done: 0 }, '');
    const transport = section.querySelector('.vault-transport');
    assert.equal(transport?.hidden, false);
    assert.equal(transport?.textContent, viLocale.vault.transport.mtproto);
    assert.notEqual(transport?.textContent, viLocale.vault.transport.bot);
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

// 5. Every vault.mode.*, vault.bot.*, vault.transport.* key used by the panel
//    exists in both locales with non-empty strings.
test('tier i18n keys (mode/bot/transport) exist in vi and en', () => {
  for (const group of ['mode', 'bot', 'transport']) {
    const viKeys = Object.keys(viLocale.vault[group] || {});
    const enKeys = Object.keys(enLocale.vault[group] || {});
    assert.deepEqual(viKeys.sort(), enKeys.sort(), `vault.${group} keys must match across locales`);
    assert.ok(viKeys.length > 0, `vault.${group} must declare keys`);
    for (const key of viKeys) {
      assert.equal(typeof viLocale.vault[group][key], 'string', `vault.${group}.${key} must be a string in vi`);
      assert.ok(viLocale.vault[group][key].length > 0, `vault.${group}.${key} must be non-empty in vi`);
      assert.equal(typeof enLocale.vault[group][key], 'string', `vault.${group}.${key} must be a string in en`);
      assert.ok(enLocale.vault[group][key].length > 0, `vault.${group}.${key} must be non-empty in en`);
    }
  }
  // The panel source references exactly these tier groups.
  const panelSource = fs.readFileSync(path.join(uiDir, 'vault-panel.js'), 'utf8');
  for (const key of ['vault.mode.', 'vault.bot.', 'vault.transport.']) {
    assert.ok(panelSource.includes(`'${key}`) || panelSource.includes(key), `panel must use ${key} keys`);
  }
});

// ---- Modern Web Guidance (forms guide, 2026_11_09-5bcc021b) criteria.
// Asserted only for behavior WebKitGTK 4.1 supports (Baseline); the panel is
// not modified here — gaps are reported as findings to the orchestrator.

// Guide "forms": DO use fieldset+legend (legend as first child) to group
// related controls; 2–5 mutually exclusive choices belong on visible native
// radio buttons sharing one name.
test('mode chooser follows the fieldset/legend + native radio guidance', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    const section = renderSection();
    const chooser = section.querySelector('.vault-mode-chooser');
    assert.equal(chooser.tagName, 'FIELDSET');
    const legend = chooser.querySelector('.vault-mode-legend');
    assert.ok(legend, 'fieldset must contain a legend');
    assert.equal(chooser.children[0], legend, 'legend must be the first child of the fieldset');

    const radios = chooser.children.filter((child) => child.querySelector?.('.vault-mode-radio-bot') || child.querySelector?.('.vault-mode-radio-mtp'));
    assert.equal(radios.length, 2, 'both tier options must be visible radio labels inside the fieldset');
    for (const label of radios) {
      assert.equal(label.tagName, 'LABEL', 'each option wraps its radio in a label (inline label after input)');
      const radio = label.children[0];
      assert.equal(radio.tagName, 'INPUT', 'option control must be a native input');
      assert.equal(radio.type, 'radio', 'tier choice must use native radio buttons, not a select or custom widget');
      assert.equal(radio.name, 'vault-pair-mode', 'mutually exclusive radios must share one name');
    }
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

// Guide "forms": DO announce dynamic errors via aria-live / role="alert";
// both tiers must route failures through the same live region.
test('both tiers announce errors through the same role=alert region', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const invoke = (command, args) => {
    if (command === 'vault_get_status') {
      return Promise.resolve({ ok: true, data: { paired: false, enabled: false, engineRunning: false, pending: 0, failed: 0, done: 0 } });
    }
    if (command === 'vault_get_log') return Promise.resolve({ ok: true, data: [] });
    if (command === 'vault_pair_bot') return Promise.resolve({ ok: false, message: 'token rejected' });
    if (command === 'vault_pair') return Promise.resolve({ ok: false, message: 'mtproto pair rejected' });
    return Promise.resolve({ ok: false, message: `unexpected command: ${command}` });
  };
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    const errorNode = () => {
      const node = dialog.children.at(-1).querySelector('.vault-error');
      assert.equal(node.getAttribute('role'), 'alert', 'error region must announce via role=alert');
      return node;
    };

    // Bot tier failure.
    const botSection = renderSection();
    await flush();
    botSection.querySelector('.vault-bot-token').value = '123456789:wrong';
    botSection.querySelector('.vault-form-bot').listeners.get('submit')({ preventDefault() {} });
    await flush();
    assert.notEqual(errorNode().textContent, '', 'bot tier failure must land in the live region');

    // Advanced tier failure.
    const mtpSection = renderSection();
    await flush();
    mtpSection.querySelector('.vault-mode-radio-mtp').checked = true;
    mtpSection.querySelector('.vault-mode-radio-mtp').listeners.get('change')();
    const pairButton = mtpSection.querySelector('.vault-actions').children[0];
    assert.equal(pairButton.tagName, 'BUTTON');
    pairButton.listeners.get('click')();
    await flush();
    assert.notEqual(errorNode().textContent, '', 'advanced tier failure must land in the same live region');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

// Guide "forms": DO associate every input with a label via for/id; DO use
// valid autocomplete values on authentication fields. Asserted only where the
// panel already complies (token/apiHash gaps are reported, not asserted).
test('visible tier inputs keep label association and guide-compliant autocomplete', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    const section = renderSection();

    // Bot tier: token input has a labelled-for and a masked password type.
    const tokenInput = section.querySelector('.vault-bot-token');
    assert.equal(tokenInput.type, 'password');
    const tokenLabel = section.querySelector('.vault-form-bot').children[0];
    assert.equal(tokenLabel.tagName, 'LABEL');
    assert.equal(tokenLabel.getAttribute('for'), 'vault-bot-token', 'token input must have a for/id label association');
    assert.equal(tokenInput.id, 'vault-bot-token');

    // Advanced tier: api fields keep for/id label associations.
    for (const [cls, id] of [['.vault-api-id', 'vault-api-id'], ['.vault-api-hash', 'vault-api-hash']]) {
      const input = section.querySelector(cls);
      assert.equal(input.id, id, `${cls} must carry its id for label association`);
    }

    // Existing login flow already uses guide-recommended autocomplete values.
    const panelSource = fs.readFileSync(path.join(uiDir, 'vault-panel.js'), 'utf8');
    assert.match(panelSource, /autocomplete = 'tel'/, 'phone field uses autocomplete=tel');
    assert.match(panelSource, /autocomplete = 'one-time-code'/, 'code field uses autocomplete=one-time-code');
    assert.match(panelSource, /autocomplete = 'current-password'/, 'password field uses autocomplete=current-password');
    // Copy-paste is not blocked on auth fields (guide: DON'T block paste).
    assert.doesNotMatch(panelSource, /onpaste|paste'/, 'panel must not block paste on authentication fields');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

// aria-invalid wiring (approved finding fix): failed submits mark the offending
// inputs invalid and point describedby at the alert; success restores state.
test('validation failures set aria-invalid and describedby, cleared on success', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  let pairOk = false;
  const invoke = (command) => {
    if (command === 'vault_get_status') {
      return Promise.resolve({ ok: true, data: { paired: false, enabled: false, engineRunning: false, pending: 0, failed: 0, done: 0 } });
    }
    if (command === 'vault_get_log') return Promise.resolve({ ok: true, data: [] });
    if (command === 'vault_pair_bot') {
      return pairOk
        ? Promise.resolve({ ok: true, data: { botUsername: 'aria_bot', deepLink: 'https://t.me/aria_bot?start=vault' } })
        : Promise.resolve({ ok: false, message: 'token rejected' });
    }
    return Promise.resolve({ ok: false, message: `unexpected command: ${command}` });
  };
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    const section = renderSection();
    await flush();

    const errorNode = section.querySelector('.vault-error');
    assert.equal(errorNode.getAttribute('id'), 'vault-error-msg', 'alert region must carry a stable id');
    const tokenHint = section.querySelector('.vault-bot-hint');
    assert.equal(tokenHint.getAttribute('id'), 'vault-bot-token-hint', 'token hint must carry a stable id');

    const tokenInput = section.querySelector('.vault-bot-token');
    assert.equal(tokenInput.getAttribute('aria-describedby'), 'vault-bot-token-hint', 'token input describes its hint while valid');
    assert.equal(tokenInput.getAttribute('aria-invalid'), null);

    // Failure: aria-invalid + describedby -> alert.
    tokenInput.value = '123456789:wrong';
    section.querySelector('.vault-form-bot').listeners.get('submit')({ preventDefault() {} });
    await flush();
    assert.equal(tokenInput.getAttribute('aria-invalid'), 'true');
    assert.equal(tokenInput.getAttribute('aria-describedby'), 'vault-error-msg');

    // Success: state restored, hint re-linked.
    pairOk = true;
    tokenInput.value = '123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ';
    section.querySelector('.vault-form-bot').listeners.get('submit')({ preventDefault() {} });
    await flush();
    assert.equal(tokenInput.getAttribute('aria-invalid'), null, 'aria-invalid must clear on success');
    assert.equal(tokenInput.getAttribute('aria-describedby'), 'vault-bot-token-hint', 'describedby must return to the hint');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

// B1: a token-only config is persisted as paired but not configured. Restore
// the continuation UI so a restart cannot strand the owner after token entry,
// while exposing an Unpair button to cancel/reset a half-paired bot.
test('half-paired bot status resumes deep-link and Start flow with unpair button only', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  const calls = [];
  let isPaired = true;
  const invoke = (command, args) => {
    calls.push([command, args]);
    if (command === 'vault_get_status') {
      return Promise.resolve({
        ok: true,
        data: isPaired
          ? { paired: true, transport: 'bot', botConfigured: false, botUsername: 'saved_bot', botDeepLink: 'https://t.me/saved_bot?start=nonce123', enabled: true, engineRunning: false, pending: 0, failed: 0, done: 0 }
          : { paired: false, transport: 'bot', botConfigured: false, credentialsAvailable: false, enabled: false, engineRunning: false, pending: 0, failed: 0, done: 0 },
      });
    }
    if (command === 'vault_get_log') return Promise.resolve({ ok: true, data: [] });
    if (command === 'vault_pair_bot_wait') return Promise.resolve({ ok: true, message: 'No bot chat captured yet; ask again later.', data: { chatCaptured: false } });
    if (command === 'vault_unpair') {
      isPaired = false;
      return Promise.resolve({ ok: true, data: { removed: true } });
    }
    return Promise.resolve({ ok: false, message: `unexpected command: ${command}` });
  };
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    const section = renderSection();
    await flush();
    panel.applyStatus({ paired: true, transport: 'bot', botConfigured: false, botUsername: 'saved_bot', enabled: true, engineRunning: false, pending: 0, failed: 0, done: 0 }, '');

    assert.equal(section.querySelector('.vault-state')?.textContent, viLocale.vault.states.continuingBot);
    assert.equal(section.querySelector('.vault-mode-chooser')?.hidden, true, 'mode chooser must hide during continuation');
    assert.equal(section.querySelector('.vault-form-bot')?.hidden, true, 'token form must not be shown after it was saved');
    const linkStep = section.querySelector('.vault-bot-link');
    assert.equal(linkStep?.hidden, false, 'continuation deep-link step must be visible');
    const anchor = linkStep.querySelector('.vault-bot-link-anchor');
    assert.equal(anchor?.getAttribute('href'), 'https://t.me/saved_bot');
    assert.equal(anchor?.textContent, 'saved_bot');
    assert.equal(linkStep.querySelector('.vault-bot-start-hint')?.textContent, viLocale.vault.bot.continuationHint);
    assert.equal(linkStep.querySelector('.vault-bot-wait')?.textContent, viLocale.vault.bot.continuationConfirm);
    const fingerprint = actionFingerprint(section);
    assert.deepEqual(
      fingerprint.buttons,
      [viLocale.vault.actions.unpair],
      'bot-continuation must expose only the unpair action (no sync/enable buttons)'
    );
    assert.equal(fingerprint.deleteSessionCheckbox, null, 'bot-continuation must not show delete-session checkbox');

    // When backend supplies botDeepLink with ?start=<nonce>, continuation uses it.
    panel.applyStatus({ paired: true, transport: 'bot', botConfigured: false, botUsername: 'saved_bot', botDeepLink: 'https://t.me/saved_bot?start=nonce123', enabled: true, engineRunning: false, pending: 0, failed: 0, done: 0 }, '');
    assert.equal(anchor?.getAttribute('href'), 'https://t.me/saved_bot?start=nonce123');

    const waitButton = linkStep.querySelector('.vault-bot-wait');
    waitButton.listeners.get('click')();
    await flush();
    assert.ok(calls.some(([command]) => command === 'vault_pair_bot_wait'), 'continuation action must resume the normal wait command');
    assert.equal(section.querySelector('.vault-error')?.textContent, viLocale.vault.bot.notCaptured);
    assert.equal(waitButton.disabled, false, 'retry button must remain usable after timeout');

    const unpairBtn = section.querySelector('.vault-actions').children[0];
    unpairBtn.listeners.get('click')();
    await flush();
    await flush();
    assert.ok(calls.some(([command]) => command === 'vault_unpair'), 'continuation unpair button must call vault_unpair');
    assert.equal(section.querySelector('.vault-state')?.textContent, viLocale.vault.states.unpaired, 'unpair in bot-continuation must transition UI back to unpaired');
    assert.equal(section.querySelector('.vault-mode-chooser')?.hidden, false);
    assert.equal(section.querySelector('.vault-form-bot')?.hidden, false);
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('async generation guard prevents stale responses across profile switches and log failure keeps paired status', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  let resolveProfileA;
  const invoke = (command, args) => {
    if (command === 'vault_get_status' && args.profileId === 'prof_a') {
      return new Promise((resolve) => {
        resolveProfileA = resolve;
      });
    }
    if (command === 'vault_get_status' && args.profileId === 'prof_b') {
      return Promise.resolve({
        ok: true,
        data: { paired: true, transport: 'bot', botConfigured: true, enabled: true, engineRunning: false, pending: 0, failed: 0, done: 2 },
      });
    }
    if (command === 'vault_get_log') {
      return Promise.reject(new Error('sqlite busy'));
    }
    return Promise.resolve({ ok: false, message: 'unexpected' });
  };
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    panel.render({ id: 'prof_a', name: 'Profile A' });
    panel.render({ id: 'prof_b', name: 'Profile B' });
    const section = dialog.children.at(-1);
    await flush();
    // Even though vault_get_log rejected, prof_b stays paired (never reverted to unpaired).
    assert.equal(section.querySelector('.vault-state')?.textContent, viLocale.vault.states.paired);

    // Late response for prof_a arriving after switching to prof_b must be ignored.
    resolveProfileA({ ok: true, data: { paired: false, enabled: false, engineRunning: false, pending: 0, failed: 0, done: 0 } });
    await flush();
    assert.equal(section.querySelector('.vault-state')?.textContent, viLocale.vault.states.paired);
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('failed actions and failed login code preserve error messages instead of refreshing over them', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  const invoke = (command) => {
    if (command === 'vault_get_status') {
      return Promise.resolve({
        ok: true,
        data: { paired: true, transport: 'bot', botConfigured: true, enabled: true, engineRunning: false, pending: 0, failed: 0, done: 0 },
      });
    }
    if (command === 'vault_get_log') return Promise.resolve({ ok: true, data: [] });
    if (command === 'vault_sync_now') return Promise.resolve({ ok: false, message: 'sync failed explicitly' });
    if (command === 'vault_login_code') return Promise.resolve({ ok: false, message: 'invalid 2FA password' });
    return Promise.resolve({ ok: false, message: 'unexpected' });
  };
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    const section = renderSection();
    await flush();
    const syncBtn = section.querySelector('.vault-actions').children[0];
    syncBtn.listeners.get('click')();
    await flush();
    assert.equal(section.querySelector('.vault-error')?.textContent, 'sync failed explicitly');
    assert.equal(section.querySelector('.vault-error')?.hidden, false);

    // Failed vault_login_code also preserves its error.
    section.querySelector('#vault-code-input')?.setAttribute?.('data-test', '1');
    const codeForm = section.querySelector('.vault-form-code');
    codeForm.children[1].value = '123456';
    codeForm.listeners.get('submit')({ preventDefault() {} });
    await flush();
    assert.equal(section.querySelector('.vault-error')?.textContent, 'invalid 2FA password');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('MTProto status with sessionExists false renders session-missing re-auth path while keeping mock compatibility', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    const section = renderSection();
    panel.applyStatus(
      { paired: true, transport: 'mtproto', sessionExists: false, credentialsAvailable: true, enabled: true, engineRunning: false, pending: 0, failed: 0, done: 0 },
      ''
    );
    assert.equal(section.querySelector('.vault-state')?.textContent, viLocale.vault.states.sessionMissing);
    assert.equal(section.querySelector('.vault-form-phone')?.hidden, false, 'phone re-auth form must be visible when session is missing');
    assert.equal(section.querySelector('.vault-credentials-fields')?.hidden, true, 'stored credentials hide API ID/Hash inputs');
    assert.deepEqual(actionFingerprint(section).buttons, [viLocale.vault.actions.unpair]);
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('bot token and API hash visibility toggles are native pressed buttons and restore focus', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    const section = renderSection();
    const tokenForm = section.querySelector('.vault-form-bot');
    const tokenInput = section.querySelector('.vault-bot-token');
    const tokenToggle = tokenForm.children.find((child) => child.className.includes('vault-password-toggle'));
    assert.equal(tokenToggle.tagName, 'BUTTON');
    assert.equal(tokenToggle.type, 'button');
    assert.equal(tokenToggle.getAttribute('aria-pressed'), 'false');
    tokenToggle.listeners.get('click')();
    assert.equal(tokenInput.type, 'text');
    assert.equal(tokenToggle.getAttribute('aria-pressed'), 'true');
    assert.equal(tokenInput.focused, true);
    tokenToggle.listeners.get('click')();
    assert.equal(tokenInput.type, 'password');
    assert.equal(tokenToggle.getAttribute('aria-pressed'), 'false');

    const apiHash = section.querySelector('.vault-api-hash');
    const credentialsFields = section.querySelector('.vault-credentials-fields');
    const hashToggle = credentialsFields.children.find((child) => child.className.includes('vault-password-toggle'));
    assert.equal(apiHash.type, 'password');
    hashToggle.listeners.get('click')();
    assert.equal(apiHash.type, 'text');
    assert.equal(hashToggle.getAttribute('aria-pressed'), 'true');
    assert.equal(apiHash.focused, true);
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('failed vault_login_code during session-missing re-auth returns to phone form and keeps error visible', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  const invoke = (command) => {
    if (command === 'vault_get_status') {
      return Promise.resolve({
        ok: true,
        data: {
          paired: true,
          transport: 'mtproto',
          sessionExists: false,
          credentialsAvailable: true,
          enabled: true,
          engineRunning: false,
          pending: 0,
          failed: 0,
          done: 0,
        },
      });
    }
    if (command === 'vault_get_log') return Promise.resolve({ ok: true, data: [] });
    if (command === 'vault_login_phone') return Promise.resolve({ ok: true, data: null });
    if (command === 'vault_login_code') {
      return Promise.resolve({ ok: false, message: 'invalid verification code' });
    }
    return Promise.resolve({ ok: false, message: `unexpected command: ${command}` });
  };
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    const section = renderSection();
    await flush();
    assert.equal(section.querySelector('.vault-state')?.textContent, viLocale.vault.states.sessionMissing);
    const phoneForm = section.querySelector('.vault-form-phone');
    const codeForm = section.querySelector('.vault-form-code');
    assert.equal(phoneForm?.hidden, false);
    assert.equal(codeForm?.hidden, true);

    phoneForm.children[1].value = '+84901234567';
    phoneForm.listeners.get('submit')({ preventDefault() {} });
    await flush();
    assert.equal(phoneForm.hidden, true);
    assert.equal(codeForm.hidden, false);

    codeForm.children[1].value = '123456';
    codeForm.listeners.get('submit')({ preventDefault() {} });
    await flush();
    assert.equal(codeForm.hidden, true, 'code form must hide because failed login consumed pending challenge');
    assert.equal(phoneForm.hidden, false, 'phone form must reappear so user can request a new code');
    assert.equal(section.querySelector('.vault-error')?.textContent, 'invalid verification code');
    assert.equal(section.querySelector('.vault-error')?.hidden, false);
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('bot token submit blocks concurrent in-flight submits and ignores stale responses after re-render', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  const pendingPairBots = [];
  const invoke = (command, args) => {
    if (command === 'vault_get_status') {
      return Promise.resolve({
        ok: true,
        data: {
          paired: false,
          transport: 'bot',
          botConfigured: false,
          credentialsAvailable: false,
          enabled: false,
          engineRunning: false,
          pending: 0,
          failed: 0,
          done: 0,
        },
      });
    }
    if (command === 'vault_get_log') return Promise.resolve({ ok: true, data: [] });
    if (command === 'vault_pair_bot') {
      return new Promise((resolve) => {
        pendingPairBots.push({ args, resolve });
      });
    }
    return Promise.resolve({ ok: false, message: `unexpected command: ${command}` });
  };
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    const section1 = renderSection();
    await flush();
    const tokenForm1 = section1.querySelector('.vault-form-bot');
    const tokenInput1 = section1.querySelector('.vault-bot-token');
    const pairButton1 = section1.querySelector('.vault-bot-pair-button');
    tokenInput1.value = '123456:first';
    tokenForm1.listeners.get('submit')({ preventDefault() {} });
    tokenForm1.listeners.get('submit')({ preventDefault() {} });
    assert.equal(pendingPairBots.length, 1, 'duplicate submit while vault_pair_bot is in flight must be ignored');
    assert.equal(pairButton1.disabled, true);

    pendingPairBots[0].resolve({
      ok: true,
      data: { botUsername: 'first_bot', deepLink: 'https://t.me/first_bot?start=n1' },
    });
    await flush();
    assert.equal(pairButton1.disabled, false);
    assert.equal(section1.querySelector('.vault-bot-link')?.hidden, false);
    assert.equal(section1.querySelector('.vault-bot-link-anchor')?.getAttribute('href'), 'https://t.me/first_bot?start=n1');

    // Re-render for another profile while a new vault_pair_bot is in flight:
    // the stale response must neither leak into the new profile nor keep botPairing locked.
    const section2 = renderSection();
    await flush();
    section2.querySelector('.vault-bot-token').value = '123456:stale';
    section2.querySelector('.vault-form-bot').listeners.get('submit')({ preventDefault() {} });
    assert.equal(pendingPairBots.length, 2);

    panel.render({ id: 'work_2', name: 'Work 2' });
    const section3 = dialog.children.at(-1);
    await flush();
    assert.equal(Boolean(section3.querySelector('.vault-bot-pair-button').disabled), false);

    pendingPairBots[1].resolve({
      ok: true,
      data: { botUsername: 'stale_bot', deepLink: 'https://t.me/stale_bot?start=old' },
    });
    await flush();
    assert.equal(section3.querySelector('.vault-bot-link')?.hidden, true, 'stale pair_bot response must not populate re-rendered profile');

    section3.querySelector('.vault-bot-token').value = '123456:second';
    section3.querySelector('.vault-form-bot').listeners.get('submit')({ preventDefault() {} });
    assert.equal(pendingPairBots.length, 3);
    assert.equal(pendingPairBots[2].args.profileId, 'work_2');
    pendingPairBots[2].resolve({
      ok: true,
      data: { botUsername: 'second_bot', deepLink: 'https://t.me/second_bot?start=n2' },
    });
    await flush();
    assert.equal(section3.querySelector('.vault-bot-link')?.hidden, false);
    assert.equal(section3.querySelector('.vault-bot-link-anchor')?.getAttribute('href'), 'https://t.me/second_bot?start=n2');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});
