#!/usr/bin/env node
/**
 * End-to-end macOS multi-instance & safety hardening fixture test runner (`run-macos-fixture-tests.mjs`).
 *
 * Drives the compiled `dim_macos_cli` binary (`crates/platform-macos/examples/dim_macos_cli.rs`)
 * and `fixture-macos-app.mjs` across 16 end-to-end scenarios (the 13 baseline scenarios +
 * 3 Stage-2 runtime hardening scenarios: 12-surface live isolation audit, slow pre-lock startup &
 * deterministic user-id derivation, and graceful delayed/hung/orphan shutdown) in an isolated
 * temporary directory (leaving the git worktree 100% clean).
 */

import assert from 'node:assert/strict';
import { execFileSync, spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '..', '..');
const FIXTURE_SCRIPT = path.resolve(__dirname, 'fixture-macos-app.mjs');

function parseCliArgs(argv) {
  let outPath = null;
  let cliBin = null;
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--out' && argv[i + 1]) {
      outPath = path.resolve(argv[++i]);
    } else if (argv[i] === '--cli-bin' && argv[i + 1]) {
      cliBin = path.resolve(argv[++i]);
    }
  }
  return { outPath, cliBin };
}

function buildOrLocateCliBin(explicitBin) {
  if (explicitBin && fs.existsSync(explicitBin)) {
    return explicitBin;
  }
  const ext = process.platform === 'win32' ? '.exe' : '';
  const debugBin = path.join(REPO_ROOT, 'target', 'debug', 'examples', `dim_macos_cli${ext}`);
  if (fs.existsSync(debugBin)) {
    return debugBin;
  }

  const res = spawnSync(
    'cargo',
    ['build', '-p', 'dim-platform-macos', '--example', 'dim_macos_cli', '--locked'],
    {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      windowsHide: true,
    },
  );
  if (res.status !== 0) {
    throw new Error(`Failed to build dim_macos_cli:\n${res.stdout}\n${res.stderr}`);
  }
  assert.ok(fs.existsSync(debugBin), `Expected compiled binary at ${debugBin}`);
  return debugBin;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function isPidAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return Boolean(err && err.code === 'EPERM');
  }
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function runCli(cliBin, ctx, cmdArgs, { expectOk = true, extraFlags = [] } = {}) {
  const fullArgs = [
    '--json',
    '--fixture-host',
    '--data-dir',
    ctx.dataDir,
    '--simulated-home',
    ctx.simHome,
    '--short-tmp-root',
    ctx.shortTmp,
    '--node-fixture',
    '--app-path',
    FIXTURE_SCRIPT,
    ...extraFlags,
    ...cmdArgs,
  ];

  const res = spawnSync(cliBin, fullArgs, {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    windowsHide: true,
  });
  const stdout = (res.stdout || '').trim();
  const stderr = (res.stderr || '').trim();
  let json = null;
  try {
    json = stdout ? JSON.parse(stdout) : null;
  } catch (err) {
    throw new Error(
      `Invalid JSON from dim_macos_cli (${cmdArgs.join(' ')}): ${err.message}\nSTDOUT:\n${stdout}\nSTDERR:\n${stderr}`,
    );
  }
  if (expectOk) {
    assert.equal(
      res.status,
      0,
      `Expected exit 0 for [${cmdArgs.join(' ')}], got ${res.status}: ${stdout} ${stderr}`,
    );
    assert.equal(json?.ok, true, `Expected ok=true for [${cmdArgs.join(' ')}]: ${stdout}`);
  }
  return { status: res.status, json, stdout, stderr };
}

