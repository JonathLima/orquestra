#!/usr/bin/env node
const { spawnSync } = require('child_process');
const path = require('path');
const fs = require('fs');

const platformPackages = {
  'win32-x64': '@jonathlima/orquestra-win32-x64',
  'darwin-x64': '@jonathlima/orquestra-darwin-x64',
  'darwin-arm64': '@jonathlima/orquestra-darwin-arm64',
  'linux-x64': '@jonathlima/orquestra-linux-x64',
  'linux-arm64': '@jonathlima/orquestra-linux-arm64',
};

function findBinary() {
  const bin = process.platform === 'win32' ? 'orquestra-cli.exe' : 'orquestra-cli';
  const localBinary = path.join(__dirname, 'bin', bin);
  if (fs.existsSync(localBinary)) {
    return localBinary;
  }

  const key = `${process.platform}-${process.arch}`;
  const packageName = platformPackages[key];
  if (!packageName) {
    return null;
  }

  try {
    const manifest = require.resolve(`${packageName}/package.json`, {
      paths: [__dirname],
    });
    const packagedBinary = path.join(path.dirname(manifest), 'bin', bin);
    return fs.existsSync(packagedBinary) ? packagedBinary : null;
  } catch {
    return null;
  }
}

const binary = findBinary();
if (!binary) {
  const key = `${process.platform}-${process.arch}`;
  const packageName = platformPackages[key];
  const hint = packageName
    ? `Reinstall @jonathlima/orquestra or install ${packageName}.`
    : `No prebuilt package supports ${key}.`;
  console.error(`Orquestra CLI binary not found. ${hint}`);
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
if (result.error) {
  console.error(`Failed to start Orquestra: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
