import { test } from 'node:test';
import assert from 'node:assert/strict';
import { connections } from '../src/data/sample.ts';
import {
  connectionSnapshot,
  loadConnectionPreferences,
  CONNECTION_STORAGE_KEY,
} from '../src/services/connectionPreferences.ts';
test('legacy connection arrays migrate to sensible independent tags/colors and Ungrouped', () => {
  const legacy = connections.map((c) => ({
    ...c,
    password: 'db-secret',
    transport: {
      type: 'ssh',
      host: 'bastion',
      port: 22,
      user: 'dev',
      auth: 'private-key',
      privateKey: 'key-secret',
      passphrase: 'passphrase-secret',
      password: 'ssh-secret',
    },
  }));
  const migrated = loadConnectionPreferences(
    { getItem: () => JSON.stringify(legacy) },
    connections,
  );
  assert.equal(migrated.error, undefined);
  assert.equal(migrated.connections.length, 2);
  assert.equal(migrated.connections[0]!.environment, 'Local');
  assert.equal(migrated.connections[1]!.environment, 'Staging');
  assert.equal(migrated.connections[0]!.groupId, '');
  const serialized = JSON.stringify(migrated);
  for (const secret of [
    'db-secret',
    'key-secret',
    'passphrase-secret',
    'ssh-secret',
    'privateKey',
    'passphrase',
  ])
    assert.ok(!serialized.includes(secret));
  assert.equal(migrated.connections[0]!.transport.type, 'ssh');
});
test('groups, colors, assignments and explicit ordering survive a serialization round trip', () => {
  const groups = [
    { id: 'b', name: 'Billing', collapsed: true },
    { id: 'a', name: 'Admin', collapsed: false },
  ];
  const list = connections
    .map((c, i) => ({
      ...c,
      groupId: i ? 'a' : 'b',
      color: i ? '#8e7ba1' : '#557fa6',
      environment: 'Production' as const,
    }))
    .reverse();
  const saved = connectionSnapshot(list, groups);
  const restored = loadConnectionPreferences(
    { getItem: (key) => (key === CONNECTION_STORAGE_KEY ? JSON.stringify(saved) : null) },
    [],
  );
  assert.deepEqual(restored, saved);
  assert.equal(restored.connections[0]!.id, 'staging');
  assert.equal(restored.groups[0]!.collapsed, true);
});
test('missing groups migrate to Ungrouped without dropping connections', () => {
  const value = { connections: [{ ...connections[0], groupId: 'missing' }], groups: [] };
  const restored = loadConnectionPreferences({ getItem: () => JSON.stringify(value) }, []);
  assert.equal(restored.connections.length, 1);
  assert.equal(restored.connections[0]!.groupId, '');
});
test('malformed saved preferences surface a recovery error and retain sample connections', () => {
  const restored = loadConnectionPreferences({ getItem: () => '{bad' }, connections);
  assert.match(restored.error!, /could not be loaded/);
  assert.equal(restored.connections.length, 2);
});
