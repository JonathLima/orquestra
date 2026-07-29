#!/usr/bin/env node
import { copyFileSync, chmodSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageDir = resolve(scriptDir, '..');
const repoRoot = resolve(packageDir, '..', '..');
const binaryName = process.platform === 'win32' ? 'orquestra-cli.exe' : 'orquestra-cli';
const source = join(repoRoot, 'target', 'release', binaryName);
const destinationDir = join(packageDir, 'bin');
const destination = join(destinationDir, binaryName);

const build = spawnSync('cargo', ['build', '-p', 'orquestra-cli', '--release'], {
  cwd: repoRoot,
  stdio: 'inherit',
});

if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

mkdirSync(destinationDir, { recursive: true });
copyFileSync(source, destination);

if (process.platform !== 'win32') {
  chmodSync(destination, 0o755);
}

console.log(`Copied ${source} -> ${destination}`);