function runCliAsync(cliBin, ctx, cmdArgs, extraFlags = []) {
  return new Promise((resolve, reject) => {
    const fullArgs = [
      '--json',
      '--fixture-host',
      '--data-dir',
      ctx.dataDir,
      '--simulated-home',
      ctx.simHome,
      '--short-tmp-root',
      ctx.shortTmp,
      '--node-fixture',
      '--app-path',
      FIXTURE_SCRIPT,
      ...extraFlags,
      ...cmdArgs,
    ];
    const child = spawn(cliBin, fullArgs, {
      cwd: REPO_ROOT,
      windowsHide: true,
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => {
      stdout += d.toString('utf8');
    });
    child.stderr.on('data', (d) => {
      stderr += d.toString('utf8');
    });
    child.on('error', reject);
    child.on('close', (code) => {
      let json = null;
      try {
        json = stdout.trim() ? JSON.parse(stdout.trim()) : null;
      } catch {
        // Leave null on parse error.
      }
      resolve({ status: code, json, stdout, stderr });
    });
  });
}

async function main() {
  const { outPath, cliBin: explicitBin } = parseCliArgs(process.argv.slice(2));
  const cliBin = buildOrLocateCliBin(explicitBin);
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'dim-macos-e2e-'));

  const ctx = {
    dataDir: path.join(tempRoot, 'Library', 'Application Support', 'MultiZaloDesktop'),
    simHome: path.join(tempRoot, 'Users', 'macuser'),
    shortTmp: path.join(tempRoot, 'stmp'),
  };
  fs.mkdirSync(ctx.simHome, { recursive: true });
  fs.mkdirSync(ctx.shortTmp, { recursive: true });

  const scenarios = [];
  const recordScenario = async (number, key, title, fn) => {
    const start = Date.now();
    try {
      const evidence = await fn();
      scenarios.push({
        scenario: number,
        key,
        title,
        status: 'PASS',
        duration_ms: Date.now() - start,
        evidence: evidence || {},
      });
    } catch (err) {
      scenarios.push({
        scenario: number,
        key,
        title,
        status: 'FAIL',
        duration_ms: Date.now() - start,
        error: err.stack || err.message,
      });
      throw err;
    }
  };

  let p1;
  let p2;
  let p3;

  try {
    // Scenario 1: Single isolated session launch, window state, and focus
    await recordScenario(1, 'single_session', '1 Isolated macOS Session Launch, Window State & Focus', async () => {
      const caps = runCli(cliBin, ctx, ['capabilities']).json;
      assert.equal(caps.platform, 'macos');
      assert.equal(caps.capabilities.official_zalo_status, 'not_tested');
      assert.equal(caps.capabilities.concurrent_calls_status, 'blocked');

      p1 = runCli(cliBin, ctx, ['create', 'Đức - Công việc', '--notes', 'Mac Session 1']).json.data;
      assert.match(p1.id, /^[0-9a-f]{8}$/);
      assert.match(p1.directory_name, /^duc-cong-viec_[0-9a-f]{8}$/);

      const launched = runCli(cliBin, ctx, ['launch', p1.id], {
        extraFlags: ['--spawn-helper', '--account-label', 'Work Account'],
      }).json.data.runtime;

      assert.equal(launched.running, true);
      assert.ok(launched.main_pid > 0);
      assert.ok(launched.window_handle > 0);
      assert.equal(launched.window_title, 'Zalo — Work Account');

      runCli(cliBin, ctx, ['focus', p1.id]);
      await sleep(160);
      const launchState = readJson(path.join(launched.zalo_data_dir, 'launch-state.json'));
      assert.ok(launchState.focus_count >= 1, 'Expected focus_count >= 1 after focus');
      return {
        profile_id: p1.id,
        main_pid: launched.main_pid,
        window_handle: launched.window_handle,
        focus_count: launchState.focus_count,
      };
    });

    // Scenario 2: 2 concurrent isolated sessions
    await recordScenario(2, 'two_sessions', '2 Concurrent Isolated macOS Sessions', async () => {
      p2 = runCli(cliBin, ctx, ['create', 'Tài khoản Cá nhân']).json.data;
      const l2 = runCli(cliBin, ctx, ['launch', p2.id], {
        extraFlags: ['--spawn-helper', '--account-label', 'Personal Account'],
      }).json.data.runtime;

      const st = runCli(cliBin, ctx, ['status']).json;
      const active = st.profiles.filter((x) => x.runtime.running);
      assert.equal(active.length, 2);
      const pids = new Set(active.map((x) => x.runtime.main_pid));
      assert.equal(pids.size, 2);
      return { active_pids: [...pids], p2_window: l2.window_title };
    });

    // Scenario 3: 3 concurrent isolated sessions
    await recordScenario(3, 'three_sessions', '3 Concurrent Isolated macOS Sessions', async () => {
      p3 = runCli(cliBin, ctx, ['create', 'Hỗ trợ Khách hàng']).json.data;
      runCli(cliBin, ctx, ['launch', p3.id], {
        extraFlags: ['--spawn-helper', '--account-label', 'Support Account'],
      });

      const st = runCli(cliBin, ctx, ['status', '--with-size']).json;
      const active = st.profiles.filter((x) => x.runtime.running);
      assert.equal(active.length, 3);
      assert.equal(new Set(active.map((x) => x.runtime.main_pid)).size, 3);
      assert.equal(new Set(active.map((x) => x.runtime.detected_zalo_user_id)).size, 3);
      assert.ok(active.every((x) => x.runtime.disk_usage_bytes > 0));
      assert.ok(active.every((x) => x.runtime.working_set_bytes > 0));
      return {
        active_count: active.length,
        pids: active.map((x) => x.runtime.main_pid),
        user_ids: active.map((x) => x.runtime.detected_zalo_user_id),
      };
    });

    // Scenario 4 + 14: 12-Surface Live Isolation Audit & Duplicate Account Guard (`P0-2`)
    await recordScenario(4, 'isolated_data_12_surfaces', '12-Surface Runtime Isolation Audit & Duplicate Account Guard', async () => {
      const isoReport = runCli(cliBin, ctx, ['verify-isolation', p1.id, p2.id, p3.id]).json.data;
      assert.equal(isoReport.verified_surfaces_count, 12);
      assert.equal(isoReport.distinct_tmpdirs, 3);
      assert.equal(isoReport.distinct_caches, 3);
      assert.equal(isoReport.primary_zalo_data_untouched, true);

      for (const prof of [p1, p2, p3]) {
        const pdir = path.join(ctx.dataDir, 'Profiles', prof.directory_name);
        const prefXml = fs.readFileSync(
          path.join(pdir, 'Library', 'Preferences', 'com.vng.zalo.plist'),
          'utf8',
        );
        assert.match(prefXml, new RegExp(`<string>${prof.id}</string>`));
        assert.equal(
          fs.lstatSync(path.join(pdir, 'Downloads')).isSymbolicLink(),
          false,
          'Downloads must be an isolated directory by default (zero cross-profile symlink sharing)',
        );
      }

      const st = runCli(cliBin, ctx, ['status']).json;
      const p1Uid = st.profiles.find((x) => x.record.id === p1.id).runtime.detected_zalo_user_id;
      const dupProf = runCli(cliBin, ctx, ['create', 'Duplicate UID Probe']).json.data;
      const dupDbDir = path.join(
        ctx.dataDir,
        'Profiles',
        dupProf.directory_name,
        'Library',
        'Application Support',
        'ZaloData',
        'Database',
        '_production',
        p1Uid,
      );
      fs.mkdirSync(dupDbDir, { recursive: true });
      const dupLaunch = runCli(cliBin, ctx, ['launch', dupProf.id], { expectOk: false });
      assert.equal(dupLaunch.json.error_code, 'DUPLICATE_ZALO_USER_ID');
      runCli(cliBin, ctx, ['delete', dupProf.id, '--confirm']);

      return {
        verified_surfaces_count: isoReport.verified_surfaces_count,
        distinct_tmpdirs: isoReport.distinct_tmpdirs,
        duplicate_account_blocked: dupLaunch.json.error_code,
      };
    });

    // Scenario 6: Individual close (stop P2 while P1 and P3 stay running!)
    await recordScenario(6, 'individual_close', 'Individual Session Close Preserving Sibling Sessions', async () => {
      const before = runCli(cliBin, ctx, ['status']).json;
      const p1PidBefore = before.profiles.find((x) => x.record.id === p1.id).runtime.main_pid;
      const p3PidBefore = before.profiles.find((x) => x.record.id === p3.id).runtime.main_pid;

      const stopRes = runCli(cliBin, ctx, ['stop', p2.id]).json;
      assert.ok(stopRes.data.terminated_process_count >= 1);

      const after = runCli(cliBin, ctx, ['status']).json;
      const s1 = after.profiles.find((x) => x.record.id === p1.id).runtime;
      const s2 = after.profiles.find((x) => x.record.id === p2.id).runtime;
      const s3 = after.profiles.find((x) => x.record.id === p3.id).runtime;

      assert.equal(s1.running, true);
      assert.equal(s1.main_pid, p1PidBefore);
      assert.equal(s2.running, false);
      assert.equal(s2.stale_lock_detected, false);
      assert.equal(s3.running, true);
      assert.equal(s3.main_pid, p3PidBefore);
      return { stopped_profile: p2.id, preserved_pids: [s1.main_pid, s3.main_pid] };
    });

    // Scenario 5: Relaunch & Idempotent Launch + Offline Backup/Restore
    await recordScenario(5, 'relaunch', 'Relaunch, Idempotent Running Launch & Offline Backup/Restore', async () => {
      const stBefore = runCli(cliBin, ctx, ['status']).json;
      const p1Pid = stBefore.profiles.find((x) => x.record.id === p1.id).runtime.main_pid;
      const idem = runCli(cliBin, ctx, ['launch', p1.id]).json.data.runtime;
      assert.equal(idem.main_pid, p1Pid);

      const relaunched = runCli(cliBin, ctx, ['launch', p2.id]).json.data.runtime;
      assert.equal(relaunched.running, true);
      const p2State = readJson(path.join(relaunched.zalo_data_dir, 'launch-state.json'));
      assert.equal(p2State.launch_count, 2);

      runCli(cliBin, ctx, ['stop', p1.id]);
      runCli(cliBin, ctx, ['stop', p2.id]);
      runCli(cliBin, ctx, ['stop', p3.id]);

      const bk = runCli(cliBin, ctx, ['backup', p2.id]).json.data;
      const restored = runCli(cliBin, ctx, [
        'restore',
        bk.backup_dir,
        '--name',
        'Restored Mac Profile',
      ]).json.data;
      assert.notEqual(restored.id, p2.id);
      runCli(cliBin, ctx, ['delete', restored.id, '--confirm']);

      return { idempotent_pid: p1Pid, p2_launch_count: p2State.launch_count, restored_id: restored.id };
    });

    // Scenario 7: Duplicate concurrent operation guard across OS processes
    await recordScenario(7, 'duplicate_operation_guard', 'Cross-Process Duplicate Concurrent Launch Guard (.op-lock)', async () => {
      const holderPromise = runCliAsync(cliBin, ctx, ['launch', p1.id], ['--hold-ms', '500']);
      await sleep(120);
      const racing = await runCliAsync(cliBin, ctx, ['launch', p1.id]);
      const holder = await holderPromise;

      assert.equal(holder.status, 0, `Holder launch failed: ${holder.stdout} ${holder.stderr}`);
      assert.notEqual(racing.status, 0, 'Concurrent duplicate launch must be rejected');
      assert.equal(racing.json?.error_code, 'PROCESS_ERROR');
      runCli(cliBin, ctx, ['stop', p1.id]);
      return { blocked_error_code: racing.json.error_code, message: racing.json.message };
    });

    // Scenario 8: Crash recovery (stale SingletonLock, SingletonSocket, SingletonCookie, lockfile, short TMPDIR)
    await recordScenario(8, 'crash_recovery', 'Crash Recovery Cleaning Stale SingletonLock & Short TMPDIR Remnants', async () => {
      const crashProf = runCli(cliBin, ctx, ['create', 'Crash Recovery Mac']).json.data;
      runCli(cliBin, ctx, ['launch', crashProf.id], {
        extraFlags: ['--crash', '--crash-delay-ms', '80'],
      });
      await sleep(320);

      const crashedSt = runCli(cliBin, ctx, ['status']).json.profiles.find(
        (x) => x.record.id === crashProf.id,
      ).runtime;
      assert.equal(crashedSt.running, false);
      assert.equal(crashedSt.stale_lock_detected, true);

      const recovered = runCli(cliBin, ctx, ['launch', crashProf.id]).json.data;
      assert.equal(recovered.crash_recovered, true);
      assert.equal(recovered.runtime.running, true);

      runCli(cliBin, ctx, ['stop', crashProf.id]);
      runCli(cliBin, ctx, ['delete', crashProf.id, '--confirm']);
      return { stale_detected: true, crash_recovered: recovered.crash_recovered };
    });

    // Scenario 9: Multi-evidence Process Ownership (Foreign & Same-Binary Zalo PID Reuse) + Primary Protection (`P0-3`)
    await recordScenario(9, 'ownership_and_primary_protection', 'Multi-Evidence PID Reuse Defense & Primary Zalo Protection', async () => {
      const guardProbe = runCli(cliBin, ctx, ['verify-foreign-or-primary-guard', p1.id]).json.data;
      assert.equal(guardProbe.foreign_not_owned, true, 'Foreign Safari PID must NOT be owned');
      assert.equal(guardProbe.foreign_stop_count, 0);
      assert.equal(guardProbe.safari_still_alive, true, 'Foreign PID must remain untouched');
      assert.equal(
        guardProbe.same_binary_pid_reuse_rejected,
        true,
        'Same-UID same-binary Zalo PID reuse with mismatched lstart must be rejected',
      );
      assert.equal(guardProbe.unowned_focus_error_code, 'PROCESS_ERROR');
      assert.equal(guardProbe.primary_detected, true, 'Primary Zalo.app PID must be detected');
      assert.equal(guardProbe.stop_primary_error_code, 'PROTECTED_PRIMARY_PROCESS');

      const primaryZaloDir = path.join(ctx.simHome, 'Library', 'Application Support', 'ZaloData');
      const pathBlocked = runCli(
        cliBin,
        { ...ctx, dataDir: primaryZaloDir },
        ['status'],
        { expectOk: false },
      );
      assert.equal(pathBlocked.json.error_code, 'PROTECTED_PRIMARY_PATH');
      return {
        ...guardProbe,
        primary_path_blocked: pathBlocked.json.error_code,
      };
    });

    // Scenario 10: Safe symlink / junction deletion preserving host target files
    await recordScenario(10, 'safe_symlink_deletion', 'Safe User-Folder Symlink Unlinking Before Profile Deletion', async () => {
      const outsideHostDownloads = path.join(tempRoot, 'RealUserDownloads');
      fs.mkdirSync(outsideHostDownloads, { recursive: true });
      const keepFile = path.join(outsideHostDownloads, 'important-contract.pdf');
      fs.writeFileSync(keepFile, 'MACOS_HOST_FILE_MUST_SURVIVE', 'utf8');

      const p3Dir = path.join(ctx.dataDir, 'Profiles', p3.directory_name);
      const downloadsLinkParent = path.join(p3Dir, 'Downloads');
      fs.mkdirSync(downloadsLinkParent, { recursive: true });
      const linkedSub = path.join(downloadsLinkParent, 'HostShared');
      if (process.platform === 'win32') {
        execFileSync('cmd.exe', ['/c', 'mklink', '/J', linkedSub, outsideHostDownloads], {
          windowsHide: true,
        });
      } else {
        fs.symlinkSync(outsideHostDownloads, linkedSub);
      }

      const noConfirm = runCli(cliBin, ctx, ['delete', p3.id], { expectOk: false });
      assert.equal(noConfirm.json.error_code, 'CONFIRM_REQUIRED');

      const delOk = runCli(cliBin, ctx, ['delete', p3.id, '--confirm']).json.data;
      assert.ok(delOk.unlinked_symlinks >= 1);
      assert.equal(fs.existsSync(p3Dir), false);
      assert.equal(fs.readFileSync(keepFile, 'utf8'), 'MACOS_HOST_FILE_MUST_SURVIVE');
      return { unlinked_symlinks: delOk.unlinked_symlinks, host_file_preserved: true };
    });

    // Scenario 11: Unicode filenames (NFC & NFD Vietnamese, Japanese CJK, Korean Hangul)
    await recordScenario(11, 'unicode_nfc_nfd', 'NFC/NFD Vietnamese, Japanese CJK & Korean Hangul Paths', async () => {
      const nfdParent = path.join(
        tempRoot,
        'Th\u01b0 mu\u0323c \u0110u\u031b\u0301c NFD',
        '東京_서울_Data',
      );
      const unicodeCtx = {
        dataDir: nfdParent,
        simHome: path.join(tempRoot, 'Users', 'Đức Nguyễn'),
        shortTmp: ctx.shortTmp,
      };
      fs.mkdirSync(unicodeCtx.simHome, { recursive: true });

      const nfdPrimaryZalo = path.join(
        tempRoot,
        'Users',
        'Đu\u031b\u0301c Nguye\u0302\u0303n',
        'Library',
        'Application Support',
        'ZaloData',
      );
      const nfdBlocked = runCli(
        cliBin,
        { ...unicodeCtx, dataDir: nfdPrimaryZalo },
        ['status'],
        { expectOk: false },
      );
      assert.equal(nfdBlocked.json.error_code, 'PROTECTED_PRIMARY_PATH');

      const cjkProf = runCli(cliBin, unicodeCtx, ['create', '東京 営業部 — Nguyễn Ánh']).json.data;
      const launched = runCli(cliBin, unicodeCtx, ['launch', cjkProf.id]).json.data.runtime;
      assert.equal(launched.running, true);
      runCli(cliBin, unicodeCtx, ['stop', cjkProf.id]);
      runCli(cliBin, unicodeCtx, ['delete', cjkProf.id, '--confirm']);
      return {
        nfd_primary_guard: nfdBlocked.json.error_code,
        cjk_vietnamese_slug: cjkProf.directory_name,
      };
    });

    // Scenario 12: Long paths (`SUN_LEN` 104B `/tmp/dz-<uid>/<id>` short TMPDIR & `PATH_MAX` 1024B guard)
    await recordScenario(12, 'long_paths_sun_len_path_max', 'Darwin SUN_LEN (104B) /tmp/dz-<uid>/<id> Hardening & PATH_MAX (1024B) Guard', async () => {
      const longProf = runCli(cliBin, ctx, [
        'create',
        'Doanh Nghiệp Chi Nhánh Thành Phố Hồ Chí Minh Số 01',
      ]).json.data;
      const launched = runCli(cliBin, ctx, ['launch', longProf.id], {
        extraFlags: ['--enforce-sun-len'],
      }).json.data.runtime;
      assert.equal(launched.running, true);

      const pdir = path.join(ctx.dataDir, 'Profiles', longProf.directory_name);
      const meta = readJson(path.join(pdir, '.macos-profile-meta.json'));
      assert.equal(meta.short_tmpdir_fallback, true, 'Long profile path must trigger short TMPDIR fallback');
      assert.match(meta.effective_tmpdir.replace(/\\/g, '/'), /\/dz-\d+\/[0-9a-f]{8}$/);

      const cacheMeta = readJson(
        path.join(pdir, 'Library', 'Caches', 'com.vng.zalo', 'cache-meta.json'),
      );
      assert.ok(
        cacheMeta.socket_path_bytes <= 103,
        `Expected socket_path_bytes <= 103, got ${cacheMeta.socket_path_bytes}`,
      );

      runCli(cliBin, ctx, ['stop', longProf.id]);
      assert.equal(
        fs.existsSync(meta.effective_tmpdir),
        false,
        'Short /tmp/dz-<uid>/<id> slot must be cleaned up upon stop()',
      );
      runCli(cliBin, ctx, ['delete', longProf.id, '--confirm']);

      const overPathMax = path.join(tempRoot, 'x'.repeat(1030));
      const maxBlocked = runCli(cliBin, ctx, ['discover'], {
        expectOk: false,
        extraFlags: ['--app-path', overPathMax],
      });
      assert.equal(maxBlocked.json.error_code, 'PATH_TRAVERSAL');

      return {
        short_tmpdir_fallback: meta.short_tmpdir_fallback,
        socket_path_bytes: cacheMeta.socket_path_bytes,
        short_tmpdir_cleaned_on_stop: true,
        path_max_rejected: maxBlocked.json.error_code,
      };
    });

    // Scenario 13: Permission failures & world-writable Profiles directory detection
    await recordScenario(13, 'permission_failures', 'POSIX 0700 Enforcement, 0777 Detection & Unwritable Directory Failure', async () => {
      const diagBefore = runCli(cliBin, ctx, ['diagnostics']).json;
      assert.equal(diagBefore.profiles_acl_safe, true);

      const modeSidecar = path.join(ctx.dataDir, 'Profiles', '.dim-simulated-posix-mode');
      fs.writeFileSync(modeSidecar, '0777\n', 'utf8');
      if (process.platform !== 'win32') {
        fs.chmodSync(path.join(ctx.dataDir, 'Profiles'), 0o777);
      }
      const diagInsecure = runCli(cliBin, ctx, ['diagnostics']).json;
      assert.equal(diagInsecure.profiles_acl_safe, false);

      fs.unlinkSync(modeSidecar);
      if (process.platform !== 'win32') {
        fs.chmodSync(path.join(ctx.dataDir, 'Profiles'), 0o700);
      }

      const roRoot = path.join(tempRoot, 'ReadOnlyDataRoot');
      const roProfiles = path.join(roRoot, 'Profiles');
      fs.mkdirSync(roProfiles, { recursive: true });
      fs.writeFileSync(path.join(roProfiles, '.dim-simulated-posix-mode'), '0500\n', 'utf8');
      if (process.platform !== 'win32') {
        fs.chmodSync(roProfiles, 0o500);
      }
      const permFail = runCli(
        cliBin,
        { ...ctx, dataDir: roRoot },
        ['create', 'Should Fail On ReadOnly'],
        { expectOk: false },
      );
      if (process.platform !== 'win32') {
        fs.chmodSync(roProfiles, 0o700);
      }
      assert.notEqual(permFail.status, 0);
      assert.equal(permFail.json.error_code, 'IO_ERROR');

      return {
        acl_safe_normal: diagBefore.profiles_acl_safe,
        acl_safe_0777: diagInsecure.profiles_acl_safe,
        readonly_error_code: permFail.json.error_code,
      };
    });

    // Scenario 14: Slow Pre-Lock Startup (`DEFECT-03` / `LC-07`) & Auto User-ID (`DEFECT-02` / `XP-MAC-03`)
    await recordScenario(14, 'slow_prelock_and_auto_userid', 'Slow Pre-Lock Startup Ownership (LC-07) & Auto User-ID Derivation (XP-MAC-03)', async () => {
      const slowProf = runCli(cliBin, ctx, ['create', 'Slow PreLock Mac']).json.data;
      const autoProf = runCli(cliBin, ctx, ['create', 'Auto UserId Mac']).json.data;

      const rtSlow = runCli(cliBin, ctx, ['launch', slowProf.id], {
        extraFlags: ['--slow-startup-pre-lock-ms', '2000'],
      }).json.data.runtime;
      assert.equal(
        rtSlow.running,
        true,
        'Launch must return running=true immediately via .runtime-launch.json even before SingletonLock exists',
      );

      const rtAuto = runCli(cliBin, ctx, ['launch', autoProf.id]).json.data.runtime;
      assert.equal(rtAuto.running, true);

      await sleep(800);
      const st = runCli(cliBin, ctx, ['status']).json;
      const uidSlow = st.profiles.find((x) => x.record.id === slowProf.id).runtime.detected_zalo_user_id;
      const uidAuto = st.profiles.find((x) => x.record.id === autoProf.id).runtime.detected_zalo_user_id;
      assert.ok(uidSlow && uidAuto);
      assert.notEqual(uidSlow, uidAuto);

      runCli(cliBin, ctx, ['stop', slowProf.id]);
      runCli(cliBin, ctx, ['stop', autoProf.id]);
      runCli(cliBin, ctx, ['delete', slowProf.id, '--confirm']);
      runCli(cliBin, ctx, ['delete', autoProf.id, '--confirm']);

      return { slow_pid: rtSlow.main_pid, uid_slow: uidSlow, uid_auto: uidAuto };
    });

    // Scenario 15: Graceful Delayed Shutdown, Hung Process Fail-Closed, and Orphan Helper Reaping (`P1-2`)
    await recordScenario(15, 'graceful_delayed_hung_orphan_shutdown', 'Delayed Cooperative Quit, Hung Fail-Closed Error & Orphan Helper Reaping', async () => {
      // 1. Delayed exit within graceful window
      const delayedProf = runCli(cliBin, ctx, ['create', 'Delayed Quit Mac']).json.data;
      const rtDelayed = runCli(cliBin, ctx, ['launch', delayedProf.id], {
        extraFlags: ['--delayed-exit-ms', '160'],
      }).json.data.runtime;
      runCli(cliBin, ctx, ['stop', delayedProf.id], { extraFlags: ['--timeout-ms', '1800'] });
      const lsDelayed = readJson(path.join(rtDelayed.zalo_data_dir, 'launch-state.json'));
      assert.equal(lsDelayed.exit_reason, 'graceful_quit_signal');
      runCli(cliBin, ctx, ['delete', delayedProf.id, '--confirm']);

      // 2. Orphan helper (`ppid == 1`) reaped via SIGTERM escalation
      const orphanProf = runCli(cliBin, ctx, ['create', 'Orphan Helper Mac']).json.data;
      const rtOrphan = runCli(cliBin, ctx, ['launch', orphanProf.id], {
        extraFlags: ['--orphan-helper'],
      }).json.data.runtime;
      const lsOrphan = readJson(path.join(rtOrphan.zalo_data_dir, 'launch-state.json'));
      const orphanPid = lsOrphan.orphan_helper_pid;
      assert.equal(isPidAlive(orphanPid), true);
      const stopOrphan = runCli(cliBin, ctx, ['stop', orphanProf.id], {
        extraFlags: ['--timeout-ms', '2500'],
      }).json.data;
      assert.ok(stopOrphan.terminated_process_count >= 2);
      assert.equal(isPidAlive(orphanPid), false, 'Orphan helper PID must be terminated');
      runCli(cliBin, ctx, ['delete', orphanProf.id, '--confirm']);

      // 3. Hung process (`--hang-on-quit`) fails closed with PROCESS_ERROR
      const hungProf = runCli(cliBin, ctx, ['create', 'Hung Process Mac']).json.data;
      const hungDir = path.join(ctx.dataDir, 'Profiles', hungProf.directory_name);
      runCli(cliBin, ctx, ['launch', hungProf.id], {
        extraFlags: ['--hang-on-quit', '--stay-ms', '1400'],
      });
      const hungStop = runCli(cliBin, ctx, ['stop', hungProf.id], {
        expectOk: false,
        extraFlags: ['--timeout-ms', '350'],
      });
      assert.notEqual(hungStop.status, 0);
      assert.equal(hungStop.json.error_code, 'PROCESS_ERROR');
      assert.equal(
        fs.existsSync(path.join(hungDir, '.runtime-launch.json')),
        true,
        '.runtime-launch.json must remain intact when stop() times out on hung process',
      );
      await sleep(1300);
      runCli(cliBin, ctx, ['recover', hungProf.id]);
      runCli(cliBin, ctx, ['delete', hungProf.id, '--confirm']);

      // Clean up p1 & p2
      runCli(cliBin, ctx, ['delete', p1.id, '--confirm']);
      runCli(cliBin, ctx, ['delete', p2.id, '--confirm']);

      return {
        delayed_exit_reason: lsDelayed.exit_reason,
        orphan_pid_reaped: orphanPid,
        hung_stop_error_code: hungStop.json.error_code,
      };
    });

    const summary = {
      ok: true,
      executed_at_utc: new Date().toISOString(),
      host_platform: process.platform,
      host_arch: process.arch,
      verification_state:
        process.platform === 'darwin' ? 'DARWIN_NATIVE_TESTED' : 'WINDOWS_HOST_FIXTURE_TESTED',
      live_zalo_status: 'LIVE_ZALO_NOT_TESTED',
      total_scenarios: scenarios.length,
      passed_scenarios: scenarios.filter((s) => s.status === 'PASS').length,
      failed_scenarios: scenarios.filter((s) => s.status === 'FAIL').length,
      scenarios,
    };

    const text = JSON.stringify(summary, null, 2);
    if (outPath) {
      fs.mkdirSync(path.dirname(outPath), { recursive: true });
      fs.writeFileSync(outPath, `${text}\n`, 'utf8');
    }
    console.log(text);
  } finally {
    try {
      fs.rmSync(tempRoot, { recursive: true, force: true });
    } catch {
      // Best-effort temp cleanup.
    }
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
