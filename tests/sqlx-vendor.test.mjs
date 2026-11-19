import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,cpSync,copyFileSync,appendFileSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
const files=[
  'tools/check-sqlx-vendor.mjs',
  'experiments/sqlx-receive-limit/upstream.json',
  'experiments/sqlx-receive-limit/receive-limit.patch',
  'experiments/sqlx-metadata-limit/metadata-limit.patch',
  'experiments/sqlx-rsa-key/rsa-key.patch',
  'experiments/sqlx-pg-message-limit/upstream.json',
  'experiments/sqlx-pg-message-limit/message-limit.patch',
];
// Each drift names the driver whose reviewed provenance it breaks.
const drifts={
  'mysql source':['sqlx-mysql',(temp)=>appendFileSync(join(temp,'vendor/sqlx-mysql/src/connection/stream.rs'),'\n// synthetic unreviewed change\n')],
  'mysql provenance':['sqlx-mysql',(temp)=>{const path=join(temp,'experiments/sqlx-receive-limit/upstream.json');const metadata=JSON.parse(readFileSync(path,'utf8'));metadata.patchSha256='0'.repeat(64);writeFileSync(path,JSON.stringify(metadata));}],
  'mysql receive-limit patch':['sqlx-mysql',(temp)=>appendFileSync(join(temp,'experiments/sqlx-receive-limit/receive-limit.patch'),'\n# synthetic drift\n')],
  'mysql metadata-limit patch':['sqlx-mysql',(temp)=>appendFileSync(join(temp,'experiments/sqlx-metadata-limit/metadata-limit.patch'),'\n# synthetic drift\n')],
  'mysql rsa-key patch':['sqlx-mysql',(temp)=>appendFileSync(join(temp,'experiments/sqlx-rsa-key/rsa-key.patch'),'\n# synthetic drift\n')],
  'postgres source':['sqlx-postgres',(temp)=>appendFileSync(join(temp,'vendor/sqlx-postgres/src/connection/stream.rs'),'\n// synthetic unreviewed change\n')],
  'postgres message-limit patch':['sqlx-postgres',(temp)=>appendFileSync(join(temp,'experiments/sqlx-pg-message-limit/message-limit.patch'),'\n# synthetic drift\n')],
};
for(const [kind,[driver,drift]] of Object.entries(drifts)) test(`vendored SQLx check accepts reviewed source and rejects ${kind} drift`,()=>{
  const temp=mkdtempSync(join(tmpdir(),'stoneql-vendor-check-'));
  try {
    for(const name of ['sqlx-mysql','sqlx-postgres'])cpSync(resolve(import.meta.dirname,'../vendor',name),join(temp,'vendor',name),{recursive:true});
    for(const file of files){mkdirSync(dirname(join(temp,file)),{recursive:true});copyFileSync(resolve(import.meta.dirname,'..',file),join(temp,file));}
    const run=()=>spawnSync(process.execPath,[join(temp,'tools/check-sqlx-vendor.mjs')],{encoding:'utf8'});
    assert.equal(run().status,0);
    drift(temp);
    const failed=run();assert.equal(failed.status,1);
    assert.match(failed.stderr,new RegExp(`${driver} (source changed|provenance drift)`));
  }finally{rmSync(temp,{recursive:true,force:true});}
});
