import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

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

test('vault panel never uses innerHTML', () => {
  assert.doesNotMatch(panelSource, /innerHTML/, 'vault panel must not use innerHTML');
  assert.match(panelSource, /createElement/, 'vault panel must build DOM nodes');
  assert.match(panelSource, /textContent/, 'vault panel must set text nodes');
});

// ---- Minimal DOM mock so render() executes for the three lifecycle states.

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

  removeAttribute() {}

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
  path.join('file://', uiDir, 'vault-panel.js')
);

const profile = { id: 'work_1', name: 'Work' };

function statuses() {
  return [
    { paired: false, enabled: false, engineRunning: false, pending: 0, failed: 0, done: 0 },
    { paired: true, enabled: true, engineRunning: true, uploading: 2, pending: 1, failed: 1, done: 10 },
    { paired: true, enabled: false, engineRunning: false, uploading: 0, pending: 3, failed: 0, done: 5 },
  ];
}

test('vault panel renders unpaired, syncing, and offline states', () => {
  const originalDocument = globalThis.document;
  globalThis.document = documentMock;
  try {
    for (const status of statuses()) {
      const before = dialog.children.length;
      panel.render(profile);
      const section = dialog.children.at(-1);
      assert.ok(section, 'vault panel section must be attached to the details dialog');
      assert.equal(section.id, PANEL_ID, 'section must carry the stable panel id');
      assert.ok(dialog.children.length >= before, 'section must be attached to the details dialog');
      panel.applyStatus(status, '');
      const apiIdInput = section.querySelector('.vault-api-id');
      const apiHashInput = section.querySelector('.vault-api-hash');
      assert.equal(apiIdInput?.inputMode, 'numeric');
      assert.equal(apiHashInput?.type, 'password');
      assert.equal(apiHashInput?.autocomplete, 'off');
      assert.equal(section.querySelector('.vault-credentials-fields')?.hidden, false);
      const stateText = section.querySelector('.vault-state')?.textContent || '';
      assert.notEqual(stateText, '', 'state label must resolve through i18n');
      assert.equal(stateText === stateText.trim(), true, 'state label must not be padded');
      const actions = section.querySelector('.vault-actions');
      assert.ok(actions, 'actions row must exist');
      assert.ok(actions.children.length > 0, 'state must expose at least one action');
    }
    // Unpaired state must show exactly the pair action.
    panel.render(profile);
    const section = dialog.children.at(-1);
    panel.applyStatus(statuses()[0], '');
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
  }
});

test('stateFor maps the three lifecycle states deterministically', () => {
  assert.equal(stateFor(null), 'unpaired');
  assert.equal(stateFor({ paired: true, enabled: true, engineRunning: true, uploading: 0 }), 'paired');
  assert.equal(stateFor({ paired: true, enabled: true, engineRunning: true, uploading: 1 }), 'syncing');
  assert.equal(stateFor({ paired: true, enabled: false }), 'offline');
});
