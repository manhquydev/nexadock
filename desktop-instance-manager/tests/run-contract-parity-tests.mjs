#!/usr/bin/env node
/**
 * Contract parity verification suite comparing:
 *   - Track A: PowerShell Signed-Host Bridge (`scripts/dim-host.ps1`)
 *   - Track B: Native Rust CLI (`dim-cli.exe`)
 *
 * Exercises all 12 core operations and security guards on identical synthetic fixtures.
 */

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { execFileSync, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..');
const HOST_SCRIPT = path.resolve(REPO_ROOT, 'scripts', 'dim-host.ps1');
const FIXTURE_SCRIPT = path.resolve(REPO_ROOT, 'fixtures', 'test-desktop-app', 'fixture-app.mjs');

function parseArgs(argv) {
  let outPath = null;
  let rustBin = null;
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--out' && argv[i + 1]) outPath = path.resolve(argv[++i]);
    else if (argv[i] === '--rust-bin' && argv[i + 1]) rustBin = path.resolve(argv[++i]);
  }
  if (!rustBin) {
    const ext = process.platform === 'win32' ? '.exe' : '';
    const releaseCand = path.join(REPO_ROOT, 'target', 'release', `dim-cli${ext}`);
    const debugCand = path.join(REPO_ROOT, 'target', 'debug', `dim-cli${ext}`);
    rustBin = fs.existsSync(releaseCand) ? releaseCand : debugCand;
  }
  return { outPath, rustBin };
}

function runRunner(kind, rustBin, dataDir, cliArgs, { extraArgs = [], expectOk = true } = {}) {
  let res;
  if (kind === 'powershell') {
    const args = [
      '-NoProfile',
      '-ExecutionPolicy', 'Bypass',
      '-File', HOST_SCRIPT,
      '--cli',
      '--json',
      '--data-dir', dataDir,
      '--node-fixture',
      '--app-path', FIXTURE_SCRIPT,
      ...extraArgs,
      ...cliArgs,
    ];
    res = spawnSync('powershell.exe', args, { encoding: 'utf8', windowsHide: true });
  } else {
    const args = [
      '--cli',
      '--json',
      '--data-dir', dataDir,
      '--node-fixture',
      '--app-path', FIXTURE_SCRIPT,
      ...extraArgs,
      ...cliArgs,
    ];
    res = spawnSync(rustBin, args, { encoding: 'utf8', windowsHide: true });
  }

  const stdout = (res.stdout || '').trim();
  const stderr = (res.stderr || '').trim();
  let json = null;
  try {
    json = stdout ? JSON.parse(stdout) : null;
  } catch (err) {
    throw new Error(`[${kind}] Invalid JSON (${cliArgs.join(' ')}): ${err.message}\nSTDOUT:\n${stdout}\nSTDERR:\n${stderr}`);
  }
  if (expectOk) {
    assert.equal(res.status, 0, `[${kind}] Expected exit 0 for ${cliArgs.join(' ')}, got ${res.status}: ${stdout} ${stderr}`);
    assert.equal(json?.ok, true, `[${kind}] Expected ok=true for ${cliArgs.join(' ')}: ${stdout}`);
  }
  return { status: res.status, json };
}

function runRawApp(kind, rustBin, dataDir, appPath, cliArgs) {
  let res;
  if (kind === 'powershell') {
    res = spawnSync('powershell.exe', [
      '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', HOST_SCRIPT,
      '--cli', '--json', '--data-dir', dataDir, '--app-path', appPath, '--', ...cliArgs
    ], { encoding: 'utf8', windowsHide: true });
  } else {
    res = spawnSync(rustBin, [
      '--cli', '--json', '--data-dir', dataDir, '--app-path', appPath, '--', ...cliArgs
    ], { encoding: 'utf8', windowsHide: true });
  }
  return JSON.parse((res.stdout || '').trim());
}

