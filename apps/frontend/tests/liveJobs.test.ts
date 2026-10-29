import test,{mock} from 'node:test';
import assert from 'node:assert/strict';
import {bootstrapNativeHost,NativeOperationError} from '../src/services/nativeHost.ts';
import {openLive,runLiveJob,cancelLiveJob,beginLiveTransaction,endLiveTransaction,writeKeywords,describeJob,describeTransactionEnd,type LiveJobResult} from '../src/services/liveConnections.ts';
import {connectionSnapshot} from '../src/services/connectionPreferences.ts';
import {connections} from '../src/data/sample.ts';
const profile={...connections[0]!,mode:'live' as const,transport:{type:'direct' as const}};
type Reply=(payload:any)=>any;
function host(replies:Record<string,Reply>){
 const calls:Array<{command:string;payload:any}>=[];
 return {calls,global:{__TAURI__:{core:{async invoke(command:string,args:{message:string}){
  const request=JSON.parse(args.message);calls.push({command,payload:request.payload});
  const base:Record<string,Reply>={host_info:p=>({runtime:'desktop',appVersion:'0.1.0',sessionId:'s',nonce:p.nonce,simulated:true}),contract_probe:p=>({values:p.values,simulated:true})};
  const reply=replies[command]??base[command];
  if(!reply)throw{code:'INTERNAL',message:'unexpected'};
  return{version:1,requestId:request.requestId,payload:reply(request.payload)};
 }}}}};
}
const statement=(index:number,status:string,extra={})=>({index,status,inTransaction:false,affectedRows:'0',resultSets:0,failure:null,resultsWithheld:false,...extra});
test('read-write is an explicit connect option; read-only stays the default payload',async()=>{
 const h=host({connect_direct:()=>({handle:'h',engineVersion:'v',database:'d',expiresAfterSeconds:3600,access:'read_only'})});
 await bootstrapNativeHost(h.global);
 await openLive(profile);await openLive(profile,undefined,'read_write');
 const [first,second]=h.calls.filter(c=>c.command==='connect_direct');
 assert.equal('access' in first!.payload,false);assert.equal(second!.payload.access,'read_write');
 assert.ok(!JSON.stringify(connectionSnapshot([{...profile,access:'read_write'}],[])).includes('read_write'),'the session choice is never persisted');
 await bootstrapNativeHost({});
});
test('jobs assemble paged result sets per statement, keep exact values and report outcomes',async()=>{
 const pages=[
  {batch:{statement:0,result:0,columns:[{name:'n',databaseType:'BIGINT',nullable:null}],rows:[[{kind:'integer',text:'18446744073709551615'}]],last:false,truncated:false}},
  {batch:{statement:0,result:0,columns:null,rows:[[{kind:'null'}]],last:true,truncated:true}},
  {batch:{statement:1,result:0,columns:[{name:'d',databaseType:'DECIMAL',nullable:null}],rows:[[{kind:'decimal',text:'0.4500'}]],last:true,truncated:false}},
  {batch:null,done:true},
 ];
 const statements=[statement(0,'succeeded',{resultSets:1}),statement(1,'succeeded',{resultSets:1}),statement(2,'failed',{failure:{constraint:'unique',sqlState:'23000'}})];
 const h=host({submit_job:()=>({jobId:'job_1',statementCount:3}),fetch_job:()=>({jobId:'job_1',state:'running',outcome:'failed',cancellation:'none',serverCancelSent:false,deadlineExceeded:false,statements,lease:null,done:false,...pages.shift()})});
 await bootstrapNativeHost(h.global);
 let seen='';
 const job=await runLiveJob('h','SELECT 1; SELECT 2; INSERT',{onJob:id=>{seen=id;}});
 assert.equal(seen,'job_1');
 assert.equal(job.outcome,'failed');assert.equal(job.statements[2]!.failure!.constraint,'unique');
 assert.equal(job.resultSets.length,2);
 assert.deepEqual(job.resultSets[0]!.rows.map(r=>r.__live_0),['18446744073709551615',null]);
 assert.equal(job.resultSets[0]!.truncated,true);assert.equal(job.resultSets[1]!.rows[0]!.__live_0,'0.4500');
 assert.equal(h.calls.filter(c=>c.command==='submit_job').length,1,'never resubmitted');
 assert.ok(h.calls.filter(c=>c.command==='fetch_job').every(c=>c.payload.jobId==='job_1'&&c.payload.waitMs===1000));
 await bootstrapNativeHost({});
});
test('cancel, transactions and BUSY use the native contracts without retry',async()=>{
 let submits=0;
 const h=host({
  submit_job:()=>{submits++;throw{code:'BUSY',message:'busy'};},
  cancel_job:()=>({cancelRequested:true}),begin_transaction:()=>({leaseId:'lease_1',idleSeconds:60}),
  end_transaction:p=>({outcome:p.action==='commit'?'committed':'rolled_back',reason:'requested',commitSent:p.action==='commit',rollbackSent:p.action!=='commit'}),
 });
 await bootstrapNativeHost(h.global);
 await assert.rejects(runLiveJob('h','UPDATE t SET a = 1',{leaseId:'lease_1'}),e=>e instanceof NativeOperationError&&e.code==='BUSY'&&/Nothing was queued/.test(e.message));
 assert.equal(submits,1);assert.equal(h.calls.find(c=>c.command==='submit_job')!.payload.leaseId,'lease_1');
 assert.equal((await cancelLiveJob('job_1')).cancelRequested,true);
 assert.equal((await beginLiveTransaction('h')).leaseId,'lease_1');
 assert.deepEqual(await endLiveTransaction('lease_1','rollback'),{outcome:'rolled_back',reason:'requested',commitSent:false,rollbackSent:true});
 await bootstrapNativeHost({});
});
test('write detection ignores quoted text and comments but sees executable comments',()=>{
 assert.deepEqual(writeKeywords("SELECT 'DROP TABLE x', `update` FROM t -- delete\n/* truncate */"),[]);
 assert.deepEqual(writeKeywords('delete from t; UPDATE t SET a=1; delete from u'),['DELETE','UPDATE']);
 assert.deepEqual(writeKeywords('/*!50001 DROP VIEW v */'),['DROP']);
});

