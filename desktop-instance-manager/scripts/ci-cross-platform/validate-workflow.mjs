#!/usr/bin/env node
/**
 * Lean static validator for `.github/workflows/cross-platform-validation.yml`
 * and non-conflict / CLI-flag parity verifier against `.github/workflows/ci.yml`
 * and `desktop-instance-manager/scripts/ci-cross-platform/*.mjs`.
 *
 * Uses Node.js 22 built-in modules (`node:*`) ONLY — zero external npm dependencies.
 */

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const DIM_ROOT = path.resolve(__dirname, '..', '..');
const REPO_ROOT = path.resolve(DIM_ROOT, '..');

const WORKFLOW_PATH = path.join(REPO_ROOT, '.github', 'workflows', 'cross-platform-validation.yml');
const BASELINE_CI_PATH = path.join(REPO_ROOT, '.github', 'workflows', 'ci.yml');

const ALLOWED_USES = new Set([
  'actions/checkout@v4',
  'actions/setup-node@v4',
  'dtolnay/rust-toolchain@stable',
  'actions/cache@v4',
  'EmbarkStudios/cargo-deny-action@v2',
  'actions/upload-artifact@v4',
  'actions/download-artifact@v4',
]);

const EXPECTED_OS_RUNNERS = {
  'ubuntu-2404-build': 'ubuntu-24.04',
  'windows-msvc-build': 'windows-2022',
  'macos-arm64-build': 'macos-15',
  'macos-intel-build': 'macos-15-intel',
};

const REQUIRED_JOBS = [
  'rust-workspace-validation',
  'dependency-audit',
  'ubuntu-2404-build',
  'windows-msvc-build',
  'macos-arm64-build',
  'macos-intel-build',
  'artifact-verification',
];

function parseArgs(argv) {
  const opts = { out: null, json: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--json') opts.json = true;
    else if (arg === '--out') opts.out = argv[++i] ?? null;
    else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node validate-workflow.mjs [--json] [--out <report.json>]');
      process.exit(0);
    } else {
      throw new Error(`Unknown CLI option: ${arg}`);
    }
  }
  return opts;
}

function extractTopLevelBlocks(rawText) {
  const lines = rawText.replace(/\r\n/g, '\n').split('\n');
  const topKeys = [];
  let inBlockScalar = false;
  let blockScalarIndent = 0;

  for (let idx = 0; idx < lines.length; idx += 1) {
    const line = lines[idx];
    const trimmed = line.trim();
    if (!trimmed) continue;
    const indent = line.match(/^ */)[0].length;
    if (inBlockScalar) {
      if (indent > blockScalarIndent) continue;
      inBlockScalar = false;
    }
    if (trimmed.startsWith('#')) continue;
    if (indent === 0) {
      const m = line.match(/^([A-Za-z0-9_-]+)\s*:(.*)$/);
      if (m) {
        topKeys.push({ key: m[1], rest: m[2].trim(), startLine: idx });
        if (/^[|>][-+]?$/.test(m[2].trim())) {
          inBlockScalar = true;
          blockScalarIndent = 0;
        }
      }
    } else if (/:\s*[|>][-+]?\s*$/.test(line)) {
      inBlockScalar = true;
      blockScalarIndent = indent;
    }
  }

  const sections = new Map();
  for (let i = 0; i < topKeys.length; i += 1) {
    const cur = topKeys[i];
    const endLine = i + 1 < topKeys.length ? topKeys[i + 1].startLine : lines.length;
    sections.set(cur.key, { rest: cur.rest, lines: lines.slice(cur.startLine, endLine) });
  }
  return sections;
}

