// zalo-linux-shim.js — Linux compatibility layer for the repackaged Zalo macOS client.
// Loaded at the very top of bootstrap.js, before any other require.
// Design: never patch the Zalo bundles; only adapt the Node/Electron environment.
'use strict';

if (globalThis.__ZALO_LINUX_SHIM__) return; // idempotent across double-loads
globalThis.__ZALO_LINUX_SHIM__ = true;

const Module = require('module');
const path = require('path');
const fs = require('fs');

const IS_LINUX = process.platform === 'linux';

// --- 1. userData override for multi-instance (before migration.js sets it) ---
// ZALO_DATA_DIR=/path  → instance-specific userData; distinct dirs bypass
// requestSingleInstanceLock (Electron keys that lock on userData).
const dataDir = process.env.ZALO_DATA_DIR;
if (dataDir) {
  let electron;
  try { electron = require('electron'); } catch { electron = null; }
  if (electron && electron.app) {
    const origSetPath = electron.app.setPath.bind(electron.app);
    electron.app.setPath = function (name, p) {
      if (name === 'userData') {
        fs.mkdirSync(dataDir, { recursive: true });
        return origSetPath('userData', dataDir);
      }
      return origSetPath(name, p);
    };
  }
}

if (!IS_LINUX) return;

// --- 2. Stub native addons whose prebuilt binaries are darwin/win32-only ---
// Linux produces two failure modes the app does not guard:
//   a) dlopen errors for Mach-O/PE binaries shipped for other platforms;
//   b) MODULE_NOT_FOUND for `napi-v6-linux-*` prebuilt dirs that were never
//      packaged (sqlite3, db-cross-v4). Both are stubbed here.
const utilInspectCustom = Symbol.for('nodejs.util.inspect.custom');
const stubCache = new Map(); // request -> single stable stub
function makeStub(request) {
  let stub = stubCache.get(request);
  if (stub) return stub;
  const fn = function stubFn () {};
  stub = new Proxy(fn, {
    get: (t, prop) => {
      if (prop === Symbol.toPrimitive) return () => 0;
      if (prop === 'then') return undefined;
      if (prop === utilInspectCustom) return () => `[ZaloLinuxStub ${request}]`;
      return makeStub(request); // stable identity per (request, prop)
    },
    construct: () => makeStub(request),
    apply: (t, thisArg, args) => {
      // Make failures visible: if the caller passed a trailing callback
      // (Node addon convention), invoke it with an error instead of hanging.
      const last = args[args.length - 1];
      if (typeof last === 'function') {
        try { last(new Error(`zalo-linux: native module stubbed on Linux (${request})`)); } catch {}
      }
      return makeStub(request);
    },
    has: () => true,
    getOwnPropertyDescriptor: (t, prop) =>
      Reflect.getOwnPropertyDescriptor(t, prop) || {
        value: makeStub(request), writable: true, enumerable: true, configurable: true,
      },
  });
  stubCache.set(request, stub);
  return stub;
}

const origLoad = Module._load;
Module._load = function (request, parent, isMain) {
  try {
    return origLoad.apply(this, arguments);
  } catch (err) {
    const isNativeAddon = /\.node(\?|$)/.test(String(request)) ||
      /\.node$/.test(String(err?.message?.match(/Cannot find module '([^']+\.node)'/)?.[1] || ''));
    const isMissingPrebuilt = err?.code === 'MODULE_NOT_FOUND' &&
      /\.node/.test(String(err?.message ?? ''));
    if (isNativeAddon && (isMissingPrebuilt ||
        /Could not locate the bindings file|was compiled against a different|dlopen|ELF|mach-o|not a valid Win|cannot open shared object/i.test(`${err?.message ?? ''}`))) {
      process.env.ZALO_LINUX_STUBBED = (process.env.ZALO_LINUX_STUBBED || '') + request + '\n';
      return makeStub(String(request));
    }
    throw err;
  }
};