const job=(outcome:LiveJobResult['outcome'],statuses:string[],extra:Partial<LiveJobResult>={}):LiveJobResult=>({outcome,cancellation:'none',serverCancelSent:false,deadlineExceeded:false,lease:null,resultSets:[],statements:statuses.map((status,index)=>statement(index,status)) as LiveJobResult['statements'],...extra});
test('V-3: only a succeeded job is described as executed successfully',()=>{
 const opened=describeJob(job('failed',['not_run','not_run']),false);
 assert.equal(opened.kind,'error');assert.match(opened.text,/connection could not be opened; nothing was sent/);
 for(const notice of [opened,describeJob(job('failed',['succeeded','failed']),false),describeJob(job('unknown',['unknown','not_run']),false),describeJob(job('cancelled',['succeeded','not_run']),false),describeJob(job('cancelled',['not_run']),false),describeJob(job('unknown',['succeeded']),false)]){
  assert.notEqual(notice.kind,'success');assert.doesNotMatch(notice.text,/executed successfully/);
 }
 assert.match(describeJob(job('cancelled',['not_run']),false).text,/before any statement was sent/);
 const ok=describeJob(job('succeeded',['succeeded','succeeded']),true);
 assert.deepEqual(ok,{kind:'success',text:'2 statements executed successfully (in the open transaction).'});
});
test('R-3: autocommit success never claims "committed"; a diverged session is unknown',()=>{
 assert.deepEqual(describeJob(job('succeeded',['succeeded']),false),{kind:'success',text:'1 statement executed successfully (autocommit).'});
 for(const inTransaction of [false,true]){
  const diverged=describeJob(job('unknown',['succeeded','succeeded']),inTransaction);
  assert.equal(diverged.kind,'error');
  assert.match(diverged.text,/^Outcome unknown: the session's transaction state after the job did not match/);
  assert.doesNotMatch(diverged.text,/executed successfully|committed\)|\(committed/);
 }
});
test('B-6: an unconfirmed cancellation says the statement may still be running',()=>{
 const requested=describeJob(job('unknown',['unknown','not_run'],{cancellation:'requested'}),false);
 assert.match(requested.text,/Outcome unknown at statement 1 of 2 \(cancellation requested, not confirmed; it may still be running on the server\)/);
 assert.doesNotMatch(describeJob(job('unknown',['unknown'],{cancellation:'confirmed'}),false).text,/still be running/);
});
test('transaction ends report only server-confirmed outcomes and what was sent',()=>{
 assert.equal(describeTransactionEnd({outcome:'committed',reason:'requested',commitSent:true,rollbackSent:false}).kind,'success');
 assert.match(describeTransactionEnd({outcome:'rolled_back',reason:'closed',commitSent:false,rollbackSent:true}).text,/rolled back \(confirmed by the server\)\. Its connection was closed/);
 const commit=describeTransactionEnd({outcome:'unknown',reason:'requested',commitSent:true,rollbackSent:false});
 assert.equal(commit.kind,'error');assert.match(commit.text,/unknown\. COMMIT may have been sent and was not confirmed/);
 assert.match(describeTransactionEnd({outcome:'unknown',reason:'closed',commitSent:false,rollbackSent:true}).text,/ROLLBACK may have been sent and was not confirmed/);
 assert.match(describeTransactionEnd({outcome:'unknown',reason:'requested',commitSent:true,rollbackSent:true}).text,/COMMIT and then ROLLBACK may have been sent; neither was confirmed/);
 for(const end of [true,false].flatMap(commitSent=>[true,false].map(rollbackSent=>describeTransactionEnd({outcome:'unknown',reason:'requested',commitSent,rollbackSent}))))
  assert.doesNotMatch(end.text,/(COMMIT|ROLLBACK) was sent but/);
 assert.match(describeTransactionEnd({outcome:'unknown',reason:'closed',commitSent:false,rollbackSent:false}).text,/Neither COMMIT nor ROLLBACK was sent or confirmed/);
 for(const end of [commit]) assert.doesNotMatch(end.text,/rolled back|committed \(/);
});
test('B-2: a commit whose reply exceeds the renderer wait is unknown, after at least the native bound, and is not resent',async()=>{
 let ends=0;
 const h=host({});
 // end_transaction never replies; everything else uses the standard stub.
 const global={__TAURI__:{core:{async invoke(command:string,args:{message:string}){
  if(command==='end_transaction'){ends++;return new Promise(()=>undefined);}
  return h.global.__TAURI__.core.invoke(command,args);
 }}}};
 await bootstrapNativeHost(global);
 mock.timers.enable({apis:['setTimeout']});
 try{
  for(const [action,pattern] of [['commit',/^Commit outcome unknown: COMMIT may have been sent and was not confirmed\. Verify on the server; StoneQL never retries\.$/],['rollback',/^Rollback outcome unknown: .*never claimed rolled back/]] as const){
   let settled=false;
   const pending=endLiveTransaction('lease_1',action).then(()=>{settled=true;},error=>{settled=true;return error;});
   for(let i=0;i<5;i++)await Promise.resolve();
   mock.timers.tick(14_000);for(let i=0;i<5;i++)await Promise.resolve();
   assert.equal(settled,false,'the renderer must wait at least the native COMMIT+ROLLBACK+close bound');
   mock.timers.tick(6_000);
   const error=await pending;
   assert.ok(error instanceof NativeOperationError);assert.equal(error.code,'DEADLINE_EXCEEDED');
   assert.match(error.message,pattern);assert.doesNotMatch(error.message,/cancellation/i);
  }
 }finally{mock.timers.reset();}
 assert.equal(ends,2,'one end_transaction per request; never resent');
 await bootstrapNativeHost({});
});
test('B-10: a fetch failure after the job was accepted reports an unknown outcome without resubmitting',async()=>{
 let submits=0;
 const h=host({submit_job:()=>{submits++;return{jobId:'job_1',statementCount:1};},fetch_job:()=>{throw{code:'INTERNAL',message:'gone'};}});
 await bootstrapNativeHost(h.global);
 await assert.rejects(runLiveJob('h','UPDATE t SET a = 1'),e=>e instanceof NativeOperationError&&/^Outcome unknown: the job was accepted/.test(e.message)&&/may still be running natively; StoneQL never retries/.test(e.message));
 assert.equal(submits,1);
 await bootstrapNativeHost({});
});
test('B-11: results withheld after an interrupt are marked incomplete and said so',async()=>{
 const pages=[
  {batch:{statement:0,result:0,columns:[{name:'n',databaseType:'BIGINT',nullable:null}],rows:[[{kind:'integer',text:'1'}]],last:false,truncated:false}},
  {batch:{statement:1,result:0,columns:[{name:'m',databaseType:'BIGINT',nullable:null}],rows:[[{kind:'integer',text:'2'}]],last:true,truncated:false}},
  {batch:null,done:true},
 ];
 const statements=[statement(0,'succeeded',{resultSets:1,resultsWithheld:true}),statement(1,'succeeded',{resultSets:1,resultsWithheld:false})];
 const h=host({submit_job:()=>({jobId:'job_1',statementCount:2}),fetch_job:()=>({jobId:'job_1',state:'finished',outcome:'succeeded',cancellation:'confirmed',serverCancelSent:false,deadlineExceeded:false,statements,lease:null,done:false,...pages.shift()})});
 await bootstrapNativeHost(h.global);
 const result=await runLiveJob('h','SELECT 1; SELECT 2');
 assert.deepEqual(result.resultSets.map(set=>set.truncated),[true,false]);
 const notice=describeJob(result,false);
 assert.match(notice.text,/Results of statement 1 after the interrupt were not delivered; the rows shown are incomplete\.$/);
 assert.doesNotMatch(describeJob(job('succeeded',['succeeded']),false).text,/not delivered/);
 await bootstrapNativeHost({});
});
test('L-3: a statement ended by the server statement timeout is described as a time limit, not a rejection or an unknown',()=>{
 const timedOut=(index:number,inTransaction=false)=>job('failed',index?['succeeded','failed']:['failed','not_run'],{deadlineExceeded:true,statements:(index?['succeeded','failed']:['failed','not_run']).map((status,i)=>statement(i,status,{inTransaction,...(status==='failed'?{failure:{constraint:null,sqlState:'57014'}}:{})})) as LiveJobResult['statements']});
 const first=describeJob(timedOut(0),false);
 assert.deepEqual(first,{kind:'error',text:'The server stopped statement 1 of 2 at the statement time limit · SQLSTATE 57014. Later statements were not sent.'});
 assert.match(describeJob(timedOut(1),false).text,/^The server stopped statement 2 of 2 at the statement time limit · SQLSTATE 57014\. Earlier statements were committed; later statements were not sent\.$/);
 assert.match(describeJob(timedOut(1,true),true).text,/Earlier statements remain in the open transaction/);
 for(const notice of [first,describeJob(timedOut(1),false)])assert.doesNotMatch(notice.text,/rejected|unknown|cancel|rolled back|executed successfully/i);
 // Without deadlineExceeded (for example someone else's cancel), a 57014 stays a rejection.
 const external=job('failed',['failed'],{statements:[statement(0,'failed',{failure:{constraint:null,sqlState:'57014'}})] as LiveJobResult['statements']});
 assert.match(describeJob(external,false).text,/^The database rejected statement 1 of 1 · SQLSTATE 57014\./);
});
