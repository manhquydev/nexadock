#!/usr/bin/env node
/**
 * Comprehensive integration, safety, and Modern Web Guidance test suite
 * executed via the signed-host bridge (powershell.exe + node.exe) in isolated temp storage.
 */

import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { execFileSync, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { startUiServer } from '../apps/desktop/ui-server.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..');
const HOST_SCRIPT = path.resolve(REPO_ROOT, 'scripts', 'dim-host.ps1');
const FIXTURE_SCRIPT = path.resolve(REPO_ROOT, 'fixtures', 'test-desktop-app', 'fixture-app.mjs');
const UI_DIR = path.resolve(REPO_ROOT, 'apps', 'desktop', 'ui');

function parseOutPath(argv) {
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--out' && argv[i + 1]) {
      return path.resolve(argv[i + 1]);
    }
  }
  return null;
}

function runHost(dataDir, hostArgs, { extraArgs = [], expectOk = true } = {}) {
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
    ...hostArgs,
  ];
  const res = spawnSync('powershell.exe', args, { encoding: 'utf8', windowsHide: true });
  const stdout = (res.stdout || '').trim();
  const stderr = (res.stderr || '').trim();
  let json = null;
  try {
    json = stdout ? JSON.parse(stdout) : null;
  } catch (err) {
    throw new Error(`Invalid JSON from dim-host.ps1 (${hostArgs.join(' ')}): ${err.message}\nSTDOUT:\n${stdout}\nSTDERR:\n${stderr}`);
  }
  if (expectOk) {
    assert.equal(res.status, 0, `Expected exit 0 for ${hostArgs.join(' ')}, got ${res.status}: ${stdout} ${stderr}`);
    assert.equal(json?.ok, true, `Expected ok=true for ${hostArgs.join(' ')}: ${stdout}`);
  }
  return { status: res.status, json, stdout, stderr };
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

