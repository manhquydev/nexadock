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
const panelSource = fs.readFileSync(path.join(uiDir, 'vault-panel.js'), 'utf8');

function flatten(obj, prefix = '') {
  return Object.entries(obj).flatMap(([key, value]) => {
    const keyPath = prefix ? `${prefix}.${key}` : key;
    return value && typeof value === 'object' ? flatten(value, keyPath) : [keyPath];
  });
}

test('vault i18n keys are identical across vi-VN and en-US', () => {
  const viKeys = flatten(viLocale.vault).sort();
  const enKeys = flatten(enLocale.vault).sort();
  assert.ok(viKeys.length >= 20, 'expected a substantial vault key set');
  assert.deepEqual(viKeys, enKeys, 'vault key sets must match between locales');
  for (const key of viKeys) {
    const viValue = key.split('.').reduce((node, part) => node?.[part], viLocale.vault);
    const enValue = key.split('.').reduce((node, part) => node?.[part], enLocale.vault);
    assert.ok(String(viValue).trim().length > 0, `empty vi-VN value for vault.${key}`);
    assert.ok(String(enValue).trim().length > 0, `empty en-US value for vault.${key}`);
    const viParams = [...String(viValue).matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
    const enParams = [...String(enValue).matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
    assert.deepEqual(viParams, enParams, `placeholder mismatch for vault.${key}`);
  }
});

test('vault bot tier declares its dedicated i18n groups', () => {
  for (const locale of [viLocale, enLocale]) {
    assert.ok(locale.vault.mode?.legend, 'vault.mode.legend must exist');
    assert.ok(locale.vault.mode?.botTitle && locale.vault.mode?.botDesc, 'bot mode copy must exist');
    assert.ok(locale.vault.mode?.mtpTitle && locale.vault.mode?.mtpDesc, 'advanced mode copy must exist');
    assert.ok(locale.vault.bot?.tokenLabel && locale.vault.bot?.pairButton, 'bot form copy must exist');
    assert.ok(locale.vault.bot?.startHint && locale.vault.bot?.startConfirm, 'bot start copy must exist');
    assert.ok(locale.vault.bot?.notCaptured && locale.vault.bot?.waitAgain, 'bot wait retry copy must exist');
    assert.ok(locale.vault.transport?.bot && locale.vault.transport?.mtproto, 'transport labels must exist');
  }
});

test('vault panel never uses innerHTML', () => {
  assert.doesNotMatch(panelSource, /innerHTML/, 'vault panel must not use innerHTML');
  assert.match(panelSource, /createElement/, 'vault panel must build DOM nodes');
  assert.match(panelSource, /textContent/, 'vault panel must set text nodes');
});

test('vault panel opens the bot deep link safely', () => {
  assert.match(panelSource, /'vault_pair_bot'/, 'panel must call vault_pair_bot');
  assert.match(panelSource, /'vault_pair_bot_wait'/, 'panel must call vault_pair_bot_wait');
  assert.match(panelSource, /chatCaptured/, 'panel must branch on chatCaptured');
  assert.doesNotMatch(panelSource, /console\./, 'vault panel must not log (tokens must stay unlogged)');
});

// ---- Minimal DOM mock so render() executes for the lifecycle states.

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

const { default: panel, stateFor, PANEL_ID } = await import(
  pathToFileURL(path.join(uiDir, 'vault-panel.js')).href
);

const profile = { id: 'work_1', name: 'Work' };
const stubDesktopInvoke = () => new Promise(() => {});

function collectByTag(node, tag) {
  const found = [];
  if (!node) return found;
  if (node.tagName === tag) found.push(node);
  for (const child of node.children || []) {
    found.push(...collectByTag(child, tag));
  }
  return found;
}

function statuses() {
  return [
    { paired: false, enabled: false, engineRunning: false, pending: 0, failed: 0, done: 0 },
    { paired: true, enabled: true, engineRunning: true, uploading: 2, pending: 1, failed: 1, done: 10 },
    { paired: true, enabled: false, engineRunning: false, uploading: 0, pending: 3, failed: 0, done: 5 },
  ];
}

function renderSection() {
  panel.render(profile);
  const section = dialog.children.at(-1);
  assert.ok(section, 'vault panel section must be attached to the details dialog');
  assert.equal(section.id, PANEL_ID, 'section must carry the stable panel id');
  return section;
}

test('vault panel renders unpaired, syncing, and offline states', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    for (const status of statuses()) {
      const section = renderSection();
      panel.applyStatus(status, '');
      const apiIdInput = section.querySelector('.vault-api-id');
      const apiHashInput = section.querySelector('.vault-api-hash');
      assert.equal(apiIdInput?.inputMode, 'numeric');
      assert.equal(apiHashInput?.type, 'password');
      assert.equal(apiHashInput?.autocomplete, 'new-password');
      assert.equal(section.querySelector('.vault-credentials-fields')?.hidden, false);
      const stateText = section.querySelector('.vault-state')?.textContent || '';
      assert.notEqual(stateText, '', 'state label must resolve through i18n');
      assert.equal(stateText === stateText.trim(), true, 'state label must not be padded');
      const actions = section.querySelector('.vault-actions');
      assert.ok(actions, 'actions row must exist');
      const botForm = section.querySelector('.vault-form-bot');
      const transport = section.querySelector('.vault-transport');
      if (!status.paired) {
        // Unpaired defaults to the bot tier: the action lives in the bot form.
        assert.equal(actions.children.length, 0, 'bot tier keeps the pair action inside its own form');
        assert.equal(botForm?.hidden, false, 'bot token form must be visible in bot tier');
        assert.equal(transport?.hidden, true, 'transport label must hide while unpaired');
      } else {
        assert.ok(actions.children.length > 0, 'paired state must expose at least one action');
        assert.equal(botForm?.hidden, true, 'bot token form must hide once paired');
        assert.equal(section.querySelector('.vault-mode-chooser')?.hidden, true, 'mode chooser must hide once paired');
        assert.equal(transport?.hidden, false, 'paired state must show the transport tier');
      }
    }
    // Unpaired state must show exactly the pair action once the advanced tier is picked.
    const section = renderSection();
    panel.applyStatus(statuses()[0], '');
    const mtpRadio = section.querySelector('.vault-mode-radio-mtp');
    mtpRadio.checked = true;
    mtpRadio.listeners.get('change')();
    assert.equal(section.querySelector('.vault-actions').children.length, 1);
    panel.applyStatus({ ...statuses()[0], credentialsAvailable: true }, '');
    assert.equal(section.querySelector('.vault-credentials-fields')?.hidden, true);
    panel.applyStatus(statuses()[1], '');
    const stats = section.querySelector('.vault-stats')?.textContent || '';
    assert.match(stats, /1/);
    assert.match(stats, /1/);
    panel.applyStatus(statuses()[2], '');
    assert.notEqual(section.querySelector('.vault-state')?.textContent, '');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('stateFor maps the lifecycle states deterministically', () => {
  assert.equal(stateFor(null), 'unpaired');
  assert.equal(stateFor({ paired: true, enabled: true, engineRunning: true, uploading: 0 }), 'paired');
  assert.equal(stateFor({ paired: true, enabled: true, engineRunning: true, uploading: 1 }), 'syncing');
  assert.equal(stateFor({ paired: true, enabled: false }), 'offline');
  assert.equal(stateFor({ paired: true, transport: 'mtproto', sessionExists: false, enabled: true }), 'session-missing');
});

test('bridge mode without Tauri invoke renders a localized desktop-only notice and no interactive forms', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = {};
  try {
    const section = renderSection();
    const notice = section.querySelector('.vault-bridge-notice');
    assert.ok(notice, 'bridge mode must render a desktop-only notice');
    assert.equal(notice.textContent, viLocale.vault.bridge.desktopOnly);
    assert.equal(collectByTag(section, 'FORM').length, 0, 'bridge mode must render zero forms');
    assert.equal(collectByTag(section, 'INPUT').length, 0, 'bridge mode must render zero inputs');
    assert.equal(collectByTag(section, 'BUTTON').length, 0, 'bridge mode must render zero buttons');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('unpaired panel defaults to the bot tier with a two-option mode chooser', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    const section = renderSection();
    panel.applyStatus(statuses()[0], '');
    const chooser = section.querySelector('.vault-mode-chooser');
    assert.equal(chooser?.hidden, false, 'mode chooser must be visible while unpaired');
    assert.equal(chooser.tagName, 'FIELDSET', 'mode chooser must group radios in a fieldset');
    assert.ok(chooser.querySelector('.vault-mode-legend'), 'mode chooser must have a legend');
    const botRadio = section.querySelector('.vault-mode-radio-bot');
    const mtpRadio = section.querySelector('.vault-mode-radio-mtp');
    assert.equal(botRadio?.type, 'radio');
    assert.equal(botRadio?.checked, true, 'bot tier must be the default pairing mode');
    assert.equal(mtpRadio?.checked, false);
    assert.equal(botRadio?.name, 'vault-pair-mode');
    assert.equal(mtpRadio?.name, 'vault-pair-mode');
    assert.equal(botRadio?.value, 'bot');
    assert.equal(mtpRadio?.value, 'mtproto');

    const botForm = section.querySelector('.vault-form-bot');
    assert.equal(botForm?.hidden, false, 'bot token form must show for the default tier');
    const tokenInput = section.querySelector('.vault-bot-token');
    assert.equal(tokenInput?.type, 'password', 'bot token must be masked');
    assert.equal(tokenInput?.required, true);
    assert.equal(tokenInput?.autocomplete, 'new-password');
    assert.ok(botForm.querySelector('.vault-bot-pair-button'), 'bot form must expose its own pair action');

    assert.equal(section.querySelector('.vault-form-phone')?.hidden, true, 'phone form must hide in bot tier');
    assert.equal(section.querySelector('.vault-form-code')?.hidden, true, 'code form must hide in bot tier');
    assert.equal(section.querySelector('.vault-hint')?.hidden, true, 'advanced credentials hint must hide in bot tier');
    assert.equal(section.querySelector('.vault-bot-link')?.hidden, true, 'bot link step must hide until paired');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('switching to the advanced tier reveals the existing login flow', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    const section = renderSection();
    panel.applyStatus(statuses()[0], '');
    const mtpRadio = section.querySelector('.vault-mode-radio-mtp');
    const botRadio = section.querySelector('.vault-mode-radio-bot');
    const botForm = section.querySelector('.vault-form-bot');
    mtpRadio.checked = true;
    mtpRadio.listeners.get('change')();
    assert.equal(botForm?.hidden, true, 'bot form must hide in the advanced tier');
    assert.equal(section.querySelector('.vault-hint')?.hidden, false, 'advanced tier shows the credentials hint');
    assert.equal(section.querySelector('.vault-form-phone')?.hidden, true, 'phone form stays hidden until pair starts');
    assert.equal(section.querySelector('.vault-actions').children.length, 1, 'advanced tier exposes the pair action');
    botRadio.checked = true;
    botRadio.listeners.get('change')();
    assert.equal(botForm?.hidden, false, 'switching back restores the bot form');
    assert.equal(section.querySelector('.vault-hint')?.hidden, true);
    assert.equal(section.querySelector('.vault-actions').children.length, 0);
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('bot pairing flow: token -> deep link -> retry wait -> paired', async () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  let statusResult = statuses()[0];
  let waitResult = { ok: true, message: 'No bot chat captured yet; ask again later.', data: { chatCaptured: false } };
  const calls = [];
  const invoke = (command, args) => {
    calls.push([command, args]);
    if (command === 'vault_get_status') return Promise.resolve({ ok: true, data: statusResult });
    if (command === 'vault_get_log') return Promise.resolve({ ok: true, data: [] });
    if (command === 'vault_pair_bot') {
      return Promise.resolve({
        ok: true,
        data: { botUsername: 'nexadock_vault_test_bot', deepLink: 'https://t.me/nexadock_vault_test_bot?start=vault' },
      });
    }
    if (command === 'vault_pair_bot_wait') return Promise.resolve(waitResult);
    return Promise.resolve({ ok: false, message: `unexpected command: ${command}` });
  };
  const flush = () => new Promise((resolve) => setTimeout(resolve, 0));
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke } };
  try {
    const section = renderSection();
    await flush();
    panel.applyStatus(statuses()[0], '');

    const botForm = section.querySelector('.vault-form-bot');
    const tokenInput = section.querySelector('.vault-bot-token');
    tokenInput.value = '123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ';
    botForm.listeners.get('submit')({ preventDefault() {} });
    await flush();

    const pairCall = calls.find(([command]) => command === 'vault_pair_bot');
    assert.ok(pairCall, 'submitting the bot form must call vault_pair_bot');
    assert.equal(pairCall[1].profileId, profile.id);
    assert.equal(pairCall[1].token, '123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ');
    assert.equal(tokenInput.value, '', 'bot token must be cleared from the input after pairing');

    const linkStep = section.querySelector('.vault-bot-link');
    assert.equal(linkStep?.hidden, false, 'deep-link step must show after vault_pair_bot');
    assert.equal(botForm.hidden, true, 'token form must hide once the link step shows');
    const anchor = linkStep.querySelector('.vault-bot-link-anchor');
    assert.equal(anchor?.getAttribute('href'), 'https://t.me/nexadock_vault_test_bot?start=vault');
    assert.equal(anchor?.getAttribute('target'), '_blank');
    assert.equal(anchor?.getAttribute('rel'), 'noopener');
    assert.equal(anchor?.textContent, 'nexadock_vault_test_bot');
    assert.equal(linkStep.querySelector('.vault-bot-wait')?.textContent, viLocale.vault.bot.startConfirm);

    // Wait #1: the 60s window passed without a captured chat.
    const waitButton = linkStep.querySelector('.vault-bot-wait');
    waitButton.listeners.get('click')();
    await flush();
    let waitCall = calls.find(([command]) => command === 'vault_pair_bot_wait');
    assert.ok(waitCall, 'confirm button must call vault_pair_bot_wait');
    assert.equal(waitCall[1].profileId, profile.id);
    assert.equal(section.querySelector('.vault-error')?.textContent, viLocale.vault.bot.notCaptured);
    assert.equal(waitButton.textContent, viLocale.vault.bot.waitAgain, 'retry affordance must appear after a missed Start');
    assert.equal(waitButton.disabled, false);
    assert.equal(linkStep.hidden, false, 'link step must stay visible after a missed Start');

    // Wait #2: chat captured -> panel transitions to paired through applyStatus.
    statusResult = { paired: true, transport: 'bot', enabled: true, engineRunning: false, pending: 0, failed: 0, done: 0 };
    waitResult = { ok: true, data: { ...statusResult, chatCaptured: true } };
    waitButton.listeners.get('click')();
    await flush();
    waitCall = calls.filter(([command]) => command === 'vault_pair_bot_wait').at(-1);
    assert.ok(waitCall, 'second confirm must call vault_pair_bot_wait again');
    const transport = section.querySelector('.vault-transport');
    assert.equal(transport?.hidden, false, 'paired panel must reveal the transport label');
    assert.equal(transport?.textContent, viLocale.vault.transport.bot);
    assert.equal(section.querySelector('.vault-state')?.textContent, viLocale.vault.states.paired);
    assert.equal(section.querySelector('.vault-mode-chooser')?.hidden, true, 'mode chooser must hide once paired');
    assert.equal(section.querySelector('.vault-bot-link')?.hidden, true, 'link step must hide once paired');
    assert.equal(section.querySelector('.vault-form-bot')?.hidden, true);
    assert.equal(section.querySelector('.vault-error')?.textContent, '', 'pairing errors must clear on success');
    assert.ok(section.querySelector('.vault-actions').children.length > 0, 'paired state exposes managed actions');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});

test('paired panel labels the active transport tier, defaulting legacy status to mtproto', () => {
  const originalDocument = globalThis.document;
  const originalWindow = globalThis.window;
  globalThis.document = documentMock;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: stubDesktopInvoke } };
  try {
    const section = renderSection();
    panel.applyStatus({ paired: true, enabled: true, engineRunning: false, transport: 'mtproto' }, '');
    assert.equal(section.querySelector('.vault-transport')?.textContent, viLocale.vault.transport.mtproto);
    panel.applyStatus({ paired: true, enabled: true, engineRunning: false }, '');
    assert.equal(
      section.querySelector('.vault-transport')?.textContent,
      viLocale.vault.transport.mtproto,
      'legacy paired status without transport defaults to mtproto'
    );
    panel.applyStatus({ paired: true, enabled: true, engineRunning: false, transport: 'bot' }, '');
    assert.equal(section.querySelector('.vault-transport')?.textContent, viLocale.vault.transport.bot);
    panel.applyStatus(statuses()[0], '');
    assert.equal(section.querySelector('.vault-transport')?.hidden, true, 'unpaired status hides the tier label');
  } finally {
    globalThis.document = originalDocument;
    if (originalWindow === undefined) delete globalThis.window;
    else globalThis.window = originalWindow;
  }
});
