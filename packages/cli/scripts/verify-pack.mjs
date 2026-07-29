#!/usr/bin/env node
import { spawnSync } from 'node:child_process';

const pack =
  process.platform === 'win32'
    ? spawnSync(process.env.ComSpec ?? 'cmd.exe', ['/d', '/s', '/c', 'npm pack --json --dry-run'], {
        encoding: 'utf8',
      })
    : spawnSync('npm', ['pack', '--json', '--dry-run'], {
        encoding: 'utf8',
      });

if (pack.error) {
  console.error(pack.error.message);
  process.exit(1);
}

if (pack.status !== 0) {
  process.stderr.write(pack.stderr ?? '');
  process.exit(pack.status ?? 1);
}

const packages = JSON.parse(pack.stdout);
const files = packages.flatMap((pkg) => pkg.files?.map((file) => file.path) ?? []);
const binary = process.platform === 'win32' ? 'bin/orquestra-cli.exe' : 'bin/orquestra-cli';

if (!files.includes(binary)) {
  console.error(`Missing packaged Rust binary: ${binary}`);
  process.exit(1);
}

console.log(pack.stdout.trim());
