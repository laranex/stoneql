import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,cpSync,copyFileSync,appendFileSync,readFileSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
for(const kind of ['source','provenance','patch']) test(`vendored SQLx check accepts reviewed source and rejects ${kind} drift`,()=>{
  const temp=mkdtempSync(join(tmpdir(),'stoneql-vendor-check-'));
  try {
    mkdirSync(join(temp,'tools'));
    mkdirSync(join(temp,'experiments/sqlx-receive-limit'),{recursive:true});
    cpSync(resolve(import.meta.dirname,'../vendor/sqlx-mysql'),join(temp,'vendor/sqlx-mysql'),{recursive:true});
    copyFileSync(resolve(import.meta.dirname,'../tools/check-sqlx-vendor.mjs'),join(temp,'tools/check-sqlx-vendor.mjs'));
    copyFileSync(resolve(import.meta.dirname,'../experiments/sqlx-receive-limit/upstream.json'),join(temp,'experiments/sqlx-receive-limit/upstream.json'));
    copyFileSync(resolve(import.meta.dirname,'../experiments/sqlx-receive-limit/receive-limit.patch'),join(temp,'experiments/sqlx-receive-limit/receive-limit.patch'));
    const run=()=>spawnSync(process.execPath,[join(temp,'tools/check-sqlx-vendor.mjs')],{encoding:'utf8'});
    assert.equal(run().status,0);
    if(kind==='source')appendFileSync(join(temp,'vendor/sqlx-mysql/src/connection/stream.rs'),'\n// synthetic unreviewed change\n');
    else if(kind==='patch')appendFileSync(join(temp,'experiments/sqlx-receive-limit/receive-limit.patch'),'\n# synthetic drift\n');
    else {
      const path=join(temp,'experiments/sqlx-receive-limit/upstream.json');
      const metadata=JSON.parse(readFileSync(path,'utf8'));metadata.patchSha256='0'.repeat(64);
      writeFileSync(path,JSON.stringify(metadata));
    }
    const failed=run();assert.equal(failed.status,1);assert.match(failed.stderr,/SQLx (source changed|provenance drift)/);
  }finally{rmSync(temp,{recursive:true,force:true});}
});
