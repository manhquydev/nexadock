#!/usr/bin/env node
/**
 * Static validator for `.github/workflows/cross-platform-validation.yml`
 * and non-conflict verifier against `.github/workflows/ci.yml`.
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

const FORBIDDEN_RELEASE_PATTERNS = [
  /softprops\/action-gh-release/i,
  /actions\/create-release/i,
  /ncipollo\/release-action/i,
  /tauri-apps\/tauri-action/i,
  /\bgh\s+release\s+create\b/i,
];

const REQUIRED_JOBS = [
  'rust-workspace-validation',
  'dependency-audit',
  'windows-msvc-build',
  'macos-arm64-build',
  'macos-intel-build',
  'ubuntu-2404-build',
  'artifact-verification',
];

const FORBIDDEN_TRIGGERS = [
  'push',
  'pull_request',
  'pull_request_target',
  'schedule',
  'release',
  'workflow_run',
  'repository_dispatch',
];

function parseArgs(argv) {
  const opts = { out: null, json: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--json') {
      opts.json = true;
    } else if (arg === '--out') {
      opts.out = argv[i + 1] ?? null;
      i += 1;
    } else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node validate-workflow.mjs [--json] [--out <report.json>]');
      process.exit(0);
    } else {
      throw new Error(`Unknown CLI option: ${arg}`);
    }
  }
  return opts;
}

/**
 * Extracts top-level YAML sections (indent = 0) as raw line ranges,
 * ignoring full-line comments and blank lines outside scalar blocks.
 */
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
      if (indent > blockScalarIndent) {
        continue;
      }
      inBlockScalar = false;
    }

    if (trimmed.startsWith('#')) continue;

    if (indent === 0) {
      const m = line.match(/^([A-Za-z0-9_-]+)\s*:(.*)$/);
      if (m) {
        const key = m[1];
        const rest = m[2].trim();
        topKeys.push({ key, rest, startLine: idx });
        if (rest === '|' || rest === '>' || rest === '|-' || rest === '>-') {
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
    sections.set(cur.key, {
      rest: cur.rest,
      startLine: cur.startLine + 1,
      lines: lines.slice(cur.startLine, endLine),
    });
  }
  return { lines, sections };
}

/**
 * Extracts direct child keys at a specific indentation level inside a section's lines,
 * properly skipping multiline `|` / `>` scalar blocks (such as `run: |`).
 */
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
      const re = new RegExp(`^ {${childIndent}}([A-Za-z0-9_-]+)\\s*:(.*)$`);
      const m = line.match(re);
      if (m) {
        entries.push({ key: m[1], rest: m[2].trim(), idx: i });
      }
    }
  }

  for (let i = 0; i < entries.length; i += 1) {
    const cur = entries[i];
    const nextIdx = i + 1 < entries.length ? entries[i + 1].idx : sectionLines.length;
    children.set(cur.key, {
      rest: cur.rest,
      lines: sectionLines.slice(cur.idx, nextIdx),
    });
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

  if (!fs.existsSync(WORKFLOW_PATH)) {
    record('workflow-exists', false, `Missing workflow file: ${WORKFLOW_PATH}`);
    return { ok: false, checks, errors };
  }
  if (!fs.existsSync(BASELINE_CI_PATH)) {
    record('baseline-ci-exists', false, `Missing baseline CI file: ${BASELINE_CI_PATH}`);
    return { ok: false, checks, errors };
  }

  const rawWorkflow = fs.readFileSync(WORKFLOW_PATH, 'utf8');
  const rawBaseline = fs.readFileSync(BASELINE_CI_PATH, 'utf8');

  record('workflow-exists', true, 'Found .github/workflows/cross-platform-validation.yml');
  record('baseline-ci-exists', true, 'Found .github/workflows/ci.yml');

  // Check tabs (YAML forbids tab indentation)
  const tabLine = rawWorkflow
    .replace(/\r\n/g, '\n')
    .split('\n')
    .findIndex((l) => /^\s*\t/.test(l));
  record(
    'yaml-no-tab-indent',
    tabLine === -1,
    tabLine === -1 ? 'Zero tab indentation characters in YAML' : `Tab indentation found on line ${tabLine + 1}`
  );

  const { sections } = extractTopLevelBlocks(rawWorkflow);
  const baselineParsed = extractTopLevelBlocks(rawBaseline);

  // 1. Distinct workflow names & non-conflict with ci.yml
  const wfName = sections.get('name')?.rest ?? '';
  const ciName = baselineParsed.sections.get('name')?.rest ?? '';
  record(
    'distinct-workflow-name',
    Boolean(wfName) && wfName !== ciName,
    `Workflow "${wfName}" is distinct from baseline "${ciName}"`
  );

  // 2. Trigger validation: workflow_dispatch ONLY, no schedule/push/PR
  const onSection = sections.get('on');
  if (!onSection) {
    record('trigger-workflow-dispatch-only', false, 'Missing top-level `on:` trigger block');
  } else {
    const triggers = extractChildBlocks(onSection.lines, 2);
    const triggerKeys = [...triggers.keys()];
    const forbiddenFound = triggerKeys.filter((k) => FORBIDDEN_TRIGGERS.includes(k));
    const dispatchOnly = triggerKeys.length === 1 && triggerKeys[0] === 'workflow_dispatch' && forbiddenFound.length === 0;
    record(
      'trigger-workflow-dispatch-only',
      dispatchOnly,
      dispatchOnly
        ? 'Trigger is strictly `workflow_dispatch` only (zero push/PR/schedule conflict with ci.yml)'
        : `Invalid triggers detected: [${triggerKeys.join(', ')}]`
    );

    const wdBlock = triggers.get('workflow_dispatch');
    const wdChildren = wdBlock ? extractChildBlocks(wdBlock.lines, 4) : new Map();
    const inputsBlock = wdChildren.get('inputs');
    const inputs = inputsBlock ? extractChildBlocks(inputsBlock.lines, 6) : new Map();

    const expectedDefaults = {
      enable_windows: 'true',
      enable_ubuntu: 'true',
      enable_macos_arm64: 'false',
      enable_macos_intel: 'false',
      bundles: 'all',
    };

    for (const [inputName, expectedDefault] of Object.entries(expectedDefaults)) {
      const inp = inputs.get(inputName);
      if (!inp) {
        record(`dispatch-input-${inputName}`, false, `Missing workflow_dispatch input: ${inputName}`);
      } else {
        const props = extractChildBlocks(inp.lines, 8);
        const rawDef = (props.get('default')?.rest ?? '').replace(/^["']|["']$/g, '');
        record(
          `dispatch-input-${inputName}`,
          rawDef === expectedDefault,
          `Input ${inputName} present with default=${rawDef} (expected ${expectedDefault})`
        );
      }
    }
  }

  // 3. Permissions strictly read-only (`contents: read`), zero `write` scopes anywhere
  const permSection = sections.get('permissions');
  if (!permSection) {
    record('permissions-contents-read', false, 'Missing top-level `permissions:` block');
  } else {
    const perms = extractChildBlocks(permSection.lines, 2);
    const permEntries = [...perms.entries()];
    const strictlyContentsRead =
      permEntries.length === 1 && permEntries[0][0] === 'contents' && permEntries[0][1].rest === 'read';
    record(
      'permissions-contents-read',
      strictlyContentsRead,
      strictlyContentsRead
        ? 'Top-level permissions strictly `contents: read`'
        : `Unexpected top-level permissions: ${JSON.stringify(permEntries.map(([k, v]) => `${k}:${v.rest}`))}`
    );
  }

  const writePermMatch = rawWorkflow.match(/^\s*[a-z-]+:\s*write\b|\bwrite-all\b/im);
  record(
    'zero-write-permissions',
    !writePermMatch,
    !writePermMatch ? 'Zero write permissions anywhere in workflow' : `Forbidden write permission: ${writePermMatch[0].trim()}`
  );

  // 4. Zero automatic release actions or gh release commands
  const releaseHit = FORBIDDEN_RELEASE_PATTERNS.find((re) => re.test(rawWorkflow));
  record(
    'no-release-creation',
    !releaseHit,
    !releaseHit ? 'Zero release actions or `gh release create` commands' : `Forbidden release pattern matched: ${releaseHit}`
  );

  // 5. Zero secret references (including cache keys/paths)
  const secretHit = rawWorkflow.match(/\$\{\{\s*secrets\.[^}]+\}\}/);
  record(
    'zero-secret-references',
    !secretHit,
    !secretHit ? 'Zero `${{ secrets.* }}` references in workflow or cache keys' : `Forbidden secret reference: ${secretHit[0]}`
  );

  // 6. Trusted official pinned actions & short upload retention
  const usesMatches = [...rawWorkflow.matchAll(/^\s*(?:-\s*)?uses:\s*([^\s#]+)/gm)].map((m) => m[1].trim());
  const untrustedUses = usesMatches.filter((u) => !ALLOWED_USES.has(u));
  record(
    'pinned-official-actions',
    usesMatches.length > 0 && untrustedUses.length === 0,
    untrustedUses.length === 0
      ? `All ${usesMatches.length} action \`uses:\` steps match pinned allowlist`
      : `Disallowed or unpinned action(s): ${untrustedUses.join(', ')}`
  );

  const uploadBlocks = [...rawWorkflow.matchAll(/uses:\s*actions\/upload-artifact@v4([\s\S]*?)(?=\n\s*-\s+name:|\n  [a-z0-9-]+:|\s*$)/g)];
  const missingRetention = uploadBlocks.filter((m) => !/retention-days:\s*7\b/.test(m[1]));
  record(
    'artifact-retention-7d',
    uploadBlocks.length > 0 && missingRetention.length === 0,
    missingRetention.length === 0
      ? `All ${uploadBlocks.length} upload-artifact steps enforce retention-days: 7`
      : `${missingRetention.length} upload-artifact step(s) missing retention-days: 7`
  );

  // 7. Required 7 jobs & runner checks
  const jobsSection = sections.get('jobs');
  const jobs = jobsSection ? extractChildBlocks(jobsSection.lines, 2) : new Map();
  for (const jobId of REQUIRED_JOBS) {
    const jobBlock = jobs.get(jobId);
    record(
      `job-present-${jobId}`,
      Boolean(jobBlock),
      jobBlock ? `Job \`${jobId}\` defined` : `Missing required job \`${jobId}\``
    );
  }

  const retiredMacRunner = rawWorkflow.match(/runs-on:\s*macos-1[123]\b/);
  record(
    'macos-runner-not-retired',
    !retiredMacRunner,
    !retiredMacRunner
      ? 'macOS jobs use GA `macos-14` runner (no retired macos-12/13 runner dependency)'
      : `Deprecated/retired macOS runner detected: ${retiredMacRunner[0]}`
  );

  // Check cache key isolation against ci.yml
  const xplatCacheKeys = [...rawWorkflow.matchAll(/key:\s*(.+)/g)].map((m) => m[1].trim());
  const ciCacheKeys = new Set([...rawBaseline.matchAll(/key:\s*(.+)/g)].map((m) => m[1].trim()));
  const collidingKeys = xplatCacheKeys.filter((k) => ciCacheKeys.has(k));
  record(
    'cache-key-non-collision',
    xplatCacheKeys.length > 0 && collidingKeys.length === 0,
    collidingKeys.length === 0
      ? `All ${xplatCacheKeys.length} cache keys are lockfile-hashed and isolated from ci.yml`
      : `Cache key collision with ci.yml: ${collidingKeys.join(', ')}`
  );

  // 8. Verify every referenced `scripts/ci-cross-platform/*.mjs` file exists, passes `node --check`,
  //    and mentions every `--<flag>` invoked by the workflow YAML.
  const invocationRegex = /node\s+(desktop-instance-manager\/scripts\/ci-cross-platform\/[A-Za-z0-9._-]+\.mjs)([\s\S]*?)(?=\n\s*(?:node\b|cat\b|Get-Content\b|if\b|for\b|BUNDLE_ROOT=|OUT_DIR=|\$|$))/g;
  const scriptFlagsMap = new Map();

  for (const match of rawWorkflow.matchAll(invocationRegex)) {
    const relScript = match[1];
    const argsChunk = match[2] ?? '';
    // Keep lines that belong to the line-continued command (`\` or PowerShell backtick `` ` ``)
    const cmdLines = [];
    const splitLines = argsChunk.replace(/\r\n/g, '\n').split('\n');
    for (let i = 0; i < splitLines.length; i += 1) {
      cmdLines.push(splitLines[i]);
      const trimmed = splitLines[i].trim();
      if (!trimmed.endsWith('\\') && !trimmed.endsWith('`')) break;
    }
    const joinedArgs = cmdLines.join(' ');
    const flags = [...joinedArgs.matchAll(/(?:^|\s)(--[a-z0-9-]+)/g)].map((m) => m[1]);
    if (!scriptFlagsMap.has(relScript)) {
      scriptFlagsMap.set(relScript, new Set());
    }
    for (const fl of flags) {
      scriptFlagsMap.get(relScript).add(fl);
    }
  }

  // Also capture execFileSync invocation of generate-artifact-manifest.mjs in artifact-verification
  const inlineMergeMatch = rawWorkflow.match(
    /'(desktop-instance-manager\/scripts\/ci-cross-platform\/generate-artifact-manifest\.mjs)'([\s\S]*?)\]/
  );
  if (inlineMergeMatch) {
    const relScript = inlineMergeMatch[1];
    const flags = [...inlineMergeMatch[2].matchAll(/'(--[a-z0-9-]+)'/g)].map((m) => m[1]);
    if (!scriptFlagsMap.has(relScript)) scriptFlagsMap.set(relScript, new Set());
    for (const fl of flags) scriptFlagsMap.get(relScript).add(fl);
  }

  record(
    'referenced-scripts-discovered',
    scriptFlagsMap.size >= 6,
    `Discovered ${scriptFlagsMap.size} distinct scripts/ci-cross-platform/*.mjs references in workflow`
  );

  for (const [relScript, flagsSet] of scriptFlagsMap.entries()) {
    const absScript = path.join(REPO_ROOT, relScript);
    const baseName = path.basename(relScript);
    if (!fs.existsSync(absScript)) {
      record(`script-exists-${baseName}`, false, `Referenced script does not exist on disk: ${relScript}`);
      continue;
    }
    record(`script-exists-${baseName}`, true, `Referenced script exists: ${relScript}`);

    try {
      execFileSync(process.execPath, ['--check', absScript], { stdio: 'pipe' });
      record(`script-syntax-${baseName}`, true, `node --check passed for ${baseName}`);
    } catch (err) {
      record(`script-syntax-${baseName}`, false, `node --check failed for ${baseName}: ${err.message}`);
      continue;
    }

    const scriptSrc = fs.readFileSync(absScript, 'utf8');
    const missingFlags = [...flagsSet].filter((flag) => !scriptSrc.includes(flag));
    record(
      `script-flags-${baseName}`,
      missingFlags.length === 0,
      missingFlags.length === 0
        ? `All workflow CLI flags [${[...flagsSet].join(', ')}] supported in ${baseName}`
        : `Workflow passes unsupported CLI flag(s) to ${baseName}: ${missingFlags.join(', ')}`
    );
  }

  return {
    ok: errors.length === 0,
    workflow_file: '.github/workflows/cross-platform-validation.yml',
    baseline_ci_file: '.github/workflows/ci.yml',
    live_ci_status: 'NOT_EXECUTED (STATICALLY_VALIDATED_ONLY)',
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
  console.log(`=== Static Workflow Policy & Contract Validation ===`);
  console.log(`Target Workflow : ${report.workflow_file}`);
  console.log(`Baseline CI     : ${report.baseline_ci_file}`);
  console.log(`Live CI Status  : ${report.live_ci_status}`);
  console.log(`Checks Passed   : ${report.passed_checks}/${report.total_checks}`);
  for (const c of report.checks) {
    console.log(`  [${c.ok ? 'PASS' : 'FAIL'}] ${c.id}: ${c.detail}`);
  }
  if (!report.ok) {
    console.error(`\nValidation FAILED with ${report.failed_checks} error(s).`);
  } else {
    console.log(`\nValidation PASSED (statically validated; remote GitHub Actions NOT executed).`);
  }
}

process.exit(report.ok ? 0 : 1);