async function runSuite() {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'dim-fixture-tests-'));
  const dataDir = path.join(tempRoot, 'data');
  const results = [];
  let uiServerHandle = null;

  const record = async (id, name, fn) => {
    const start = Date.now();
    try {
      const details = await fn();
      results.push({ id, name, status: 'PASS', duration_ms: Date.now() - start, details: details || {} });
    } catch (err) {
      results.push({ id, name, status: 'FAIL', duration_ms: Date.now() - start, error: err.stack || err.message });
      throw err;
    }
  };

  try {
    // 1. Profile CRUD & Vietnamese slugification
    let p1;
    let p2;
    let p3;
    await record('Test-1', 'Profile CRUD, Schema & Vietnamese Slugification', async () => {
      const c1 = runHost(dataDir, ['create', 'Đức - Công việc', '--notes', 'Primary test account']);
      p1 = c1.json.data;
      assert.match(p1.id, /^[0-9a-f]{8}$/);
      assert.match(p1.directory_name, /^duc-cong-viec_[0-9a-f]{8}$/);
      assert.equal(p1.last_launched_at, null);
      assert.equal(p1.zalo_user_id, null);

      const storeDoc = readJson(path.join(dataDir, 'profiles.json'));
      assert.equal(storeDoc.schema_version, 1);
      assert.equal(storeDoc.profiles.length, 1);

      const p1Dir = path.join(dataDir, 'Profiles', p1.directory_name);
      assert.deepEqual(readJson(path.join(p1Dir, 'AppData', 'Roaming', 'zmeta.json')), { failedMigrateCount: 0 });
      assert.deepEqual(readJson(path.join(p1Dir, 'AppData', 'Roaming', 'ZaloData', 'migrate-config.json')), { zalopc_m_c: true });
      assert.equal(fs.readFileSync(path.join(p1Dir, 'AppData', 'Roaming', 'ZaloData', '.zalo-profile-id'), 'utf8').trim(), p1.id);

      const r1 = runHost(dataDir, ['rename', p1.id, 'Đức - Công việc (Updated)']);
      p1 = r1.json.data;
      assert.equal(p1.name, 'Đức - Công việc (Updated)');
      assert.match(p1.directory_name, /^duc-cong-viec_[0-9a-f]{8}$/);

      p2 = runHost(dataDir, ['create', 'Tài khoản Cá nhân']).json.data;
      p3 = runHost(dataDir, ['create', 'Hỗ trợ Khách hàng']).json.data;
      const st = runHost(dataDir, ['status']).json;
      assert.equal(st.profiles.length, 3);
      return { profile_ids: [p1.id, p2.id, p3.id], vietnamese_dir: p1.directory_name };
    });

    // 2. Launch 2 then 3 concurrent isolated fixture instances + focus + verify-isolation
    await record('Test-2', 'Concurrent Multi-Instance Launch (2 then 3) & Focus', async () => {
      const l1 = runHost(dataDir, ['launch', p1.id]);
      const l2 = runHost(dataDir, ['launch', p2.id]);
      assert.notEqual(l1.json.data.pid, l2.json.data.pid);

      const st2 = runHost(dataDir, ['status']).json;
      const running2 = st2.profiles.filter((p) => p.running);
      assert.equal(running2.length, 2);

      const l3 = runHost(dataDir, ['launch', p3.id]);
      const st3 = runHost(dataDir, ['status', '--with-size']).json;
      const running3 = st3.profiles.filter((p) => p.running);
      assert.equal(running3.length, 3);
      assert.equal(new Set(running3.map((p) => p.main_pid)).size, 3);
      assert.equal(new Set(running3.map((p) => p.zalo_user_id)).size, 3);

      const iso = runHost(dataDir, ['verify-isolation']).json;
      assert.equal(iso.all_passed, true, `verify-isolation failed: ${JSON.stringify(iso)}`);

      runHost(dataDir, ['focus', p1.id]);
      await sleep(180);
      const p1LaunchState = readJson(path.join(dataDir, 'Profiles', p1.directory_name, 'AppData', 'Roaming', 'ZaloData', 'launch-state.json'));
      assert.ok(p1LaunchState.focus_count >= 1, 'Expected focus_count >= 1 after focus command');

      return { pids: running3.map((p) => p.main_pid), focus_count: p1LaunchState.focus_count };
    });

    // 3. Single-instance lock collision test (raw duplicate in same isolated env exits 2)
    await record('Test-3', 'Single-Instance Lock Collision (Exit Code 2)', async () => {
      const p1Root = path.join(dataDir, 'Profiles', p1.directory_name);
      const p1Roaming = path.join(p1Root, 'AppData', 'Roaming');
      const p1Local = path.join(p1Root, 'AppData', 'Local');
      const p1Temp = path.join(p1Local, 'Temp');

      const stBefore = runHost(dataDir, ['status']).json;
      const p1Before = stBefore.profiles.find((p) => p.id === p1.id);
      assert.equal(p1Before.running, true);

      const dup = spawnSync(process.execPath, [FIXTURE_SCRIPT], {
        env: {
          ...process.env,
          USERPROFILE: p1Root,
          APPDATA: p1Roaming,
          LOCALAPPDATA: p1Local,
          TEMP: p1Temp,
          TMP: p1Temp,
          DIM_PROFILE_ID: p1.id,
        },
        encoding: 'utf8',
        windowsHide: true,
      });
      assert.equal(dup.status, 2, `Expected duplicate launch exit code 2, got ${dup.status}`);

      const startupLog = fs.readFileSync(path.join(p1Roaming, 'ZaloData', 'startup.log'), 'utf8');
      assert.match(startupLog, /SECOND_INSTANCE_REJECTED/);

      const stAfter = runHost(dataDir, ['status']).json;
      const p1After = stAfter.profiles.find((p) => p.id === p1.id);
      assert.equal(p1After.running, true);
      assert.equal(p1After.main_pid, p1Before.main_pid);

      // Stop P1, P2, P3 cleanly
      runHost(dataDir, ['stop', p1.id]);
      runHost(dataDir, ['stop', p2.id]);
      runHost(dataDir, ['stop', p3.id]);
      return { collision_exit_code: dup.status, primary_pid_preserved: p1After.main_pid };
    });

    // 4. 10 consecutive close/reopen cycles using graceful --z-update-force-quit
    let cycleProfile;
    await record('Test-4', '10 Consecutive Graceful Close/Reopen Cycles', async () => {
      cycleProfile = runHost(dataDir, ['create', 'Cycle 10 Account']).json.data;
      const zData = path.join(dataDir, 'Profiles', cycleProfile.directory_name, 'AppData', 'Roaming', 'ZaloData');

      for (let cycle = 1; cycle <= 10; cycle += 1) {
        const launchRes = runHost(dataDir, ['launch', cycleProfile.id]).json;
        assert.equal(launchRes.ok, true);

        const stRunning = runHost(dataDir, ['status']).json.profiles.find((p) => p.id === cycleProfile.id);
        assert.equal(stRunning.running, true, `Cycle ${cycle}: expected running=true`);

        const stopRes = runHost(dataDir, ['stop', cycleProfile.id]).json;
        assert.equal(stopRes.data.graceful_exit, true, `Cycle ${cycle}: expected graceful_exit=true via --z-update-force-quit`);
        assert.equal(fs.existsSync(path.join(zData, 'lockfile')), false, `Cycle ${cycle}: lockfile should be removed cleanly`);
      }

      const finalState = readJson(path.join(zData, 'launch-state.json'));
      assert.equal(finalState.launch_count, 10, `Expected launch_count == 10, got ${finalState.launch_count}`);
      assert.equal(finalState.clean_shutdown, true);
      return { cycles_completed: finalState.launch_count, clean_shutdown: finalState.clean_shutdown };
    });

    // 5. Crash recovery: --crash leaves stale lockfile, dim-host detects & recovers cleanly
    await record('Test-5', 'Crash Recovery & Stale Lockfile Cleanup', async () => {
      const crashProf = runHost(dataDir, ['create', 'Crash Recovery Test']).json.data;
      const zData = path.join(dataDir, 'Profiles', crashProf.directory_name, 'AppData', 'Roaming', 'ZaloData');

      runHost(dataDir, ['launch', crashProf.id], { extraArgs: ['--crash', '--crash-delay-ms', '80'] });
      await sleep(350);

      const stCrashed = runHost(dataDir, ['status']).json.profiles.find((p) => p.id === crashProf.id);
      assert.equal(stCrashed.running, false, 'Crashed process must report running=false');
      assert.equal(stCrashed.stale_lockfile_detected, true, 'Stale lockfile must be detected after crash');
      assert.equal(fs.existsSync(path.join(zData, 'lockfile')), true);
      assert.equal(readJson(path.join(zData, 'launch-state.json')).crashed, true);

      const recovered = runHost(dataDir, ['launch', crashProf.id]).json;
      assert.equal(recovered.data.crash_recovered, true, 'Expected crash_recovered=true on relaunch');
      const stRecovered = runHost(dataDir, ['status']).json.profiles.find((p) => p.id === crashProf.id);
      assert.equal(stRecovered.running, true);

      runHost(dataDir, ['stop', crashProf.id]);
      return { crash_recovered: recovered.data.crash_recovered, launch_count: readJson(path.join(zData, 'launch-state.json')).launch_count };
    });

    // 6. Duplicate zalo_user_id guard
    await record('Test-6', 'Duplicate zalo_user_id Guard', async () => {
      const dupA = runHost(dataDir, ['create', 'Duplicate UID A']).json.data;
      const dupB = runHost(dataDir, ['create', 'Duplicate UID B']).json.data;

      for (const prof of [dupA, dupB]) {
        const dbDir = path.join(dataDir, 'Profiles', prof.directory_name, 'AppData', 'Roaming', 'ZaloData', 'Database', '_production', '1234567');
        fs.mkdirSync(dbDir, { recursive: true });
        fs.writeFileSync(path.join(dbDir, 'session.json'), JSON.stringify({ zalo_user_id: '1234567' }), 'utf8');
      }

      runHost(dataDir, ['launch', dupA.id]);
      const blocked = runHost(dataDir, ['launch', dupB.id], { expectOk: false });
      assert.notEqual(blocked.status, 0);
      assert.equal(blocked.json.error_code, 'DUPLICATE_ZALO_USER_ID');

      runHost(dataDir, ['stop', dupA.id]);
      const allowedAfterStop = runHost(dataDir, ['launch', dupB.id]);
      assert.equal(allowedAfterStop.json.ok, true);
      runHost(dataDir, ['stop', dupB.id]);
      return { blocked_error_code: blocked.json.error_code };
    });

    // 7. Offline atomic backup & non-destructive restore with SHA-256 verification
    await record('Test-7', 'Offline Atomic Backup & Non-Destructive Restore', async () => {
      const customNotePath = path.join(dataDir, 'Profiles', cycleProfile.directory_name, 'AppData', 'Roaming', 'ZaloData', 'custom-notes.txt');
      fs.writeFileSync(customNotePath, 'preserve-me-10-cycles', 'utf8');

      // Refuses backup while profile is running (check using p1 so cycleProfile stays at launch_count == 10)
      runHost(dataDir, ['launch', p1.id]);
      const runningBackup = runHost(dataDir, ['backup', p1.id], { expectOk: false });
      assert.equal(runningBackup.json.error_code, 'PROFILE_RUNNING');
      runHost(dataDir, ['stop', p1.id]);

      const bkRes = runHost(dataDir, ['backup', cycleProfile.id]).json.data;
      assert.equal(fs.existsSync(bkRes.manifest_path), true);
      assert.equal(bkRes.backup_dir.endsWith('.partial'), false);
      const manifest = readJson(bkRes.manifest_path);
      assert.equal(manifest.schema_version, 1);
      assert.ok(manifest.file_count >= 5);
      assert.equal(manifest.files.some((f) => f.relative_path.endsWith('lockfile')), false);

      // Tamper detection on a copied backup
      const tamperedBackupDir = path.join(tempRoot, 'tampered-backup');
      fs.cpSync(bkRes.backup_dir, tamperedBackupDir, { recursive: true });
      fs.writeFileSync(path.join(tamperedBackupDir, 'AppData', 'Roaming', 'ZaloData', 'custom-notes.txt'), 'TAMPERED', 'utf8');
      const tamperedRestore = runHost(dataDir, ['restore', tamperedBackupDir], { expectOk: false });
      assert.equal(tamperedRestore.json.error_code, 'BACKUP_ERROR');

      // Non-destructive restore creates a brand-new profile
      const restored = runHost(dataDir, ['restore', bkRes.backup_dir, '--name', 'Restored Cycle Profile']).json.data;
      assert.notEqual(restored.id, cycleProfile.id);
      assert.notEqual(restored.directory_name, cycleProfile.directory_name);
      const restoredZData = path.join(dataDir, 'Profiles', restored.directory_name, 'AppData', 'Roaming', 'ZaloData');
      assert.equal(fs.readFileSync(path.join(restoredZData, '.zalo-profile-id'), 'utf8').trim(), restored.id);
      assert.equal(fs.readFileSync(path.join(restoredZData, 'custom-notes.txt'), 'utf8'), 'preserve-me-10-cycles');
      assert.equal(readJson(path.join(restoredZData, 'launch-state.json')).launch_count, 10);

      return { backup_files: manifest.file_count, restored_id: restored.id };
    });

    // 8. Security, Junction Unlinking, Primary Protection, Sanitized Logs & Modern Web Guidance UI Server Smoke
    await record('Test-8', 'Security Guards, Sanitized Logs, Modern Web Guidance & UI Server', async () => {
      // Path traversal rejections
      const travBackup = runHost(dataDir, ['backup', cycleProfile.id, '--dest', '../escape'], { expectOk: false });
      assert.equal(travBackup.json.error_code, 'PATH_TRAVERSAL');
      const travRestore = runHost(dataDir, ['restore', '../escape'], { expectOk: false });
      assert.equal(travRestore.json.error_code, 'PATH_TRAVERSAL');
      const travData = runHost('../escape', ['status'], { expectOk: false });
      assert.equal(travData.json.error_code, 'PATH_TRAVERSAL');

      // Delete without --confirm rejected
      const delNoConf = runHost(dataDir, ['delete', p3.id], { expectOk: false });
      assert.equal(delNoConf.json.error_code, 'CONFIRM_REQUIRED');

      // NTFS junction unlinking safety
      const outsideDir = path.join(tempRoot, 'outside-target');
      fs.mkdirSync(outsideDir, { recursive: true });
      const outsideSecret = path.join(outsideDir, 'keep-safe.txt');
      fs.writeFileSync(outsideSecret, 'DO_NOT_DELETE', 'utf8');
      const junctionPath = path.join(dataDir, 'Profiles', p3.directory_name, 'Desktop', 'LinkedOutside');
      execFileSync('cmd.exe', ['/c', 'mklink', '/J', junctionPath, outsideDir], { windowsHide: true });
      const delRes = runHost(dataDir, ['delete', p3.id, '--confirm']).json;
      assert.ok(delRes.data.unlinked_junctions >= 1, 'Expected junction to be unlinked before deletion');
      assert.equal(fs.readFileSync(outsideSecret, 'utf8'), 'DO_NOT_DELETE', 'Junction target must remain intact!');

      // Primary Zalo protection check
      const baseStatus = runHost(dataDir, ['status']).json;
      let primaryPidProtected = 'skipped_not_running';
      if (baseStatus.primary_zalo.is_running && baseStatus.primary_zalo.locking_pids.length > 0) {
        const primPid = baseStatus.primary_zalo.locking_pids[0];
        const probeProf = runHost(dataDir, ['create', 'Primary Guard Probe']).json.data;
        const probeZData = path.join(dataDir, 'Profiles', probeProf.directory_name, 'AppData', 'Roaming', 'ZaloData');
        fs.writeFileSync(path.join(probeZData, 'lockfile'), JSON.stringify({ pid: primPid }), 'utf8');
        const guardRes = runHost(dataDir, ['stop', probeProf.id], { expectOk: false });
        assert.equal(guardRes.json.error_code, 'PROTECTED_PRIMARY_PROCESS');
        fs.unlinkSync(path.join(probeZData, 'lockfile'));
        runHost(dataDir, ['delete', probeProf.id, '--confirm']);
        primaryPidProtected = `verified_pid_${primPid}`;
      }
      const primPathGuard = runHost(path.join(process.env.APPDATA, 'ZaloData'), ['status'], { expectOk: false });
      assert.equal(primPathGuard.json.error_code, 'PROTECTED_PRIMARY_PATH');

      // Argument-injection safety (`--` sentinel + invalid name characters)
      const argInjCli = runHost(dataDir, ['--', 'create', '--node-fixture']);
      assert.equal(argInjCli.json.data.name, '--node-fixture');
      runHost(dataDir, ['--confirm', '--', 'delete', argInjCli.json.data.id]);

      for (const badName of ['../evil', 'bad/name', 'bad\\name', 'bad\x01name']) {
        const badRes = runHost(dataDir, ['--', 'create', badName], { expectOk: false });
        assert.equal(badRes.json.error_code, 'INVALID_NAME', `Expected INVALID_NAME for ${JSON.stringify(badName)}`);
      }

      // WinVerifyTrust cryptographic Authenticode validation (tampered PE must NOT report Signed)
      const tamperedExe = path.join(tempRoot, 'tampered-node.exe');
      fs.copyFileSync(process.execPath, tamperedExe);
      const buf = fs.readFileSync(tamperedExe);
      buf[buf.length - 10] ^= 0xff;
      fs.writeFileSync(tamperedExe, buf);
      const tamperedDisc = spawnSync('powershell.exe', [
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', HOST_SCRIPT,
        '--cli', '--json', '--data-dir', dataDir, '--app-path', tamperedExe, '--', 'discover'
      ], { encoding: 'utf8', windowsHide: true });
      const tamperedJson = JSON.parse(tamperedDisc.stdout.trim());
      assert.match(tamperedJson.authenticode_status, /^Unverified/, 'Tampered executable must fail WinVerifyTrust');

      // Sanitized diagnostic logs
      const diag = runHost(dataDir, ['diagnostics']).json;
      assert.equal(diag.is_elevated, false);
      const logsText = (diag.sanitized_logs || []).join('\n');
      const rawUser = os.userInfo().username;
      assert.equal(logsText.toLowerCase().includes(rawUser.toLowerCase()), false, 'Sanitized logs leaked raw OS username');
      assert.equal(logsText.toLowerCase().includes((process.env.USERPROFILE || '').toLowerCase()), false, 'Sanitized logs leaked raw USERPROFILE');

      // Modern Web Guidance static compliance checks on apps/desktop/ui/
      const html = fs.readFileSync(path.join(UI_DIR, 'index.html'), 'utf8');
      const css = fs.readFileSync(path.join(UI_DIR, 'styles.css'), 'utf8');
      const js = fs.readFileSync(path.join(UI_DIR, 'app.js'), 'utf8');

      // dialog
      assert.match(html, /<dialog\s+id="delete-profile-dialog"\s+aria-labelledby="delete-profile-title">/);
      assert.match(html, /command="show-modal"\s+commandfor="create-profile-dialog"/);
      assert.match(html, /command="close"\s+commandfor="safety-dialog"/);
      assert.match(html, /<button\s+type="submit"\s+value="cancel"\s+class="[^"]*"\s+autofocus>Cancel<\/button>/);
      assert.equal(/role="dialog"|aria-modal/i.test(html), false, 'Native <dialog> must not have role="dialog" or aria-modal');
      assert.match(js, /\.returnValue\s*=\s*['"]['"]/);
      assert.match(js, /'commandForElement'\s+in\s+HTMLButtonElement\.prototype/);
      assert.match(js, /'closedBy'\s+in\s+HTMLDialogElement\.prototype/);
      assert.match(css, /dialog:modal\s*\{[^}]*overscroll-behavior:\s*contain/s);
      assert.match(css, /dialog::backdrop\s*\{[^}]*overflow:\s*hidden;[^}]*overscroll-behavior:\s*contain/s);
      assert.equal(/(?:html|body)\s*\{[^}]*overflow:\s*hidden/s.test(css), false, 'Must never set overflow: hidden on html/body');

      // responsive-table
      assert.match(html, /<div\s+tabindex="0"\s+role="region"\s+aria-labelledby="profiles-table-caption"\s+class="table-wrapper">/);
      assert.match(html, /<div\s+tabindex="0"\s+role="region"\s+aria-labelledby="diagnostics-table-caption"\s+class="table-wrapper">/);
      assert.match(css, /container-type:\s*inline-size/);
      assert.match(css, /@container\s*\(width\s*<\s*600px\)/);
      assert.match(css, /content:\s*var\(--label-2\)\s*": "/);

      // dark-mode
      assert.match(html, /<meta\s+name="color-scheme"\s+content="light dark">/);
      assert.match(css, /:root\s*\{[^}]*color-scheme:\s*light dark/s);
      assert.match(css, /@media\s*\(prefers-color-scheme:\s*dark\)/);
      assert.match(css, /@supports\s*\(color:\s*light-dark\(white,\s*black\)\)/);
      assert.match(css, /@supports\s+not\s*\(scrollbar-color:\s*auto\)\s*\{[^}]*scrollbar-width:\s*auto/s);
      assert.equal(/transition[^;]*scrollbar-color/i.test(css), false, 'Must never transition scrollbar-color');

      // tabs
      assert.match(html, /<div\s+focusgroup="tablist nomemory"/);
      assert.match(html, /role="tab"[^>]*aria-selected="true"[^>]*focusgroupstart/);
      assert.match(html, /role="tabpanel"[^>]*hidden="until-found"/);
      assert.match(js, /addEventListener\('beforematch'/);
      assert.match(js, /scrollIntoView\(\{\s*block:\s*'nearest',\s*inline:\s*'nearest',\s*container:\s*'nearest'\s*\}\)/);
      assert.match(css, /border-block-end:\s*3px solid var\(--accent\)/);
      assert.match(css, /\.panel-body\s*\{/);

      // Live HTTP smoke + CSRF / Host / Argument-Injection checks against ui-server.mjs
      uiServerHandle = await startUiServer({
        port: 0,
        dataDir,
        nodeFixture: true,
        appPath: FIXTURE_SCRIPT,
      });
      const baseUrl = uiServerHandle.url.replace(/\/$/, '');
      const indexResp = await fetch(`${baseUrl}/`);
      assert.equal(indexResp.status, 200);
      assert.match(await indexResp.text(), /Multi-Zalo Desktop/);

      const healthResp = await (await fetch(`${baseUrl}/api/health`)).json();
      assert.equal(healthResp.ok, true);

      const statusResp = await (await fetch(`${baseUrl}/api/status`)).json();
      assert.equal(statusResp.ok, true);

      const apiCreate = await (await fetch(`${baseUrl}/api/profiles`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: '--node-fixture', notes: 'arg-inj-smoke' }),
      })).json();
      assert.equal(apiCreate.ok, true);
      assert.equal(apiCreate.data.name, '--node-fixture');

      const apiDelete = await (await fetch(`${baseUrl}/api/profiles/${apiCreate.data.id}`, {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ confirm: true }),
      })).json();
      assert.equal(apiDelete.ok, true);

      // CSRF: reject cross-origin Origin header
      const csrfOriginResp = await fetch(`${baseUrl}/api/profiles`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Origin': 'https://evil.example',
        },
        body: JSON.stringify({ name: 'Should Be Blocked' }),
      });
      assert.equal(csrfOriginResp.status, 403);
      assert.equal((await csrfOriginResp.json()).error_code, 'FORBIDDEN_ORIGIN');

      // CSRF: reject non-JSON Content-Type (preflightless text/plain attack)
      const csrfCtResp = await fetch(`${baseUrl}/api/profiles`, {
        method: 'POST',
        headers: { 'Content-Type': 'text/plain' },
        body: '{"name":"Should Be Blocked"}',
      });
      assert.equal(csrfCtResp.status, 415);
      assert.equal((await csrfCtResp.json()).error_code, 'UNSUPPORTED_MEDIA_TYPE');

      // Host validation: reject non-loopback Host header
      const badHostHttp = await new Promise((resolve, reject) => {
        http.request({
          host: '127.0.0.1',
          port: uiServerHandle.port,
          path: '/api/health',
          method: 'GET',
          headers: { Host: 'evil.example:1234' },
        }, (res) => {
          let body = '';
          res.on('data', (c) => { body += c; });
          res.on('end', () => resolve({ status: res.statusCode, json: JSON.parse(body) }));
        }).on('error', reject).end();
      });
      assert.equal(badHostHttp.status, 403);
      assert.equal(badHostHttp.json.error_code, 'FORBIDDEN_HOST');

      const travHttp = await new Promise((resolve, reject) => {
        http.get({ host: '127.0.0.1', port: uiServerHandle.port, path: '/../../etc/passwd' }, (res) => {
          let body = '';
          res.on('data', (c) => { body += c; });
          res.on('end', () => resolve({ status: res.statusCode, json: JSON.parse(body) }));
        }).on('error', reject);
      });
      assert.equal(travHttp.status, 403);
      assert.equal(travHttp.json.error_code, 'PATH_TRAVERSAL_REJECTED');

      return {
        primary_pid_protected: primaryPidProtected,
        win_verify_trust_tamper_rejected: tamperedJson.authenticode_status,
        ui_server_port: uiServerHandle.port,
        modern_web_guidance_verified: ['dialog', 'responsive-table', 'dark-mode', 'tabs'],
      };
    });

    const summary = {
      ok: true,
      executed_at_utc: new Date().toISOString(),
      node_version: process.version,
      platform: process.platform,
      total_tests: results.length,
      passed_tests: results.filter((r) => r.status === 'PASS').length,
      failed_tests: results.filter((r) => r.status === 'FAIL').length,
      results,
    };

    const outPath = parseOutPath(process.argv.slice(2));
    const jsonOut = JSON.stringify(summary, null, 2);
    if (outPath) {
      fs.mkdirSync(path.dirname(outPath), { recursive: true });
      fs.writeFileSync(outPath, `${jsonOut}\n`, 'utf8');
    }
    console.log(jsonOut);
  } finally {
    if (uiServerHandle?.server) {
      await new Promise((resolve) => uiServerHandle.server.close(resolve));
    }
    // Ensure any lingering fixture processes inside temp dataDir are stopped before removing tempRoot
    if (fs.existsSync(path.join(dataDir, 'profiles.json'))) {
      try {
        const st = runHost(dataDir, ['status']).json;
        for (const prof of st.profiles || []) {
          if (prof.running) {
            runHost(dataDir, ['stop', prof.id], { expectOk: false });
          }
        }
      } catch {
        // Best-effort cleanup
      }
    }
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

runSuite().catch((err) => {
  console.error(JSON.stringify({ ok: false, error: err.message, stack: err.stack }, null, 2));
  process.exit(1);
});