function extractChildBlocks(sectionLines, childIndent) {
  const children = new Map();
  const entries = [];
  let inBlockScalar = false;
  let blockScalarIndent = 0;

  for (let i = 1; i < sectionLines.length; i += 1) {
    const line = sectionLines[i];
    const trimmed = line.trim();
    if (!trimmed) continue;
    const indent = line.match(/^ */)[0].length;
    if (inBlockScalar) {
      if (indent > blockScalarIndent) continue;
      inBlockScalar = false;
    }
    if (trimmed.startsWith('#')) continue;
    if (/:\s*[|>][-+]?\s*$/.test(line)) {
      inBlockScalar = true;
      blockScalarIndent = indent;
    }
    if (indent === childIndent) {
      const m = line.match(new RegExp(`^ {${childIndent}}([A-Za-z0-9_-]+)\\s*:(.*)$`));
      if (m) entries.push({ key: m[1], rest: m[2].trim(), idx: i });
    }
  }

  for (let i = 0; i < entries.length; i += 1) {
    const cur = entries[i];
    const nextIdx = i + 1 < entries.length ? entries[i + 1].idx : sectionLines.length;
    children.set(cur.key, { rest: cur.rest, lines: sectionLines.slice(cur.idx, nextIdx) });
  }
  return children;
}

