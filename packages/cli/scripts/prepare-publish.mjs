#!/usr/bin/env node
import { existsSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageDir = join(scriptDir, '..');
const manifest = JSON.parse(readFileSync(join(packageDir, 'package.json'), 'utf8'));
const expectedPackages = [
  '@jonathlima/orquestra-win32-x64',
  '@jonathlima/orquestra-darwin-x64',
  '@jonathlima/orquestra-darwin-arm64',
  '@jonathlima/orquestra-linux-x64',
  '@jonathlima/orquestra-linux-arm64',
];

const actualPackages = Object.keys(manifest.optionalDependencies ?? {}).sort();
if (JSON.stringify(actualPackages) !== JSON.stringify(expectedPackages.sort())) {
  console.error('Optional platform package inventory is incomplete.');
  process.exit(1);
}

for (const packageName of expectedPackages) {
  if (manifest.optionalDependencies[packageName] !== manifest.version) {
    console.error(`${packageName} must use wrapper version ${manifest.version}.`);
    process.exit(1);
  }
}

const binDir = join(packageDir, 'bin');
if (existsSync(binDir)) {
  rmSync(binDir, { recursive: true, force: true });
}

console.log('Universal wrapper prepared without an embedded platform binary.');
