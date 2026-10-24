import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir, stat } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { siteBase, sidebar, repositoryUrl } from '../.website/site.mjs';
const json = async path => JSON.parse(await readFile(new URL(`../${path}`, import.meta.url), 'utf8'));

test('root commands delegate to the owning npm workspace and lockfile agrees', async () => {
  const root = await json('package.json');
  const lock = await json('package-lock.json');
  assert.deepEqual(root.workspaces, ['apps/frontend', '.website']);
  assert.equal(root.engines.node, '26.x');
  for (const path of root.workspaces) {
    const pkg = await json(`${path}/package.json`);
    for (const field of ['dependencies', 'devDependencies']) assert.deepEqual(lock.packages[path][field], pkg[field]);
    assert.equal(lock.packages[`node_modules/${pkg.name}`].resolved, path);
  }
  for (const script of ['dev', 'build', 'preview', 'test:browser']) assert.match(root.scripts[script], /--workspace @dbadmin\/frontend/);
  for (const script of ['docs:dev', 'docs:build', 'docs:preview']) assert.match(root.scripts[script], /--workspace @dbadmin\/docs/);
});

test('documentation supports repository bases and rejects unsafe or ambiguous paths', () => {
  assert.equal(siteBase('/dbadmin/'), '/dbadmin/');
  assert.equal(siteBase('/'), '/');
  for (const value of ['dbadmin', '//host/', '/a/../', '/a?b/', '/a']) assert.throws(() => siteBase(value));
});

test('navigation resolves canonical docs and website does not mirror Markdown', async () => {
  for (const group of sidebar) for (const item of group.items) {
    const path = item.link.endsWith('/') ? `${item.link}index.md` : `${item.link}.md`;
    assert.ok((await stat(new URL(`../docs${path}`, import.meta.url))).isFile());
  }
  const config = await readFile(new URL('../.website/.vitepress/config.mts', import.meta.url), 'utf8');
  assert.match(config, /srcDir:.*\.\.\/\.\.\/docs\//);
  const entries = await readdir(new URL('../.website/', import.meta.url));
  assert.ok(!entries.some(name => name.endsWith('.md')));
});

test('local progress is ignored and never tracked', () => {
  assert.match(execFileSync('git', ['check-ignore', '.ref/workprogress.md'], { encoding: 'utf8' }), /\.ref\/workprogress\.md/);
  assert.equal(execFileSync('git', ['ls-files', '--', '.ref/workprogress.md'], { encoding: 'utf8' }), '');
});

test('frontend remains native-framework independent and no Rust placeholders exist', async () => {
  const pkg = await json('apps/frontend/package.json');
  assert.ok(!Object.keys({ ...pkg.dependencies, ...pkg.devDependencies }).some(name => /tauri/.test(name)));
  for (const path of ['apps/desktop', 'apps/server', 'crates', 'Cargo.toml']) {
    await assert.rejects(stat(new URL(`../${path}`, import.meta.url)), { code: 'ENOENT' });
  }
});

test('repository links are explicit and do not invent an owner', () => {
  const previous = { url: process.env.DOCS_REPOSITORY_URL, repository: process.env.GITHUB_REPOSITORY };
  try {
    delete process.env.DOCS_REPOSITORY_URL;
    delete process.env.GITHUB_REPOSITORY;
    assert.equal(repositoryUrl(), '');
    process.env.GITHUB_REPOSITORY = 'owner/dbadmin';
    assert.equal(repositoryUrl(), 'https://github.com/owner/dbadmin');
    process.env.DOCS_REPOSITORY_URL = 'https://user:secret@github.com/owner/dbadmin';
    assert.throws(repositoryUrl);
  } finally {
    for (const [key, value] of [['DOCS_REPOSITORY_URL', previous.url], ['GITHUB_REPOSITORY', previous.repository]]) {
      if (value === undefined) delete process.env[key]; else process.env[key] = value;
    }
  }
});
