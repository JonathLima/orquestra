const fs = require('fs');
const crypto = require('crypto');
const path = require('path');

const binPath = path.join(__dirname, 'bin', 'orquestra-cli');
const shaPath = binPath + '.sha256';
const sigPath = binPath + '.sigstore.json';

if (!fs.existsSync(binPath)) {
  console.error('Binary not found at', binPath);
  process.exit(1);
}

if (!fs.existsSync(shaPath)) {
  console.error('SHA256 sidecar not found at', shaPath);
  process.exit(1);
}

const expected = fs.readFileSync(shaPath, 'utf-8').trim().split(/\s+/)[0];
const actual = crypto.createHash('sha256').update(fs.readFileSync(binPath)).digest('hex');
if (expected !== actual) {
  console.error('SHA256 mismatch');
  process.exit(1);
}
console.log('SHA256 OK');

if (!binPath.endsWith('.exe') && (fs.statSync(binPath).mode & 0o111) === 0) {
  console.error('Binary is not executable');
  process.exit(1);
}

if (!fs.existsSync(sigPath)) {
  console.error('Sigstore bundle not found at', sigPath);
  process.exit(1);
}

const { spawnSync } = require('child_process');
const result = spawnSync(
  'cosign',
  [
    'verify-blob',
    '--bundle',
    sigPath,
    '--certificate-identity-regexp',
    '^https://github\\.com/JonathLima/orquestra/\\.github/workflows/release\\.yml@refs/tags/v.*$',
    '--certificate-oidc-issuer',
    'https://token.actions.githubusercontent.com',
    binPath,
  ],
  { stdio: 'ignore' }
);
if (result.error?.code === 'ENOENT') {
  console.warn('Cosign verification skipped (cosign not available)');
} else if (result.status !== 0) {
  console.error('Cosign signature verification failed');
  process.exit(1);
} else {
  console.log('Cosign signature OK');
}
