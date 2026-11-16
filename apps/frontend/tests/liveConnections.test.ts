import test from 'node:test';
import assert from 'node:assert/strict';
import {bootstrapNativeHost,NativeOperationError} from '../src/services/nativeHost.ts';
import {openLive,queryLive,closeLive} from '../src/services/liveConnections.ts';
import {connectionSnapshot,loadConnectionPreferences} from '../src/services/connectionPreferences.ts';
import {connections} from '../src/data/sample.ts';
const profile={...connections[0]!,mode:'live' as const,password:'synthetic-password',rootCertificatePem:'synthetic-public-ca',transport:{type:'direct' as const}};
function fixture(fail=false){
 const calls:Array<{command:string;payload:any}>=[];
 return {calls,global:{__TAURI__:{core:{async invoke(command:string,args:{message:string}){
  const request=JSON.parse(args.message);calls.push({command,payload:request.payload});
  if(command==='read_only_query'&&fail)throw{code:'QUERY_REJECTED',message:'synthetic-password must not be displayed'};
  const payload=command==='host_info'?{runtime:'desktop',appVersion:'0.1.0',sessionId:'test_session',nonce:request.payload.nonce,simulated:true,sqlite:true}
   :command==='contract_probe'?{values:request.payload.values,simulated:true}
   :command==='connect_direct'?{handle:'owned_handle',engineVersion:'fixture-version',database:profile.database,expiresAfterSeconds:3600}
   :command==='disconnect_database'?{connected:false}
   :{index:0,affectedRows:'0',truncated:true,columns:[{name:'value',databaseType:'BIGINT'},{name:'value',databaseType:'DECIMAL'},{name:'n',databaseType:'VARCHAR'}],rows:[[{kind:'integer',text:'18446744073709551615'},{kind:'decimal',text:'12345678901234567890.4500'},{kind:'null'}]]};
  return{version:1,requestId:request.requestId,payload};
 }}}}};
}
test('live bridge preserves ordered duplicate labels and exact raw values without mock fallback',async()=>{
 const f=fixture();await bootstrapNativeHost(f.global);
 const info=await openLive(profile);const result=await queryLive(info.handle,'SELECT value,value,n FROM fixture');
 assert.deepEqual(result.columns,['__live_0','__live_1','__live_2']);
 assert.equal(result.columnLabels!.__live_0,'value');assert.equal(result.columnLabels!.__live_1,'value');
 assert.equal(result.rows[0]!.__live_0,'18446744073709551615');assert.equal(result.rows[0]!.__live_1,'12345678901234567890.4500');assert.equal(result.rows[0]!.__live_2,null);
 assert.equal(result.truncated,true);assert.equal(result.raw!.rows[0]![1]!.kind,'decimal');
 await closeLive(info.handle);assert.equal(f.calls.find(c=>c.command==='connect_direct')!.payload.sessionId,'test_session');
 assert.deepEqual(f.calls.find(c=>c.command==='read_only_query')!.payload,{handle:'owned_handle',sql:'SELECT value,value,n FROM fixture',sessionId:'test_session'},'limits are left to the native runtime defaults');
 await bootstrapNativeHost({});
});
test('native query failures remain failures and hide untrusted error details',async()=>{
 const f=fixture(true);await bootstrapNativeHost(f.global);
 await assert.rejects(queryLive('owned_handle','SELECT * FROM fixture'),error=>error instanceof NativeOperationError&&error.code==='QUERY_REJECTED'&&!error.message.includes('synthetic-password'));
 await bootstrapNativeHost({});await assert.rejects(openLive(profile),/native desktop app/);
});
test('live preference migration never persists credentials, CA content or runtime handles',()=>{
 const serialized=JSON.stringify(connectionSnapshot([{...profile,handle:'not-allowed'} as any],[]));
 assert.equal(JSON.parse(serialized).connections[0].mode,'live');
 for(const secret of ['synthetic-password','synthetic-public-ca','not-allowed'])assert.ok(!serialized.includes(secret));
 assert.equal(loadConnectionPreferences({getItem:()=>JSON.stringify(connections)},connections).connections[0]!.mode,undefined);
});
test('live PostgreSQL uses the network engine id and SQLite requires a native grant',async()=>{
 const f=fixture();await bootstrapNativeHost(f.global);
 await openLive({...profile,engine:'PostgreSQL',port:5432});
 assert.equal(f.calls.find(c=>c.command==='connect_direct')!.payload.engine,'postgresql');
 await assert.rejects(openLive({...profile,engine:'SQLite'}),/Choose the SQLite file again/);
 assert.ok(!f.calls.some(c=>c.command==='connect_sqlite'));
 await assert.rejects(openLive({...profile,engine:'SQLite',sqliteGrant:'g',transport:{type:'ssh',host:'h',port:22,user:'u',auth:'agent'}}),/support MySQL, MariaDB and PostgreSQL over Direct or SSH/);
 assert.ok(!f.calls.some(c=>c.command==='connect_sqlite'),'SQLite never gains a network tunnel');
 await bootstrapNativeHost({});
});
test('F-01: SSH requests carry opaque references and native grants, never key material or a host key',async()=>{
 const f=fixture();await bootstrapNativeHost(f.global);
 const ssh={type:'ssh' as const,host:'bastion',port:2222,user:'deploy',hostKey:'ssh-ed25519 AAAA',hostKeyFingerprint:'SHA256:x'};
 await openLive({...profile,password:'',passwordRef:'db-ref',transport:{...ssh,auth:'password',passwordRef:'ssh-ref'}},'attempt_1');
 await openLive({...profile,transport:{...ssh,auth:'private-key',keyGrant:'grant',passphrase:'typed',passphraseRef:'unused',privateKey:'-----BEGIN OPENSSH PRIVATE KEY-----'}});
 await openLive({...profile,transport:{...ssh,auth:'agent',hostKey:undefined}});
 const sent=f.calls.filter(c=>c.command==='connect_direct').map(c=>c.payload);
 assert.equal(sent[0].passwordRef,'db-ref');assert.equal(sent[0].password,'');assert.equal(sent[0].attemptId,'attempt_1');
 assert.deepEqual(sent[0].ssh,{host:'bastion',port:2222,username:'deploy',auth:{method:'password',passwordRef:'ssh-ref'}},'legacy hostKey fields are never sent');
 assert.equal(sent[1].passwordRef,undefined,'a typed password wins over a reference');
 assert.deepEqual(sent[1].ssh.auth,{method:'key_file',grant:'grant',passphrase:'typed'});
 assert.ok(!JSON.stringify(sent).includes('PRIVATE KEY'));
 assert.deepEqual(sent[2].ssh,{host:'bastion',port:2222,username:'deploy',auth:{method:'agent'}});
 assert.ok(!JSON.stringify(sent).includes('hostKey'));
 await assert.rejects(openLive({...profile,transport:{...ssh,auth:'private-key'}}),/Choose the SSH key file again/);
 await bootstrapNativeHost({});
});
test('profile storage keeps secret references but drops secrets, grants, legacy host keys and malformed values',()=>{
 const live={...profile,passwordRef:'00000000-0000-4000-8000-000000000001',rememberPassword:true,transport:{type:'ssh' as const,host:'b',port:22,user:'d',auth:'private-key' as const,hostKey:'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl',hostKeyFingerprint:'SHA256:9Xl0yQZzJ8m2kKpz1bq5N2f3S5gkT0r9u1g6vXy3K0M',passphraseRef:'00000000-0000-4000-8000-000000000002',keyGrant:'grant-secret',keyFileName:'../id_ed25519',passphrase:'synthetic-passphrase',privateKey:'synthetic-key',rememberPassphrase:true}};
 const saved=JSON.parse(JSON.stringify(connectionSnapshot([live],[]))).connections[0];
 assert.equal(saved.passwordRef,live.passwordRef);
 assert.deepEqual(saved.transport,{type:'ssh',host:'b',port:22,user:'d',auth:'private-key',passphraseRef:live.transport.passphraseRef,keyFileName:'..id_ed25519'},'F-01: trust lives natively; legacy keys are dropped');
 for(const secret of ['synthetic-password','synthetic-passphrase','synthetic-key','grant-secret','remember'])assert.ok(!JSON.stringify(saved).includes(secret),secret);
 const tampered=JSON.parse(JSON.stringify(connectionSnapshot([{...live,passwordRef:'../../etc',transport:{...live.transport,hostKey:'ssh-rsa AAAA; rm -rf',passphraseRef:'x'}}],[]))).connections[0];
 assert.equal(tampered.passwordRef,undefined);assert.equal(tampered.transport.hostKey,undefined);assert.equal(tampered.transport.hostKeyFingerprint,undefined);assert.equal(tampered.transport.passphraseRef,undefined);
 const mock=JSON.parse(JSON.stringify(connectionSnapshot([{...live,mode:'mock'}],[]))).connections[0];
 assert.equal(mock.passwordRef,undefined,'mock profiles never carry OS secret references');
});