function exerciseSingleImplementation(kind, rustBin, workDir, tamperedExe) {
  const dataDir = path.join(workDir, 'data');
  const run = (cliArgs, opts) => runRunner(kind, rustBin, dataDir, cliArgs, opts);
  const observed = {};

  // 1. Discover (node-fixture + tampered PE)
  const discFix = run(['discover']).json;
  assert.equal(discFix.valid, true);
  assert.equal(discFix.mode, 'node-fixture');
  const discTampered = runRawApp(kind, rustBin, dataDir, tamperedExe, ['discover']);
  assert.equal(discTampered.valid, false, `[${kind}] tampered PE must report valid=false`);
  assert.match(discTampered.authenticode_status, /^Unverified/);
  observed.discover = {
    fixture_valid: discFix.valid,
    tampered_valid: discTampered.valid,
    tampered_status: discTampered.authenticode_status,
  };

  // 2. Create + reject launch of unverified custom PE + invalid name
  const p1 = run(['create', 'Đức - Công việc', '--notes', 'parity']).json.data;
  assert.match(p1.id, /^[0-9a-f]{8}$/);
  assert.match(p1.directory_name, /^duc-cong-viec_[0-9a-f]{8}$/);
  const tamperedLaunch = runRawApp(kind, rustBin, dataDir, tamperedExe, ['launch', p1.id]);
  assert.equal(tamperedLaunch.ok, false, `[${kind}] tampered custom PE launch must fail closed`);
  assert.equal(tamperedLaunch.error_code, 'UNVERIFIED_BINARY');
  const p2 = run(['create', 'Tài khoản Hai']).json.data;
  const badName = run(['--', 'create', '../evil'], { expectOk: false }).json;
  assert.equal(badName.error_code, 'INVALID_NAME');
  observed.create = { dir_prefix: p1.directory_name.split('_')[0], invalid_name_code: badName.error_code };

  // 3. Rename
  const ren = run(['rename', p1.id, 'Đức - Renamed']).json.data;
  assert.equal(ren.name, 'Đức - Renamed');
  assert.equal(ren.directory_name, p1.directory_name);
  observed.rename = { preserved_dir: ren.directory_name === p1.directory_name };

  // 4. Status + Focus while stopped
  const focusStopped = run(['focus', p1.id], { expectOk: false }).json;
  assert.equal(focusStopped.error_code, 'NOT_RUNNING');
  const stBefore = run(['status', '--with-size']).json;
  assert.equal(stBefore.profiles.length, 2);
  observed.focus_stopped_code = focusStopped.error_code;

  // 5. Launch + Focus while running + Backup while running blocked + Delete while running blocked
  const l1 = run(['launch', p1.id]).json;
  assert.ok(l1.data.pid > 0);
  const stRunning = run(['status', '--with-size']).json;
  const p1State = stRunning.profiles.find((p) => p.id === p1.id);
  assert.equal(p1State.running, true);
  assert.ok(Number.isFinite(p1State.data_size_bytes));

  const focusRun = run(['focus', p1.id]).json;
  assert.equal(focusRun.ok, true);

  const runningBk = run(['backup', p1.id], { expectOk: false }).json;
  assert.equal(runningBk.error_code, 'PROFILE_RUNNING');

  const runningDel = run(['delete', p1.id, '--confirm'], { expectOk: false }).json;
  assert.equal(runningDel.error_code, 'PROFILE_RUNNING');

  // 6. Duplicate zalo_user_id guard across profiles
  const p1Uid = p1State.zalo_user_id;
  assert.ok(p1Uid && /^\d{6,}$/.test(p1Uid), `Expected numeric zalo_user_id, got ${p1Uid}`);
  const p2UidDir = path.join(dataDir, 'Profiles', p2.directory_name, 'AppData', 'Roaming', 'ZaloData', 'Database', '_production', p1Uid);
  fs.mkdirSync(p2UidDir, { recursive: true });
  const dupBlocked = run(['launch', p2.id], { expectOk: false }).json;
  assert.equal(dupBlocked.error_code, 'DUPLICATE_ZALO_USER_ID');

  // 7. Verify Isolation + Diagnostics
  const iso = run(['verify-isolation']).json;
  assert.equal(iso.all_passed, true, `[${kind}] verify-isolation failed: ${JSON.stringify(iso)}`);
  assert.ok(iso.checks.every((c) => c.status === 'PASS'));
  const diag = run(['diagnostics']).json;
  assert.equal(diag.ok, true);
  assert.equal(typeof diag.is_elevated, 'boolean');
  assert.equal(diag.disk_free_ok, true);
  assert.equal(diag.profiles_acl_safe, true);
  observed.verify_isolation_checks = iso.checks.map((c) => `${c.check_name}:${c.status}`);
  observed.diagnostics = { is_elevated: diag.is_elevated, sac: diag.smart_app_control_state, acl: diag.profiles_acl_safe };

  // 8. Stop
  run(['stop', p1.id]);
  const stStopped = run(['status']).json;
  assert.equal(stStopped.profiles.find((p) => p.id === p1.id).running, false);

  // 9. Backup + Backups list + Restore
  const bk = run(['backup', p1.id]).json.data;
  assert.ok(bk.backup_dir && fs.existsSync(path.join(bk.backup_dir, 'backup.json')));
  const bks = run(['backups']).json.backups;
  assert.ok(bks.some((b) => b.backup_id === bk.backup_id));

  const tamperedBk = path.join(workDir, 'tampered-bk');
  fs.cpSync(bk.backup_dir, tamperedBk, { recursive: true });
  fs.writeFileSync(path.join(tamperedBk, 'AppData', 'Roaming', 'zmeta.json'), '{"tampered":true}', 'utf8');
  const badRestore = run(['restore', tamperedBk], { expectOk: false }).json;
  assert.equal(badRestore.error_code, 'BACKUP_ERROR');

  const restored = run(['restore', bk.backup_dir, '--name', 'Restored Parity']).json.data;
  assert.notEqual(restored.id, p1.id);
  const restoredMarker = fs.readFileSync(
    path.join(dataDir, 'Profiles', restored.directory_name, 'AppData', 'Roaming', 'ZaloData', '.zalo-profile-id'),
    'utf8'
  ).trim();
  assert.equal(restoredMarker, restored.id);

  // 10. Delete without confirm + Delete with junction protection
  const delNoConf = run(['delete', p2.id], { expectOk: false }).json;
  assert.equal(delNoConf.error_code, 'CONFIRM_REQUIRED');
  const outsideDir = path.join(workDir, 'outside');
  fs.mkdirSync(outsideDir, { recursive: true });
  fs.writeFileSync(path.join(outsideDir, 'keep.txt'), 'SAFE', 'utf8');
  if (process.platform === 'win32') {
    const junction = path.join(dataDir, 'Profiles', p2.directory_name, 'Desktop', 'ExtLink');
    execFileSync('cmd.exe', ['/c', 'mklink', '/J', junction, outsideDir], { windowsHide: true });
  }
  run(['delete', p2.id, '--confirm']);
  assert.equal(fs.readFileSync(path.join(outsideDir, 'keep.txt'), 'utf8'), 'SAFE');

  // 11. Traversal & Protected primary guards
  const travBk = run(['backup', p1.id, '--dest', '../escape'], { expectOk: false }).json;
  assert.equal(travBk.error_code, 'PATH_TRAVERSAL');
  const travRest = run(['restore', '../escape'], { expectOk: false }).json;
  assert.equal(travRest.error_code, 'PATH_TRAVERSAL');
  const travRoot = runRunner(kind, rustBin, '../escape', ['status'], { expectOk: false }).json;
  assert.equal(travRoot.error_code, 'PATH_TRAVERSAL');

  const primPath = process.platform === 'win32'
    ? path.join(process.env.APPDATA, 'ZaloData')
    : path.join(os.homedir(), '.config', 'ZaloData');
  const primBlocked = runRunner(kind, rustBin, primPath, ['status'], { expectOk: false }).json;
  assert.equal(primBlocked.error_code, 'PROTECTED_PRIMARY_PATH');

  run(['delete', p1.id, '--confirm']);
  run(['delete', restored.id, '--confirm']);

  observed.guards = {
    running_backup: runningBk.error_code,
    running_delete: runningDel.error_code,
    dup_uid: dupBlocked.error_code,
    tampered_restore: badRestore.error_code,
    confirm_required: delNoConf.error_code,
    traversal: travBk.error_code,
    primary_path: primBlocked.error_code,
  };
  return observed;
}