function validateWorkflow() {
  const checks = [];
  const errors = [];
  const record = (id, ok, detail) => {
    checks.push({ id, ok, detail });
    if (!ok) errors.push(`[${id}] ${detail}`);
  };

  if (!fs.existsSync(WORKFLOW_PATH) || !fs.existsSync(BASELINE_CI_PATH)) {
    record('workflow-files', false, 'Missing cross-platform-validation.yml or baseline ci.yml');
    return { ok: false, checks, errors };
  }

  const rawWorkflow = fs.readFileSync(WORKFLOW_PATH, 'utf8');
  const rawBaseline = fs.readFileSync(BASELINE_CI_PATH, 'utf8');
  const sections = extractTopLevelBlocks(rawWorkflow);
  const baselineSections = extractTopLevelBlocks(rawBaseline);

  // 1. Basic YAML sanity & non-conflicting workflow identity
  const hasTabs = rawWorkflow.split('\n').some((l) => /^\s*\t/.test(l));
  const wfName = sections.get('name')?.rest ?? '';
  const ciName = baselineSections.get('name')?.rest ?? '';
  record(
    'workflow-identity-and-yaml',
    !hasTabs && Boolean(wfName) && wfName !== ciName,
    !hasTabs && wfName !== ciName
      ? `Valid space-indented YAML; distinct workflow "${wfName}" vs "${ciName}"`
      : 'YAML tab error or duplicate workflow name with ci.yml',
  );

  // 2. Trigger & ordered workflow_dispatch inputs
  const onBlock = sections.get('on');
  const triggers = onBlock ? extractChildBlocks(onBlock.lines, 2) : new Map();
  const triggerKeys = [...triggers.keys()];
  const inputsMap = triggers.get('workflow_dispatch')
    ? extractChildBlocks(
        extractChildBlocks(triggers.get('workflow_dispatch').lines, 4).get('inputs')?.lines ?? [],
        6,
      )
    : new Map();
  const inputOrder = [...inputsMap.keys()];
  const expectedOrder = ['enable_ubuntu', 'enable_windows', 'enable_macos_arm64', 'enable_macos_intel', 'bundles'];
  const macOptOut =
    (inputsMap.get('enable_macos_arm64')?.lines.join('\n') ?? '').includes('default: false') &&
    (inputsMap.get('enable_macos_intel')?.lines.join('\n') ?? '').includes('default: false');
  const triggerOk =
    triggerKeys.length === 1 &&
    triggerKeys[0] === 'workflow_dispatch' &&
    JSON.stringify(inputOrder) === JSON.stringify(expectedOrder) &&
    macOptOut;
  record(
    'trigger-and-dispatch-inputs',
    triggerOk,
    triggerOk
      ? `workflow_dispatch only; inputs ordered [${inputOrder.join(', ')}] with macOS 10x opt-in defaults=false`
      : `Invalid triggers [${triggerKeys.join(', ')}] or input order [${inputOrder.join(', ')}]`,
  );

  // 3. Read-only permissions & zero release/secret exposure (ignoring comment lines)
  const nonCommentWorkflow = rawWorkflow
    .split('\n')
    .filter((l) => !l.trim().startsWith('#'))
    .join('\n');
  const perms = sections.get('permissions') ? extractChildBlocks(sections.get('permissions').lines, 2) : new Map();
  const readonlyOk =
    perms.size === 1 &&
    perms.get('contents')?.rest === 'read' &&
    !/^\s*[a-z-]+:\s*write\b|\bwrite-all\b/im.test(nonCommentWorkflow);
  const noReleaseOrSecrets =
    !/softprops\/action-gh-release|actions\/create-release|tauri-apps\/tauri-action|\bgh\s+release\s+create\b/i.test(nonCommentWorkflow) &&
    !/\$\{\{\s*secrets\.[^}]+\}\}/.test(nonCommentWorkflow);
  record(
    'readonly-and-supply-chain-guards',
    readonlyOk && noReleaseOrSecrets,
    readonlyOk && noReleaseOrSecrets
      ? 'permissions: contents: read; zero write scopes, zero release actions, zero secret refs'
      : 'Write permission, release step, or secret reference detected',
  );

  // 4. Trusted pinned actions, cache isolation & 7-day artifact retention
  const usesList = [...rawWorkflow.matchAll(/^\s*(?:-\s*)?uses:\s*([^\s#]+)/gm)].map((m) => m[1].trim());
  const badUses = usesList.filter((u) => !ALLOWED_USES.has(u));
  const uploadBlocks = [...rawWorkflow.matchAll(/uses:\s*actions\/upload-artifact@v4([\s\S]*?)(?=\n\s*-\s+name:|\n  [a-z0-9-]+:|\s*$)/g)];
  const missingRetention = uploadBlocks.some((m) => !/retention-days:\s*7\b/.test(m[1]));
  const ciCacheKeys = new Set([...rawBaseline.matchAll(/key:\s*(.+)/g)].map((m) => m[1].trim()));
  const cacheCollision = [...rawWorkflow.matchAll(/key:\s*(.+)/g)].some((m) => ciCacheKeys.has(m[1].trim()));
  const unpinnedTauriLatest = /@tauri-apps\/cli@latest\b/.test(rawWorkflow);
  record(
    'pinned-actions-and-retention',
    badUses.length === 0 && !missingRetention && !cacheCollision && !unpinnedTauriLatest,
    badUses.length === 0 && !missingRetention && !cacheCollision && !unpinnedTauriLatest
      ? `${usesList.length} pinned official action refs, isolated cache keys, pinned @tauri-apps/cli, retention-days: 7`
      : `Disallowed uses [${badUses.join(', ')}], missing 7d retention, cache collision, or floating @latest`,
  );

  // 5. Required 7 jobs, explicit native OS runner labels (no macos-14 Intel cross-compile!), and timeouts
  const jobs = sections.get('jobs') ? extractChildBlocks(sections.get('jobs').lines, 2) : new Map();
  const missingJobs = REQUIRED_JOBS.filter((j) => !jobs.has(j));
  const runnerIssues = [];
  for (const jobId of REQUIRED_JOBS) {
    const jb = jobs.get(jobId);
    if (!jb) continue;
    const props = extractChildBlocks(jb.lines, 4);
    if (!props.has('timeout-minutes')) runnerIssues.push(`${jobId} missing timeout-minutes`);
    if (EXPECTED_OS_RUNNERS[jobId]) {
      const actualRunner = props.get('runs-on')?.rest ?? '';
      if (actualRunner !== EXPECTED_OS_RUNNERS[jobId]) {
        runnerIssues.push(`${jobId} runs-on "${actualRunner}" != expected native "${EXPECTED_OS_RUNNERS[jobId]}"`);
      }
      const body = jb.lines.join('\n');
      if (!body.includes('rustc -vV')) {
        runnerIssues.push(`${jobId} missing native rustc -vV host check`);
      }
      if (jobId !== 'windows-msvc-build' && !body.includes('uname -m')) {
        runnerIssues.push(`${jobId} missing native uname -m check`);
      }
    }
  }
  record(
    'jobs-and-native-os-runners',
    missingJobs.length === 0 && runnerIssues.length === 0,
    missingJobs.length === 0 && runnerIssues.length === 0
      ? 'All 7 jobs present with timeouts and native runners (ubuntu-24.04, windows-2022, macos-15, macos-15-intel) + uname/rustc -vV gates'
      : `Issues: ${[...missingJobs.map((j) => `missing ${j}`), ...runnerIssues].join('; ')}`,
  );

  // 6. Referenced scripts exist, pass `node --check`, and support all workflow CLI flags
  const invocationRegex = /node\s+(desktop-instance-manager\/scripts\/ci-cross-platform\/[A-Za-z0-9._-]+\.mjs)([\s\S]*?)(?=\n\s*(?:node\b|cat\b|Get-Content\b|if\b|for\b|BUNDLE_ROOT=|OUT_DIR=|\$|$))/g;
  const scriptFlagsMap = new Map();
  for (const match of rawWorkflow.matchAll(invocationRegex)) {
    const relScript = match[1];
    const splitLines = (match[2] ?? '').replace(/\r\n/g, '\n').split('\n');
    const cmdLines = [];
    for (const ln of splitLines) {
      cmdLines.push(ln);
      const t = ln.trim();
      if (!t.endsWith('\\') && !t.endsWith('`')) break;
    }
    const flags = [...cmdLines.join(' ').matchAll(/(?:^|\s)(--[a-z0-9-]+)/g)].map((m) => m[1]);
    if (!scriptFlagsMap.has(relScript)) scriptFlagsMap.set(relScript, new Set());
    for (const fl of flags) scriptFlagsMap.get(relScript).add(fl);
  }
  const inlineMergeMatch = rawWorkflow.match(
    /'(desktop-instance-manager\/scripts\/ci-cross-platform\/generate-artifact-manifest\.mjs)'([\s\S]*?)\]/,
  );
  if (inlineMergeMatch) {
    const relScript = inlineMergeMatch[1];
    if (!scriptFlagsMap.has(relScript)) scriptFlagsMap.set(relScript, new Set());
    for (const m of inlineMergeMatch[2].matchAll(/'(--[a-z0-9-]+)'/g)) {
      scriptFlagsMap.get(relScript).add(m[1]);
    }
  }

  const scriptProblems = [];
  for (const [relScript, flagsSet] of scriptFlagsMap.entries()) {
    const absScript = path.join(REPO_ROOT, relScript);
    const base = path.basename(relScript);
    if (!fs.existsSync(absScript)) {
      scriptProblems.push(`missing ${relScript}`);
      continue;
    }
    try {
      execFileSync(process.execPath, ['--check', absScript], { stdio: 'pipe' });
    } catch (err) {
      scriptProblems.push(`syntax error in ${base}: ${err.message}`);
      continue;
    }
    const src = fs.readFileSync(absScript, 'utf8');
    const missingFlags = [...flagsSet].filter((f) => !src.includes(f));
    if (missingFlags.length > 0) {
      scriptProblems.push(`${base} missing flag(s): ${missingFlags.join(', ')}`);
    }
  }
  record(
    'script-existence-and-cli-parity',
    scriptFlagsMap.size >= 6 && scriptProblems.length === 0,
    scriptProblems.length === 0
      ? `All ${scriptFlagsMap.size} referenced CI scripts exist, pass node --check, and match workflow CLI flags`
      : `Script contract issues: ${scriptProblems.join('; ')}`,
  );

  return {
    ok: errors.length === 0,
    workflow_file: '.github/workflows/cross-platform-validation.yml',
    baseline_ci_file: '.github/workflows/ci.yml',
    live_ci_status: 'BLOCKED (REMOTE_WORKFLOW_DISPATCH_UNREGISTERED_AND_MISSING_WORKFLOW_SCOPE)',
    total_checks: checks.length,
    passed_checks: checks.filter((c) => c.ok).length,
    failed_checks: errors.length,
    checks,
    errors,
  };
}

const opts = parseArgs(process.argv.slice(2));
const report = validateWorkflow();

if (opts.out) {
  fs.mkdirSync(path.dirname(path.resolve(opts.out)), { recursive: true });
  fs.writeFileSync(path.resolve(opts.out), JSON.stringify(report, null, 2) + '\n', 'utf8');
}

if (opts.json) {
  console.log(JSON.stringify(report, null, 2));
} else {
  console.log(`=== Static Workflow & Native Runner Contract Check ===`);
  console.log(`Target Workflow : ${report.workflow_file}`);
  console.log(`Live CI Status  : ${report.live_ci_status}`);
  console.log(`Checks Passed   : ${report.passed_checks}/${report.total_checks}`);
  for (const c of report.checks) {
    console.log(`  [${c.ok ? 'PASS' : 'FAIL'}] ${c.id}: ${c.detail}`);
  }
}

process.exit(report.ok ? 0 : 1);
