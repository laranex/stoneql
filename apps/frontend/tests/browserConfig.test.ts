import { test } from 'node:test';
import assert from 'node:assert/strict';
import config from '../playwright.config.ts';

test('browser verification requires sandboxed Chromium and contains no security bypass flags', () => {
  const options = config.use?.launchOptions;
  assert.equal(options?.chromiumSandbox, true);
  assert.ok(!options?.args?.some(argument => /no-sandbox|disable-web-security|disable-setuid-sandbox/.test(argument)));
});
