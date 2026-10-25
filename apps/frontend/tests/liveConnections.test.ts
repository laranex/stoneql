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
  const payload=command==='host_info'?{runtime:'desktop',appVersion:'0.1.0',sessionId:'test_session',nonce:request.payload.nonce,simulated:true}
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
