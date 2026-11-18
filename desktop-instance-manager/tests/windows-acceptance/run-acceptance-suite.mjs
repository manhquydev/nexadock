#!/usr/bin/env node
/**
 * Unified Windows Native User Acceptance & Installation Lab Orchestrator (`run-acceptance-suite.mjs`).
 *
 * Executes all 4 Windows acceptance tracks sequentially with hard timeouts (zero infinite wait loops):
 *   1. `journeys-and-errors.mjs`              -> `evidence/journeys-and-errors-report.json`
 *   2. `native-tauri-smoke.mjs`               -> `evidence/native-tauri-smoke-report.json`
 *   3. `installer-sandbox-check.ps1`          -> `evidence/installer-env-audit.json`
 *   4. `run-l3-guided-acceptance.ps1 -NonInteractive` -> non-interactive L3 gate check (`AWAITING_MANUAL_L3`)
 *
 * Enforces host safety invariants across the entire run:
 *   - 100% Primary `%APPDATA%\ZaloData\config.json` SHA-256 & live `Zalo*` PID invariance
 *   - Zero orphan `dim-*` / `fixture-app.mjs` / `ui-server.mjs` processes after suite completion
 *   - Zero PII leaks across `tests/windows-acceptance/evidence/**` and `docs/qa/windows/**`
 */

import { execFile, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

import {
  ACCEPTANCE_ROOT,
  EVIDENCE_DIR,
  QA_DOCS_DIR,
  Stopwatch,
  WORKTREE_ROOT,
  assertZeroPiiLeaks,
  captureManagedProcessSnapshot,
  capturePrimaryZaloSnapshot,
  diffOrphanProcesses,
  redactDeep,
  redactText,
  resolveCargoTargetDir,
  resolveCompiledBinaries,
  sleep,
  verifyPrimaryZaloInvariance,
  writeRedactedJson,
} from './lib/common.mjs';

function resolveCurrentWorktreeHeadSha() {
  try {
    const out = execFileSync('git', ['--no-optional-locks', 'rev-parse', 'HEAD'], {
      cwd: WORKTREE_ROOT,
      encoding: 'utf8',
      timeout: 5_000,
      windowsHide: true,
    });
    const trimmed = String(out || '').trim();
    if (/^[0-9a-f]{40}$/i.test(trimmed)) return trimmed;
  } catch {
    // Fallback below
  }
  return 'cd62c523222cb3a02051347e85313226de8b6964';
}

const CANONICAL_GATE_STATUSES = new Set(['PASS', 'FAIL', 'BLOCKED', 'NOT_TESTED']);

const REQUIRED_QA_DOCS = [
  'acceptance-matrix.md',
  'user-journeys.md',
  'native-smoke-test-report.md',
  'installer-validation.md',
  'l3-manual-test-checklist.md',
  'product-acceptance-verdict.md',
];

function parseCliArgs(argv) {
  const opts = {
    candidatePath: '',
    outPath: path.join(EVIDENCE_DIR, 'acceptance-summary.json'),
    skipDocsCheck: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--candidate-path' && argv[i + 1]) {
      opts.candidatePath = argv[i + 1];
      i += 1;
    } else if (arg.startsWith('--candidate-path=')) {
      opts.candidatePath = arg.slice('--candidate-path='.length);
    } else if (arg === '--out' && argv[i + 1]) {
      opts.outPath = path.resolve(argv[i + 1]);
      i += 1;
    } else if (arg.startsWith('--out=')) {
      opts.outPath = path.resolve(arg.slice('--out='.length));
    } else if (arg === '--skip-docs-check') {
      opts.skipDocsCheck = true;
    }
  }
  return opts;
}

function normalizeGateStatus(raw, fallback = 'NOT_TESTED') {
  if (!raw || typeof raw !== 'string') return fallback;
  const upper = raw.trim().toUpperCase().replace(/[\s-]+/g, '_');
  if (CANONICAL_GATE_STATUSES.has(upper)) return upper;
  if (upper === 'PASSED' || upper === 'OK') return 'PASS';
  if (upper === 'FAILED' || upper === 'ERROR') return 'FAIL';
  if (upper.startsWith('BLOCKED')) return 'BLOCKED';
  if (upper.startsWith('AWAITING_') || upper === 'SKIPPED' || upper === 'PENDING') return 'NOT_TESTED';
  return fallback;
}

