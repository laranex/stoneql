// Prepare an isolated, checksum-verified upstream copy. Never patches Cargo's cache or root workspace.
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync, mkdirSync, mkdtempSync, cpSync, readdirSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { resolve, dirname, join, relative } from 'node:path';
const root = resolve(import.meta.dirname, '..');
const evaluation = join(root, 'experiments/sqlx-receive-limit');
const driver = join(evaluation, 'driver');
const upstream = JSON.parse(readFileSync(join(evaluation, 'upstream.json'), 'utf8'));
const digest = data => createHash('sha256').update(data).digest('hex');
const patch = join(evaluation, 'receive-limit.patch');
const license = readFileSync(join(evaluation, 'SQLX-LICENSE-MIT'));
if (digest(license) !== upstream.mitLicenseSha256) throw Error('Upstream license checksum mismatch');
if (digest(readFileSync(patch)) !== upstream.patchSha256) throw Error('Evaluation patch checksum mismatch');
function hashes(directory) {
  const result = {};
  function visit(path) {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const name = join(path, entry.name);
      if (entry.isDirectory()) visit(name);
      else if (entry.isFile() && entry.name !== '.prepared.json') result[relative(directory, name)] = digest(readFileSync(name));
      else if (!entry.isFile()) throw Error('Unexpected file type in prepared driver');
    }
  }
  visit(directory);
  return result;
}
if (existsSync(driver)) {
  const prepared = JSON.parse(readFileSync(join(driver, '.prepared.json'), 'utf8'));
  if (prepared.patchSha256 !== upstream.patchSha256 || JSON.stringify(prepared.files) !== JSON.stringify(hashes(driver))) {
    throw Error('Prepared driver differs; refusing to overwrite local changes. Review the ignored evaluation copy.');
  }
  console.log('Prepared evaluation driver verified.');
} else {
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--format-version', '1'], { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 }));
  const pkg = metadata.packages.find(pkg => pkg.name === upstream.crate && pkg.version === upstream.version && pkg.source?.startsWith('registry+'));
  if (!pkg) throw Error('Pinned registry driver was not resolved');
  const source = dirname(pkg.manifest_path);
  const registry = dirname(dirname(source));
  const archive = join(dirname(registry), 'cache', dirname(source).split('/').at(-1), `${upstream.crate}-${upstream.version}.crate`);
  if (digest(readFileSync(archive)) !== upstream.registryPackageSha256) throw Error('Upstream archive checksum mismatch');
  const temporary = mkdtempSync(join(tmpdir(), 'dbadmin-receive-limit-'));
  try {
    // The exact known registry archive is verified before extraction.
    execFileSync('tar', ['-xf', archive, '-C', temporary]);
    const extracted = join(temporary, `${upstream.crate}-${upstream.version}`);
    mkdirSync(driver);
    for (const name of ['src', 'Cargo.toml', 'Cargo.toml.orig', 'LICENSE-MIT', 'LICENSE-APACHE']) cpSync(join(extracted, name), join(driver, name), { recursive: true, errorOnExist: true });
    execFileSync('patch', ['-p1', '-i', patch], { cwd: driver, stdio: 'pipe' });
    // The published subcrate contains relative license placeholders; retain the complete MIT grant.
    writeFileSync(join(driver, 'LICENSE-MIT'), license);
    writeFileSync(join(driver, '.prepared.json'), JSON.stringify({ patchSha256: upstream.patchSha256, files: hashes(driver) }, null, 2) + '\n');
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
  console.log('Isolated evaluation driver prepared; root driver unchanged.');
}
