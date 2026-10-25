import test from 'node:test';
import assert from 'node:assert/strict';
import { migrateLegacyPreferences } from '../src/services/brandingMigration.ts';

function storage(entries: Record<string, string>) {
  const values = new Map(Object.entries(entries));
  return { values, getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
}
test('StoneQL migrates dbAdmin preferences without copying secrets or replacing a manual preference', () => {
  const saved = storage({
    'dbadmin-theme': 'dark', 'dbadmin-default-page-size': '50',
    'dbadmin-connections': JSON.stringify({version:1,groups:[{id:'g',name:'Project'}],connections:[{id:'c',name:'Existing',engine:'MySQL',password:'secret',rootCertificatePem:'certificate',groupId:'g',transport:{type:'ssh',host:'bastion',password:'ssh-secret',privateKey:'key',passphrase:'phrase'}}]}),
    'dbadmin-grid-widths:v1': JSON.stringify({scope:{id:150,secret:'never-copy'}}),
  });
  migrateLegacyPreferences(saved);
  assert.equal(saved.getItem('stoneql-theme'),'dark');
  assert.equal(saved.getItem('stoneql-default-page-size'),'50');
  const profile=JSON.parse(saved.getItem('stoneql-connections')!);
  assert.equal(profile.connections[0].name,'Existing');
  assert.equal(profile.connections[0].groupId,'g');
  for(const secret of ['secret','certificate','privateKey','passphrase'])assert.ok(!JSON.stringify(profile).includes(secret));
  assert.deepEqual(JSON.parse(saved.getItem('stoneql-grid-widths:v1')!),{scope:{id:150}});
  saved.setItem('stoneql-theme','light');migrateLegacyPreferences(saved);
  assert.equal(saved.getItem('stoneql-theme'),'light');
  assert.equal(saved.getItem('dbadmin-theme'),'dark');
});
test('malformed legacy preferences and blocked storage do not prevent startup', () => {
  const saved=storage({'dbadmin-theme':'purple','dbadmin-default-page-size':'NaN','dbadmin-connections':'{','dbadmin-grid-widths:v1':'null'});
  migrateLegacyPreferences(saved);
  assert.equal([...saved.values.keys()].filter(key=>key.startsWith('stoneql-')).length,0);
  assert.doesNotThrow(()=>migrateLegacyPreferences({getItem(){throw Error('blocked');},setItem(){throw Error('blocked');}}));
});

test('all existing page-size choices survive the branding migration', () => {
  for (const size of ['15', '25', '50', '100', '250', '500']) {
    const saved=storage({'dbadmin-default-page-size':size});
    migrateLegacyPreferences(saved);
    assert.equal(saved.getItem('stoneql-default-page-size'),size);
  }
});
