// One-time migration baseline; intentionally separate from tests for future frontend changes.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const json = async path => JSON.parse(await readFile(new URL(`../../${path}`, import.meta.url), 'utf8'));

test('migration preserves original frontend source, tests, assets and configuration', async () => {
  const manifest = await json('.ref/planning/frontend-preservation.json');
  for (const [path, expected] of Object.entries(manifest.files)) {
    const bytes = await readFile(new URL(`../../apps/frontend/${path}`, import.meta.url));
    assert.equal(createHash('sha256').update(bytes).digest('hex'), expected, path);
  }
});
