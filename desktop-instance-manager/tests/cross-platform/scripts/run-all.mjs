#!/usr/bin/env node
/**
 * Unified OS-detecting entrypoint for the Cross-Platform Reliability & Contract Test System.
 * Dispatches strictly to the active host OS runner without cross-marking foreign OS targets.
 */

import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const extraArgs = process.argv.slice(2);

if (process.platform === 'win32') {
  const psScript = path.join(__dirname, 'run-windows.ps1');
  const res = spawnSync(
    'powershell.exe',
    ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', psScript, ...extraArgs],
    { stdio: 'inherit', windowsHide: true },
  );
  process.exit(res.status ?? 1);
} else if (process.platform === 'darwin') {
  const macScript = path.join(__dirname, 'run-macos.sh');
  const res = spawnSync('bash', [macScript, ...extraArgs], { stdio: 'inherit' });
  process.exit(res.status ?? 1);
} else if (process.platform === 'linux') {
  const lnxScript = path.join(__dirname, 'run-ubuntu.sh');
  const res = spawnSync('bash', [lnxScript, ...extraArgs], { stdio: 'inherit' });
  process.exit(res.status ?? 1);
} else {
  console.error(`Unsupported host platform: ${process.platform}`);
  process.exit(2);
}
