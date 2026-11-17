import test from 'node:test';
import assert from 'node:assert/strict';
import { bootstrapNativeHost, nativeDatabase, onSecretStoreLocked, NativeOperationError } from '../src/services/nativeHost.ts';
import { masterPasswordProblem, referenceStore, secretStoreNote, savedPlaceholder } from '../src/services/secretStore.ts';

function fixture() {
  const calls: Array<{ command: string; payload: any }> = [];
  let unlocked = false;
  const global = { __TAURI__: { core: { async invoke(command: string, args: { message: string }) {
    const request = JSON.parse(args.message); calls.push({ command, payload: request.payload });
    let payload: unknown;
    if (command === 'host_info') payload = { runtime: 'desktop', appVersion: '0.1.0', sessionId: 's', nonce: request.payload.nonce, simulated: true };
    else if (command === 'contract_probe') payload = { values: request.payload.values, simulated: true };
    else if (command === 'unlock_secret_store') { unlocked = true; payload = { kind: 'encrypted_file', vaultExists: true, unlocked }; }
    else if (command === 'store_secret') { if (!unlocked) throw { code: 'SECRET_STORE_LOCKED' }; payload = { reference: '00000000-0000-8000-8000-000000000001' }; }
    else throw { code: 'INVALID_REQUEST' };
    return { version: 1, requestId: request.requestId, payload };
  } } } };
  return { calls, global, unlock: () => nativeDatabase('unlock_secret_store', { password: 'synthetic master', create: true }) };
}

test('a locked encrypted file asks for the master password once, then retries the same request', async () => {
  const f = fixture(); await bootstrapNativeHost(f.global);
  let asked = 0;
  onSecretStoreLocked(async () => { asked++; await f.unlock(); return true; });
  const saved = await nativeDatabase<{ reference: string }>('store_secret', { purpose: 'database_password', secret: 'synthetic' });
  assert.equal(saved.reference, '00000000-0000-8000-8000-000000000001');
  assert.equal(asked, 1);
  assert.deepEqual(f.calls.filter((c) => c.command === 'store_secret').map((c) => c.payload.secret), ['synthetic', 'synthetic']);
  onSecretStoreLocked(undefined); await bootstrapNativeHost({});
});

test('a cancelled master-password prompt leaves the request refused and never retries', async () => {
  const f = fixture(); await bootstrapNativeHost(f.global);
  onSecretStoreLocked(async () => false);
  await assert.rejects(nativeDatabase('store_secret', { secret: 'synthetic' }), (error) => error instanceof NativeOperationError && error.code === 'SECRET_STORE_LOCKED' && /master password/.test(error.message));
  assert.equal(f.calls.filter((c) => c.command === 'store_secret').length, 1);
  onSecretStoreLocked(undefined); await bootstrapNativeHost({});
});

test('master password rules: entered, at least 8 characters and repeated when creating', () => {
  assert.equal(masterPasswordProblem('', '', false), 'Enter the master password.');
  assert.equal(masterPasswordProblem('short', '', false), '', 'unlocking checks only the host');
  assert.equal(masterPasswordProblem('sevench', 'sevench', true), 'Use at least 8 characters.');
  assert.equal(masterPasswordProblem('eight ch', 'eight cx', true), 'The two entries do not match.');
  assert.equal(masterPasswordProblem('eight ch', 'eight ch', true), '');
  // Counted in characters, not UTF-16 units.
  assert.equal(masterPasswordProblem('🔑'.repeat(7), '🔑'.repeat(7), true), 'Use at least 8 characters.');
});

test('the connection dialog names the store that new passwords go to', () => {
  assert.equal(secretStoreNote('keychain'), 'New passwords are saved in the macOS Keychain.');
  assert.equal(secretStoreNote('secret_service'), 'New passwords are saved in the desktop keyring (Secret Service).');
  assert.match(secretStoreNote('encrypted_file'), /^New passwords are saved in StoneQL’s encrypted file \(master password\)/);
  assert.equal(secretStoreNote('unavailable'), 'Passwords cannot be saved on this system.');
  assert.equal(secretStoreNote(undefined), '');
});

const V4 = '0f8e3d2c-1b4a-4c5d-9e6f-7a8b9c0d1e2f', V8 = '0f8e3d2c-1b4a-8c5d-9e6f-7a8b9c0d1e2f';
test('an existing reference is placed in the store its UUID version names, not where new saves go', () => {
  // macOS: version-4 references are Keychain items.
  assert.equal(referenceStore(V4, 'keychain'), 'keychain');
  // Linux: version 4 is the Secret Service and version 8 the encrypted file, whichever store is current.
  for (const kind of ['secret_service', 'encrypted_file'] as const) {
    assert.equal(referenceStore(V4, kind), 'secret_service');
    assert.equal(referenceStore(V8, kind), 'encrypted_file');
  }
  assert.equal(referenceStore(V8.toUpperCase(), undefined), 'encrypted_file', 'version 8 is only made by the encrypted file');
  // Unknown: no status (browser, Windows, a failed status call) or not a version-4/8 UUID.
  assert.equal(referenceStore(V4, undefined), undefined);
  assert.equal(referenceStore(V4, 'unavailable'), undefined);
  assert.equal(referenceStore('0f8e3d2c-1b4a-7c5d-9e6f-7a8b9c0d1e2f', 'secret_service'), undefined);
  assert.equal(referenceStore('not-a-reference', 'keychain'), undefined);
  assert.equal(referenceStore(undefined, 'keychain'), undefined);
});

test('the saved-password placeholder names the store holding that password', () => {
  assert.equal(savedPlaceholder(V4, 'keychain'), 'Saved in Keychain');
  assert.equal(savedPlaceholder(V4, 'secret_service'), 'Saved in keyring');
  assert.equal(savedPlaceholder(V4, 'encrypted_file'), 'Saved in keyring', 'saved before the keyring stopped running');
  assert.equal(savedPlaceholder(V8, 'secret_service'), 'Saved in encrypted file', 'saved before a keyring was running');
  assert.equal(savedPlaceholder(V8, 'encrypted_file'), 'Saved in encrypted file');
  assert.equal(savedPlaceholder(V4, undefined), 'Saved password');
  assert.equal(savedPlaceholder(V4, 'unavailable'), 'Saved password');
});
