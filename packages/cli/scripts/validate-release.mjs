#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = join(scriptDir, '..', '..', '..');
const tag = process.argv[2] ?? '';

if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
  console.error(`Release tag must be a semantic version prefixed with v: ${tag}`);
  process.exit(1);
}

const version = tag.slice(1);
const readJson = (path) => JSON.parse(readFileSync(path, 'utf8'));
const fail = (message) => {
  console.error(message);
  process.exit(1);
};

const wrapperPath = join(repositoryRoot, 'packages', 'cli', 'package.json');
const wrapper = readJson(wrapperPath);
const cargoMetadata = spawnSync(
  'cargo',
  ['metadata', '--no-deps', '--format-version', '1'],
  { cwd: repositoryRoot, encoding: 'utf8' }
);
if (cargoMetadata.status !== 0) {
  fail(cargoMetadata.stderr.trim() || 'Unable to read Cargo metadata');
}
const cliCrate = JSON.parse(cargoMetadata.stdout).packages.find(
  ({ name }) => name === 'orquestra-cli'
);
if (!cliCrate) {
  fail('Cargo metadata does not contain orquestra-cli');
}
if (cliCrate.version !== version) {
  fail(`Rust CLI version ${cliCrate.version} does not match tag ${tag}`);
}

const platforms = [
  {
    directory: 'cli-platform-win32-x64',
    name: '@jonathlima/orquestra-win32-x64',
    os: 'win32',
    cpu: 'x64',
  },
  {
    directory: 'cli-platform-darwin-x64',
    name: '@jonathlima/orquestra-darwin-x64',
    os: 'darwin',
    cpu: 'x64',
  },
  {
    directory: 'cli-platform-darwin-arm64',
    name: '@jonathlima/orquestra-darwin-arm64',
    os: 'darwin',
    cpu: 'arm64',
  },
  {
    directory: 'cli-platform-linux-x64',
    name: '@jonathlima/orquestra-linux-x64',
    os: 'linux',
    cpu: 'x64',
  },
  {
    directory: 'cli-platform-linux-arm64',
    name: '@jonathlima/orquestra-linux-arm64',
    os: 'linux',
    cpu: 'arm64',
  },
];

if (wrapper.name !== '@jonathlima/orquestra') {
  fail(`Unexpected wrapper name: ${wrapper.name}`);
}
if (wrapper.version !== version) {
  fail(`Wrapper version ${wrapper.version} does not match tag ${tag}`);
}
if (wrapper.license !== 'AGPL-3.0-only') {
  fail(`Unexpected wrapper license: ${wrapper.license}`);
}
if (wrapper.bin?.orquestra !== 'index.cjs') {
  fail('Wrapper must expose the orquestra executable through index.cjs');
}

const expectedOptional = platforms.map(({ name }) => name).sort();
const actualOptional = Object.keys(wrapper.optionalDependencies ?? {}).sort();
if (JSON.stringify(actualOptional) !== JSON.stringify(expectedOptional)) {
  fail('Wrapper optionalDependencies do not match the platform inventory');
}

for (const platform of platforms) {
  if (wrapper.optionalDependencies[platform.name] !== version) {
    fail(`${platform.name} must use wrapper version ${version}`);
  }

  const manifest = readJson(
    join(repositoryRoot, 'packages', platform.directory, 'package.json')
  );
  if (manifest.name !== platform.name) {
    fail(`${platform.directory} has unexpected name ${manifest.name}`);
  }
  if (manifest.version !== version) {
    fail(`${platform.name} version ${manifest.version} does not match ${version}`);
  }
  if (manifest.license !== 'AGPL-3.0-only') {
    fail(`${platform.name} has unexpected license ${manifest.license}`);
  }
  if (manifest.bin) {
    fail(`${platform.name} must not expose a competing executable`);
  }
  if (!manifest.os?.includes(platform.os) || !manifest.cpu?.includes(platform.cpu)) {
    fail(`${platform.name} has invalid os/cpu restrictions`);
  }
  for (const required of ['bin/', 'verify.js', 'LICENSE']) {
    if (!manifest.files?.includes(required)) {
      fail(`${platform.name} does not publish ${required}`);
    }
  }
}

console.log(`Release ${tag} metadata validated for wrapper and five platforms.`);