function main() {
  const { outPath, rustBin } = parseArgs(process.argv.slice(2));
  assert.ok(fs.existsSync(rustBin), `Rust dim-cli binary not found at ${rustBin}. Build workspace first.`);

  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'dim-contract-parity-'));
  try {
    const tamperedExe = path.join(tempRoot, 'tampered-node.exe');
    fs.copyFileSync(process.execPath, tamperedExe);
    const buf = fs.readFileSync(tamperedExe);
    buf[buf.length - 10] ^= 0xff;
    fs.writeFileSync(tamperedExe, buf);

    const rustObs = exerciseSingleImplementation('rust', rustBin, path.join(tempRoot, 'rust'), tamperedExe);
    let psObs = null;
    if (process.platform === 'win32') {
      psObs = exerciseSingleImplementation('powershell', rustBin, path.join(tempRoot, 'ps'), tamperedExe);
      assert.deepEqual(rustObs.guards, psObs.guards, 'Error-code guards must match between Rust and PowerShell');
      assert.deepEqual(rustObs.verify_isolation_checks, psObs.verify_isolation_checks, 'Isolation check set/statuses must match');
      assert.equal(rustObs.diagnostics.is_elevated, psObs.diagnostics.is_elevated);
      assert.equal(rustObs.diagnostics.sac, psObs.diagnostics.sac);
      assert.equal(rustObs.diagnostics.acl, psObs.diagnostics.acl);
    }

    const report = {
      ok: true,
      executed_at_utc: new Date().toISOString(),
      platform: process.platform,
      rust_cli_bin: path.relative(REPO_ROOT, rustBin).replace(/\\/g, '/'),
      powershell_parity_tested: process.platform === 'win32',
      rust: rustObs,
      powershell: psObs,
    };

    const jsonText = JSON.stringify(report, null, 2);
    if (outPath) {
      fs.mkdirSync(path.dirname(outPath), { recursive: true });
      fs.writeFileSync(outPath, `${jsonText}\n`, 'utf8');
    }
    console.log(jsonText);
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

main();
