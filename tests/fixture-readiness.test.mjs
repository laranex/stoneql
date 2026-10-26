import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,copyFileSync,writeFileSync,readFileSync,rmSync,readdirSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve,dirname} from 'node:path';
import {spawnSync} from 'node:child_process';

test('lifecycle fixture waits for final TCP startup after the init-server marker becomes visible',()=>{
  const temp=mkdtempSync(join(tmpdir(),'stoneql-readiness-test-'));
  const workspace=join(temp,'workspace');const scratch=join(temp,'scratch');
  mkdirSync(join(workspace,'tools'),{recursive:true});mkdirSync(scratch);
  try {
    for(const name of ['test-connection-lifecycle.mjs','fixture-environment.mjs', 'resource-report.mjs'])copyFileSync(resolve(import.meta.dirname,'../tools',name),join(workspace,'tools',name));
    const prefix=`#!${process.execPath}\nimport * as fs from 'node:fs';\nimport {join} from 'node:path';\nconst root=process.cwd();const args=process.argv.slice(2);\n`;
    writeFileSync(join(temp,'cargo'),prefix+`
fs.mkdirSync(join(root,'target/debug/examples'),{recursive:true});
fs.writeFileSync(join(root,'target/debug/examples/lifecycle'),${JSON.stringify(`#!${process.execPath}
import * as fs from 'node:fs';
if(!fs.existsSync('tcp-ready')){console.error('probe started before TCP readiness');process.exit(44);}
fs.appendFileSync('executed-probes','probe'+String.fromCharCode(10));
console.log(JSON.stringify(process.argv.includes('--expired-certificate')?{expiredCertificateRejected:true,runtimeRejected:true}:{lifecycleSamples:100}));
`)},{mode:0o755});
`,{mode:0o755});
    writeFileSync(join(temp,'openssl'),prefix+`for(const flag of ['-out','-keyout']){const index=args.indexOf(flag);if(index>=0)fs.writeFileSync(args[index+1],'synthetic fixture material');}`,{mode:0o755});
    writeFileSync(join(temp,'docker'),prefix+`
if(args[0]==='context'){console.log('unix:///synthetic-daemon');}
else if(args.includes('up')){fs.rmSync('tcp-ready',{force:true});fs.writeFileSync('tcp-attempts','0');}
else if(args.includes('exec')){
  if(args.at(-1).includes('/dev/tcp/')){
    const count=Number(fs.readFileSync('tcp-attempts','utf8'))+1;fs.writeFileSync('tcp-attempts',String(count));
    if(count===1)process.exit(1);fs.writeFileSync('tcp-ready','yes');
  }else console.log(process.env.STONEQL_SPIKE_FIXTURE_ID);
}
else if(args.includes('port'))console.log('127.0.0.1:12345');
else if(args.includes('--format')&&args.includes('json'))console.log(JSON.stringify({State:'running'}));
`,{mode:0o755});
    const result=spawnSync(process.execPath,[join(workspace,'tools/test-connection-lifecycle.mjs')],{
      cwd:workspace,encoding:'utf8',timeout:15000,env:{...process.env,PATH:`${temp}:${dirname(process.execPath)}:/usr/bin:/bin`,TMPDIR:scratch,DOCKER_HOST:'unix:///synthetic-local',DOCKER_CONTEXT:''},
    });
    assert.equal(result.status,0,result.stderr);
    assert.equal(readFileSync(join(workspace,'executed-probes'),'utf8').trim().split('\n').length,4);
    assert.deepEqual(readdirSync(scratch), []);
  }finally{rmSync(temp,{recursive:true,force:true});}
});
