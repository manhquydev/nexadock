import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  evaluateArchitectureSlices,
  evaluateOtoolDependencies,
  evaluateSigningVerdict,
  parseCodesignMetadata,
} from '../scripts/ci-cross-platform/macos-release-acceptance.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const dimRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(dimRoot, '..');
const tauriDir = path.join(dimRoot, 'apps', 'desktop', 'src-tauri');

test('tauri.conf.json enables macOS hardenedRuntime and minimal Entitlements.plist', () => {
  const conf = JSON.parse(fs.readFileSync(path.join(tauriDir, 'tauri.conf.json'), 'utf8'));
  assert.equal(conf.bundle?.macOS?.minimumSystemVersion, '11.0');
  assert.equal(conf.bundle?.macOS?.hardenedRuntime, true);
  assert.equal(conf.bundle?.macOS?.entitlements, 'Entitlements.plist');

  const plistPath = path.join(tauriDir, 'Entitlements.plist');
  assert.ok(fs.existsSync(plistPath), 'Entitlements.plist must exist');
  const plist = fs.readFileSync(plistPath, 'utf8');
  assert.ok(plist.includes('<plist version="1.0">'));
  assert.match(plist, /<dict>\s*<\/dict>/, 'Entitlements.plist must remain minimal (empty <dict>)');
  assert.doesNotMatch(plist, /com\.apple\.security\.app-sandbox/);
  assert.doesNotMatch(plist, /com\.apple\.security\.cs\.disable-library-validation/);
});

test('release.yml enforces fail-closed Apple Developer signing/notarization and acceptance gate', () => {
  const workflow = fs.readFileSync(path.join(repoRoot, '.github', 'workflows', 'release.yml'), 'utf8');
  assert.ok(workflow.includes('APPLE_CERTIFICATE'));
  assert.ok(workflow.includes('APPLE_CERTIFICATE_PASSWORD'));
  assert.ok(workflow.includes('APPLE_SIGNING_IDENTITY'));
  assert.ok(workflow.includes('APPLE_API_KEY'));
  assert.ok(workflow.includes('APPLE_API_ISSUER'));
  assert.ok(workflow.includes('APPLE_API_KEY_PATH'));
  assert.ok(workflow.includes('xcrun notarytool submit'));
  assert.ok(workflow.includes('xcrun stapler staple'));
  assert.ok(workflow.includes('macos-release-acceptance.mjs'));
  assert.ok(workflow.includes('ACCEPT_MODE="production"'));
  assert.ok(workflow.includes('--mode "${ACCEPT_MODE}"'));
  assert.doesNotMatch(workflow, /xcrun altool/);
});

test('parseCodesignMetadata distinguishes ad-hoc signatures from Developer ID Hardened Runtime', () => {
  const adhocOut = [
    'Executable=/Applications/NexaDock.app/Contents/MacOS/NexaDock',
    'Identifier=com.multizalodesktop.manager',
    'Format=app bundle with Mach-O thin (arm64)',
    'CodeDirectory v=20400 size=1234 flags=0x2(adhoc) hashes=32+2 location=embedded',
    'Signature=adhoc',
    'TeamIdentifier=not set',
  ].join('\n');
  const adhocMeta = parseCodesignMetadata(adhocOut);
  assert.equal(adhocMeta.is_adhoc, true);
  assert.equal(adhocMeta.team_identifier, null);
  assert.equal(adhocMeta.has_developer_id, false);
  assert.equal(adhocMeta.has_hardened_runtime, false);

  const signedOut = [
    'Executable=/Applications/NexaDock.app/Contents/MacOS/NexaDock',
    'Identifier=com.multizalodesktop.manager',
    'Format=app bundle with Mach-O thin (arm64)',
    'CodeDirectory v=20500 size=1234 flags=0x10000(runtime) hashes=32+7 location=embedded',
    'Authority=Developer ID Application: Example Org (ABCD123456)',
    'Authority=Developer ID Certification Authority',
    'Authority=Apple Root CA',
    'TeamIdentifier=ABCD123456',
  ].join('\n');
  const signedMeta = parseCodesignMetadata(signedOut);
  assert.equal(signedMeta.is_adhoc, false);
  assert.equal(signedMeta.team_identifier, 'ABCD123456');
  assert.equal(signedMeta.has_developer_id, true);
  assert.equal(signedMeta.has_hardened_runtime, true);
});

test('evaluateArchitectureSlices rejects Intel-named artifact containing only arm64', () => {
  const armOnly = evaluateArchitectureSlices('arm64', 'x86_64');
  assert.equal(armOnly.ok, false);
  assert.match(armOnly.reason, /x86_64/);

  const armMatch = evaluateArchitectureSlices('arm64', 'arm64');
  assert.equal(armMatch.ok, true);

  const universalMatch = evaluateArchitectureSlices('x86_64 arm64', 'arm64');
  assert.equal(universalMatch.ok, true);
});

test('evaluateOtoolDependencies flags non-system Homebrew or local dylibs', () => {
  const cleanOtool = [
    '/Applications/NexaDock.app/Contents/MacOS/NexaDock:',
    '\t/System/Library/Frameworks/Cocoa.framework/Versions/A/Cocoa (compatibility version 1.0.0)',
    '\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0)',
  ].join('\n');
  assert.equal(evaluateOtoolDependencies(cleanOtool).ok, true);

  const dirtyOtool = [
    '/Applications/NexaDock.app/Contents/MacOS/NexaDock:',
    '\t/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib (compatibility version 3.0.0)',
  ].join('\n');
  const res = evaluateOtoolDependencies(dirtyOtool);
  assert.equal(res.ok, false);
  assert.deepEqual(res.non_system_dependencies, ['/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib']);
});

test('evaluateSigningVerdict fails closed on any missing signing, stapling, or Gatekeeper requirement', () => {
  const validMeta = {
    is_adhoc: false,
    has_developer_id: true,
    team_identifier: 'ABCD123456',
    has_hardened_runtime: true,
  };

  const fullPass = evaluateSigningVerdict(validMeta, true, true, true, true);
  assert.equal(fullPass.passed, true);
  assert.equal(fullPass.signing_status, 'SIGNED_AND_NOTARIZED');

  const cases = [
    ['codesignVerifyOk=false', evaluateSigningVerdict(validMeta, false, true, true, true)],
    ['spctlOk=false', evaluateSigningVerdict(validMeta, true, false, true, true)],
    ['dmgStapled=false', evaluateSigningVerdict(validMeta, true, true, false, true)],
    ['appStapled=false', evaluateSigningVerdict(validMeta, true, true, true, false)],
    ['is_adhoc=true', evaluateSigningVerdict({ ...validMeta, is_adhoc: true }, true, true, true, true)],
    ['has_developer_id=false', evaluateSigningVerdict({ ...validMeta, has_developer_id: false }, true, true, true, true)],
    ['team_identifier=null', evaluateSigningVerdict({ ...validMeta, team_identifier: null }, true, true, true, true)],
    ['has_hardened_runtime=false', evaluateSigningVerdict({ ...validMeta, has_hardened_runtime: false }, true, true, true, true)],
  ];
  for (const [label, res] of cases) {
    assert.equal(res.passed, false, `Expected failure for ${label}`);
    assert.equal(res.signing_status, 'UNSIGNED_DEVELOPMENT_ARTIFACT', `Expected unsigned status for ${label}`);
  }
});