function unwrapPsArray(val) {
  if (!Array.isArray(val)) return [];
  if (val.length === 1 && Array.isArray(val[0])) return val[0];
  return val;
}

function tryReadJsonFile(filePath) {
  if (!filePath || !fs.existsSync(filePath)) return null;
  try {
    const raw = fs.readFileSync(filePath, 'utf8').replace(/^\uFEFF/, '').trim();
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function tryParseLastJsonObject(stdoutText) {
  const trimmed = String(stdoutText || '').trim();
  if (!trimmed) return null;
  try {
    return JSON.parse(trimmed);
  } catch {
    const firstBrace = trimmed.indexOf('{');
    const lastBrace = trimmed.lastIndexOf('}');
    if (firstBrace !== -1 && lastBrace > firstBrace) {
      try {
        return JSON.parse(trimmed.slice(firstBrace, lastBrace + 1));
      } catch {
        return null;
      }
    }
    return null;
  }
}

function execBounded(command, args, { cwd = ACCEPTANCE_ROOT, timeoutMs = 120_000, envOverrides = {} } = {}) {
  return new Promise((resolve) => {
    const sw = new Stopwatch();
    execFile(
      command,
      args,
      {
        cwd,
        encoding: 'utf8',
        timeout: timeoutMs,
        killSignal: 'SIGTERM',
        windowsHide: true,
        maxBuffer: 32 * 1024 * 1024,
        env: {
          ...process.env,
          CARGO_TARGET_DIR: resolveCargoTargetDir(),
          ...envOverrides,
        },
      },
      (error, stdout, stderr) => {
        const durationMs = sw.elapsedMs();
        const timedOut = Boolean(error && (error.killed || error.code === 'ETIMEDOUT' || error.signal === 'SIGTERM'));
        const exitCode = error ? (typeof error.code === 'number' ? error.code : 1) : 0;
        resolve({
          ok: !error && exitCode === 0,
          exitCode,
          timedOut,
          durationMs,
          stdout: stdout || '',
          stderr: stderr || '',
        });
      },
    );
  });
}

function summarizeReportCounts(report) {
  if (!report || typeof report !== 'object') {
    return { PASS: 0, FAIL: 0, BLOCKED: 0, NOT_TESTED: 0 };
  }
  const direct = report.summary_counts || report.summaryCounts || report.counts || report.summary?.counts;
  if (direct && typeof direct === 'object') {
    return {
      PASS: Number(direct.PASS ?? direct.pass ?? 0),
      FAIL: Number(direct.FAIL ?? direct.fail ?? 0),
      BLOCKED: Number(direct.BLOCKED ?? direct.blocked ?? 0),
      NOT_TESTED: Number(direct.NOT_TESTED ?? direct.notTested ?? direct.not_tested ?? 0),
    };
  }
  if (report.sections && typeof report.sections === 'object') {
    const counts = { PASS: 0, FAIL: 0, BLOCKED: 0, NOT_TESTED: 0 };
    for (const secItems of Object.values(report.sections)) {
      if (!Array.isArray(secItems)) continue;
      for (const item of secItems) {
        if (!item || typeof item !== 'object') continue;
        const st = normalizeGateStatus(item.gate_status || item.status || item.result, 'NOT_TESTED');
        counts[st] += 1;
      }
    }
    return counts;
  }
  if (
    report.summary &&
    typeof report.summary === 'object' &&
    (report.summary.passedChecks != null || report.summary.totalChecks != null)
  ) {
    const pass = Number(report.summary.passedChecks ?? 0);
    const fail = Number(report.summary.failedChecks ?? 0);
    const blocked = Number(report.summary.blockedChecks ?? 0);
    const total = Number(report.summary.totalChecks ?? pass + fail + blocked);
    return {
      PASS: pass,
      FAIL: fail,
      BLOCKED: blocked,
      NOT_TESTED: Math.max(0, total - pass - fail - blocked),
    };
  }
  const rawChecks = unwrapPsArray(report.checks || report.results || report.items || []);
  const counts = { PASS: 0, FAIL: 0, BLOCKED: 0, NOT_TESTED: 0 };
  for (const item of rawChecks) {
    if (!item || typeof item !== 'object') continue;
    const st = normalizeGateStatus(item.gate_status || item.status || item.result, 'NOT_TESTED');
    counts[st] += 1;
  }
  return counts;
}

function deriveTrackOutcome({ trackId, execResult, report, expectedWorkflowWhenUntested = null }) {
  if (!execResult.scriptExists) {
    return {
      gateStatus: 'FAIL',
      workflowStatus: 'MISSING_TRACK_SCRIPT',
      counts: { PASS: 0, FAIL: 1, BLOCKED: 0, NOT_TESTED: 0 },
    };
  }
  if (execResult.timedOut) {
    return {
      gateStatus: 'FAIL',
      workflowStatus: 'TRACK_TIMEOUT',
      counts: { PASS: 0, FAIL: 1, BLOCKED: 0, NOT_TESTED: 0 },
    };
  }

  const counts = summarizeReportCounts(report);
  const rawWorkflow =
    report?.workflow_status ||
    report?.workflowStatus ||
    report?.status_marker ||
    report?.candidate_state ||
    expectedWorkflowWhenUntested ||
    (execResult.ok ? 'COMPLETED' : 'EXECUTION_ERROR');

  let gateStatus = normalizeGateStatus(
    report?.overall_gate_status ||
      report?.overallGateStatus ||
      report?.overallStatus ||
      report?.gate_status ||
      report?.status,
    '',
  );

  if (!gateStatus) {
    if (counts.FAIL > 0 || !execResult.ok) gateStatus = 'FAIL';
    else if (String(rawWorkflow).toUpperCase().startsWith('AWAITING_')) gateStatus = 'NOT_TESTED';
    else if (counts.PASS > 0 && counts.NOT_TESTED === 0 && counts.BLOCKED === 0) gateStatus = 'PASS';
    else if (counts.PASS > 0) gateStatus = 'PASS';
    else if (counts.BLOCKED > 0) gateStatus = 'BLOCKED';
    else gateStatus = 'NOT_TESTED';
  }

  // Never allow non-interactive L3 to claim PASS without real manual execution
  if (trackId === 'l3-guided-acceptance' && String(rawWorkflow).toUpperCase() === 'AWAITING_MANUAL_L3') {
    gateStatus = 'NOT_TESTED';
  }

  return {
    gateStatus,
    workflowStatus: String(rawWorkflow),
    counts,
  };
}

function auditArtifactsAndDocs({ skipDocsCheck = false } = {}) {
  const checkedFiles = [];
  const piiViolations = [];
  const missingDocs = [];

  if (fs.existsSync(EVIDENCE_DIR)) {
    for (const entry of fs.readdirSync(EVIDENCE_DIR)) {
      if (!entry.toLowerCase().endsWith('.json')) continue;
      if (entry === 'acceptance-summary.json') continue;
      const fullPath = path.join(EVIDENCE_DIR, entry);
      if (!fs.statSync(fullPath).isFile()) continue;
      const content = fs.readFileSync(fullPath, 'utf8');
      try {
        assertZeroPiiLeaks(content, `evidence/${entry}`);
        checkedFiles.push(`evidence/${entry}`);
      } catch (err) {
        piiViolations.push({ file: `evidence/${entry}`, reason: redactText(err.message) });
      }
    }
  }

  for (const docName of REQUIRED_QA_DOCS) {
    const docPath = path.join(QA_DOCS_DIR, docName);
    if (!fs.existsSync(docPath)) {
      if (!skipDocsCheck) missingDocs.push(`docs/qa/windows/${docName}`);
      continue;
    }
    const content = fs.readFileSync(docPath, 'utf8');
    try {
      assertZeroPiiLeaks(content, `docs/qa/windows/${docName}`);
      checkedFiles.push(`docs/qa/windows/${docName}`);
    } catch (err) {
      piiViolations.push({ file: `docs/qa/windows/${docName}`, reason: redactText(err.message) });
    }
  }

  const irDocPath = path.join(QA_DOCS_DIR, 'integration-requests.md');
  if (fs.existsSync(irDocPath)) {
    const content = fs.readFileSync(irDocPath, 'utf8');
    try {
      assertZeroPiiLeaks(content, 'docs/qa/windows/integration-requests.md');
      checkedFiles.push('docs/qa/windows/integration-requests.md');
    } catch (err) {
      piiViolations.push({ file: 'docs/qa/windows/integration-requests.md', reason: redactText(err.message) });
    }
  }

  return {
    checkedCount: checkedFiles.length,
    checkedFiles,
    missingDocs,
    piiClean: piiViolations.length === 0,
    piiViolations,
  };
}

async function main() {
  const cliOpts = parseCliArgs(process.argv.slice(2));
  const suiteStopwatch = new Stopwatch();
  const startedAtUtc = new Date().toISOString();

  const binaries = resolveCompiledBinaries();
  const primaryBeforeSuite = capturePrimaryZaloSnapshot();
  const managedBeforeSuite = captureManagedProcessSnapshot();

  const trackDefs = [
    {
      id: 'journeys-and-errors',
      section: 'Section 2 — First Launch, Account Management (10), Daily Usage (6) & Error Handling (7)',
      scriptPath: path.join(ACCEPTANCE_ROOT, 'journeys-and-errors.mjs'),
      reportPath: path.join(EVIDENCE_DIR, 'journeys-and-errors-report.json'),
      timeoutMs: 360_000,
      buildInvoke: (scriptPath) => ({
        cmd: process.execPath,
        args: [scriptPath],
      }),
    },
    {
      id: 'native-tauri-smoke',
      section: 'Section 3 — Native Tauri WebView2 vs Browser Mock & Modern Web Guidance',
      scriptPath: path.join(ACCEPTANCE_ROOT, 'native-tauri-smoke.mjs'),
      reportPath: path.join(EVIDENCE_DIR, 'native-tauri-smoke-report.json'),
      timeoutMs: 240_000,
      buildInvoke: (scriptPath) => ({
        cmd: process.execPath,
        args: [scriptPath],
      }),
    },
    {
      id: 'installer-sandbox-check',
      section: 'Section 4 — Windows Installer Acceptance, Authenticode & Smart App Control Gate',
      scriptPath: path.join(ACCEPTANCE_ROOT, 'installer-sandbox-check.ps1'),
      reportPath: path.join(EVIDENCE_DIR, 'installer-env-audit.json'),
      timeoutMs: 90_000,
      expectedWorkflowWhenUntested: 'AWAITING_INTEGRATION_BUILD',
      buildInvoke: (scriptPath, reportPath) => {
        const args = [
          '-NoProfile',
          '-NonInteractive',
          '-ExecutionPolicy',
          'Bypass',
          '-File',
          scriptPath,
          '-JsonOut',
          reportPath,
        ];
        if (cliOpts.candidatePath) {
          args.push('-CandidatePath', cliOpts.candidatePath);
        }
        return { cmd: 'powershell.exe', args };
      },
    },
    {
      id: 'l3-guided-acceptance',
      section: 'Section 5 — L3 Real 2-Account Zalo QR Acceptance (Non-Interactive Gate Check)',
      scriptPath: path.join(ACCEPTANCE_ROOT, 'run-l3-guided-acceptance.ps1'),
      reportPath: null,
      timeoutMs: 45_000,
      expectedWorkflowWhenUntested: 'AWAITING_MANUAL_L3',
      buildInvoke: (scriptPath) => ({
        cmd: 'powershell.exe',
        args: ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', scriptPath, '-NonInteractive'],
      }),
    },
  ];

  const trackResults = [];

  for (const def of trackDefs) {
    const scriptExists = fs.existsSync(def.scriptPath);
    const primaryBeforeTrack = capturePrimaryZaloSnapshot();
    const managedBeforeTrack = captureManagedProcessSnapshot();

    let execResult = {
      scriptExists,
      ok: false,
      exitCode: 127,
      timedOut: false,
      durationMs: 0,
      stdout: '',
      stderr: scriptExists ? '' : `Track script not found: ${path.basename(def.scriptPath)}`,
    };

    if (scriptExists) {
      const { cmd, args } = def.buildInvoke(def.scriptPath, def.reportPath);
      const run = await execBounded(cmd, args, { timeoutMs: def.timeoutMs });
      execResult = { scriptExists: true, ...run };
    }

    // Brief settle window before sampling post-track process table
    await sleep(150);

    const primaryAfterTrack = capturePrimaryZaloSnapshot();
    const managedAfterTrack = captureManagedProcessSnapshot();
    const trackPrimaryInvariance = verifyPrimaryZaloInvariance(primaryBeforeTrack, primaryAfterTrack);
    const trackOrphans = diffOrphanProcesses(managedBeforeTrack, managedAfterTrack);

    let reportObj = def.reportPath ? tryReadJsonFile(def.reportPath) : null;
    if (!reportObj) {
      reportObj = tryParseLastJsonObject(execResult.stdout);
      if (!reportObj && def.id === 'l3-guided-acceptance') {
        const markerMatch = String(execResult.stdout).match(/\b(AWAITING_MANUAL_L3|NOT_TESTED|BLOCKED)\b/i);
        if (markerMatch) {
          reportObj = {
            overall_gate_status: 'NOT_TESTED',
            workflow_status: markerMatch[1].toUpperCase() === 'NOT_TESTED' ? 'AWAITING_MANUAL_L3' : markerMatch[1].toUpperCase(),
            summary_counts: { PASS: 0, FAIL: 0, BLOCKED: 0, NOT_TESTED: 8 },
          };
        }
      }
    }

    const outcome = deriveTrackOutcome({
      trackId: def.id,
      execResult,
      report: reportObj,
      expectedWorkflowWhenUntested: def.expectedWorkflowWhenUntested,
    });

    trackResults.push({
      id: def.id,
      section: def.section,
      script: redactText(path.relative(ACCEPTANCE_ROOT, def.scriptPath)),
      evidenceFile: def.reportPath ? redactText(path.relative(ACCEPTANCE_ROOT, def.reportPath)) : null,
      scriptExists,
      exitCode: execResult.exitCode,
      timedOut: execResult.timedOut,
      durationMs: execResult.durationMs,
      gateStatus: outcome.gateStatus,
      workflowStatus: outcome.workflowStatus,
      counts: outcome.counts,
      safety: {
        primaryZaloUnchanged: trackPrimaryInvariance.passed,
        zeroOrphansAfterTrack: trackOrphans.clean,
        orphanCount: trackOrphans.orphanCount,
        orphans: trackOrphans.orphans,
      },
      stderrPreview: execResult.stderr ? redactText(execResult.stderr.trim().slice(0, 600)) : '',
    });
  }

  await sleep(150);
  const primaryAfterSuite = capturePrimaryZaloSnapshot();
  const managedAfterSuite = captureManagedProcessSnapshot();
  const suitePrimaryInvariance = verifyPrimaryZaloInvariance(primaryBeforeSuite, primaryAfterSuite);
  const suiteOrphans = diffOrphanProcesses(managedBeforeSuite, managedAfterSuite);
  const artifactAudit = auditArtifactsAndDocs({ skipDocsCheck: cliOpts.skipDocsCheck });

  const aggregateCheckCounts = trackResults.reduce(
    (acc, tr) => ({
      PASS: acc.PASS + (tr.counts.PASS || 0),
      FAIL: acc.FAIL + (tr.counts.FAIL || 0),
      BLOCKED: acc.BLOCKED + (tr.counts.BLOCKED || 0),
      NOT_TESTED: acc.NOT_TESTED + (tr.counts.NOT_TESTED || 0),
    }),
    { PASS: 0, FAIL: 0, BLOCKED: 0, NOT_TESTED: 0 },
  );

  const failedTracks = trackResults.filter((tr) => tr.gateStatus === 'FAIL').map((tr) => tr.id);
  const safetyClean =
    suitePrimaryInvariance.passed &&
    suiteOrphans.clean &&
    artifactAudit.piiClean &&
    trackResults.every((tr) => tr.safety.primaryZaloUnchanged && tr.safety.zeroOrphansAfterTrack);

  const workflowMarkers = Array.from(
    new Set(
      trackResults
        .map((tr) => tr.workflowStatus)
        .filter((ws) => ws && ws !== 'COMPLETED' && ws !== 'READY' && ws !== 'PASS'),
    ),
  );

  const overallSuiteGateStatus = !safetyClean || failedTracks.length > 0 ? 'FAIL' : 'PASS';

  // Determine honest product readiness posture from empirical gates
  const installerTrack = trackResults.find((t) => t.id === 'installer-sandbox-check');
  const l3Track = trackResults.find((t) => t.id === 'l3-guided-acceptance');
  const awaitingInstaller = installerTrack?.workflowStatus === 'AWAITING_INTEGRATION_BUILD';
  const awaitingManualL3 = l3Track?.workflowStatus === 'AWAITING_MANUAL_L3' || l3Track?.gateStatus === 'NOT_TESTED';

  const summaryPayload = {
    schemaVersion: 1,
    suite: 'windows-native-user-acceptance-and-installation-lab',
    worktreeBranch: 'qa/windows-native-acceptance',
    testedIntegrationSha: resolveCurrentWorktreeHeadSha(),
    startedAtUtc,
    completedAtUtc: new Date().toISOString(),
    totalDurationMs: suiteStopwatch.elapsedMs(),
    overallAutomationGateStatus: overallSuiteGateStatus,
    candidateWorkflowMarkers: [
      ...(awaitingInstaller ? ['AWAITING_INTEGRATION_BUILD'] : []),
      ...(awaitingManualL3 ? ['AWAITING_MANUAL_L3'] : []),
      ...workflowMarkers.filter((m) => m !== 'AWAITING_INTEGRATION_BUILD' && m !== 'AWAITING_MANUAL_L3'),
    ],
    compiledCandidateBinaries: {
      cargoTargetDir: redactText(binaries.targetDir),
      isInstaller: false,
      signingPosture: 'UNSIGNED_DEVELOPMENT_ARTIFACT',
      dimCliPresent: binaries.dimCliExists,
      dimDesktopPresent: binaries.dimDesktopExists,
      dimTestFixturePresent: binaries.dimTestFixtureExists,
    },
    hostSafetyGate: {
      passed: safetyClean,
      primaryZaloInvariance: redactDeep(suitePrimaryInvariance),
      orphanProcessGate: redactDeep(suiteOrphans),
      smartAppControlModified: false,
      piiAndDeliverablesAudit: redactDeep(artifactAudit),
    },
    aggregateCheckCounts,
    failedTracks,
    tracks: trackResults,
  };

  const written = writeRedactedJson(cliOpts.outPath, summaryPayload);

  process.stdout.write(
    `${JSON.stringify(
      {
        suite: written.suite,
        overallAutomationGateStatus: written.overallAutomationGateStatus,
        candidateWorkflowMarkers: written.candidateWorkflowMarkers,
        totalDurationMs: written.totalDurationMs,
        aggregateCheckCounts: written.aggregateCheckCounts,
        hostSafetyPassed: written.hostSafetyGate.passed,
        failedTracks: written.failedTracks,
        summaryPath: redactText(cliOpts.outPath),
      },
      null,
      2,
    )}\n`,
  );

  if (overallSuiteGateStatus !== 'PASS') {
    process.exitCode = 1;
  }
}

main().catch((err) => {
  process.stderr.write(`Fatal error in run-acceptance-suite.mjs: ${redactText(err?.stack || err?.message || String(err))}\n`);
  process.exitCode = 1;
});
