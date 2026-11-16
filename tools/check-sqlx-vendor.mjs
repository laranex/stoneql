import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve, join, relative } from 'node:path';
const root = resolve(import.meta.dirname, '..');
const sha256 = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');

// Each vendored SQLx driver is the registry archive plus reviewed patches. Its provenance must
// match the approved record, every patch file must match its recorded hash, and every source
// file must match the reviewed file hashes.
function verify({ name, approvedRecord, patches }) {
  const vendor = join(root, 'vendor', name);
  const provenance = JSON.parse(readFileSync(join(vendor, 'dbadmin-provenance.json'), 'utf8'));
  const approved = JSON.parse(readFileSync(join(root, approvedRecord), 'utf8'));
  for (const key of ['crate', 'version', 'registryPackageSha256', 'patchSha256', 'mitLicenseSha256']) {
    if (provenance[key] !== approved[key]) throw Error(`${name} provenance drift: ${key}`);
  }
  for (const [file, key, record] of patches) {
    if (sha256(join(root, file)) !== record(approved, provenance)) throw Error(`${name} provenance drift: ${key} content`);
  }
  const actual = {};
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile() && entry.name !== 'dbadmin-provenance.json') actual[relative(vendor, path).replaceAll('\\', '/')] = sha256(path);
      else if (!entry.isFile()) throw Error(`Unexpected file type in ${name} source`);
    }
  };
  visit(vendor);
  if (Object.keys(actual).length !== Object.keys(provenance.files).length || Object.entries(actual).some(([file, hash]) => provenance.files[file] !== hash)) {
    throw Error(`${name} source changed; review patch and provenance before adoption`);
  }
  console.log(`${name} source provenance verified (${Object.keys(actual).length} files).`);
}

verify({
  name: 'sqlx-mysql',
  approvedRecord: 'experiments/sqlx-receive-limit/upstream.json',
  patches: [
    ['experiments/sqlx-receive-limit/receive-limit.patch', 'patch', (approved) => approved.patchSha256],
    // Second reviewed delta (A6, 2026-11-16): result-metadata column/byte limit.
    ['experiments/sqlx-metadata-limit/metadata-limit.patch', 'metadata-limit patch', (_, provenance) => provenance.metadataLimitPatchSha256],
  ],
});
verify({
  name: 'sqlx-postgres',
  approvedRecord: 'experiments/sqlx-pg-message-limit/upstream.json',
  // A8 (2026-11-16): backend message length check and limit.
  patches: [['experiments/sqlx-pg-message-limit/message-limit.patch', 'patch', (approved) => approved.patchSha256]],
});
