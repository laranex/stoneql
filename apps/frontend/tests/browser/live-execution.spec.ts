import {test,expect} from '@playwright/test';
import { newLiveConnection } from './desktop';
// Read-write live SQL against a stubbed native host: explicit opt-in, PROD acknowledgement,
// destructive confirmation, cancel, per-statement outcomes, result sets and transactions.
for(const theme of ['light','dark'])test(`${theme}: read-write live SQL is explicit, confirmed and reports per-statement outcomes`,async({page})=>{
 await page.addInitScript(theme=>{
  localStorage.setItem('stoneql-theme',theme);
  const log:any[]=[];(window as any).liveLog=log;
  let fetches=0;let lease:string|null=null;
  (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
   const request=JSON.parse(args.message);const p=request.payload;log.push({command,payload:p});
   let payload:any;
   // Every StatementReport carries the full v1 wire shape, resultsWithheld included.
   const statement=(index:number,status:string,extra={})=>({index,status,inTransaction:!!lease,affectedRows:'0',resultSets:0,failure:null,resultsWithheld:false,...extra});
   if(command==='host_info')payload={runtime:'desktop',appVersion:'0.1.0',sessionId:'fixture_session',nonce:p.nonce,simulated:true,readWriteSql:true};
   else if(command==='contract_probe')payload={values:p.values,simulated:true};
   else if(command==='list_unresolved_outcomes')payload={outcomes:[],limit:16};
   else if(command==='connect_direct')payload={handle:'00000000-0000-0000-0000-000000000001',engineVersion:'MySQL fixture 8.4.6',database:p.database,expiresAfterSeconds:3600,access:p.access??'read_only'};
   else if(command==='disconnect_database')payload={connected:false};
   else if(command==='database_status')payload={connected:true};
   else if(command==='submit_job'){fetches=0;payload={jobId:p.sql.includes('SLEEP')?'job_slow':'job_1',statementCount:3};}
   else if(command==='cancel_job')payload={cancelRequested:true};
   else if(command==='fetch_job'){
    fetches++;
    if(p.jobId==='job_slow'){
     const cancelled=log.some(entry=>entry.command==='cancel_job');
     if(!cancelled){await new Promise(resolve=>setTimeout(resolve,200));payload={jobId:p.jobId,state:'running',outcome:null,cancellation:'none',serverCancelSent:false,deadlineExceeded:false,statements:[statement(0,'running')],batch:null,lease:null,done:false};}
     else payload={jobId:p.jobId,state:'finished',outcome:'unknown',cancellation:'confirmed',serverCancelSent:true,deadlineExceeded:false,statements:[statement(0,'unknown',{resultsWithheld:true}),statement(1,'not_run')],batch:null,lease:null,done:true};
    }else{
     const statements=[statement(0,'succeeded',{resultSets:1}),statement(1,'succeeded',{resultSets:1}),statement(2,'failed',{failure:{constraint:'unique',sqlState:'23000'}})];
     const batches=[
      {statement:0,result:0,columns:[{name:'big',databaseType:'BIGINT UNSIGNED',nullable:null}],rows:[[{kind:'integer',text:'18446744073709551615'}]],last:true,truncated:false},
      {statement:1,result:0,columns:[{name:'amount',databaseType:'DECIMAL',nullable:null}],rows:[[{kind:'decimal',text:'12345678901234567890.4500'}]],last:true,truncated:false},
     ];
     payload={jobId:p.jobId,state:'finished',outcome:'failed',cancellation:'none',serverCancelSent:false,deadlineExceeded:false,statements,batch:batches[fetches-1]??null,lease:lease?'rollback_only':null,done:fetches>batches.length};
    }
   }else if(command==='begin_transaction'){lease='lease_1';payload={leaseId:lease,idleSeconds:60};}
   else if(command==='end_transaction'){lease=null;payload={outcome:p.action==='commit'?'committed':'rolled_back',reason:'requested',commitSent:p.action==='commit',rollbackSent:p.action!=='commit'};}
   else if(command==='resolve_close')payload={closed:p.action==='confirm'};
   else throw{code:'INVALID_REQUEST'};
   return{version:1,requestId:request.requestId,payload};
  }}};
 },theme);
 await page.goto('/');
 await newLiveConnection(page);
 await page.getByPlaceholder('e.g. Local development').fill('Orders PROD');
 await page.getByLabel('Connection environment',{exact:true}).selectOption('Production');
 await expect(page.getByLabel('Session access',{exact:true})).toHaveValue('read_only');
 await page.getByLabel('Session access',{exact:true}).selectOption('read_write');
 await page.getByRole('button',{name:'Save & connect',exact:true}).click();
 // PROD requires the explicit acknowledgement before opening read-write.
 const accept=page.getByRole('button',{name:'Open read-write',exact:true});
 await expect(accept).toBeDisabled();
 await page.getByLabel(/PROD — I understand/).check();
 await accept.click();
 await expect(page.getByTestId('live-access')).toHaveText('Read-write');
 await expect(page.locator('.footer-session')).toContainText('Live read-write · connected');
 const connect=await page.evaluate(()=>(window as any).liveLog.find((e:any)=>e.command==='connect_direct').payload);
 expect(connect.access).toBe('read_write');
 expect(await page.evaluate(()=>localStorage.getItem('stoneql-connections'))).not.toContain('read_write');
 // Destructive statements are confirmed (UX safeguard) and then run as one native job.
 await page.locator('.cm-content').fill("SELECT big FROM t; SELECT amount FROM t; UPDATE t SET note = 'x'");
 await page.getByRole('button',{name:'Run query',exact:true}).click();
 await expect(page.getByRole('alertdialog')).toContainText('UPDATE');
 await page.getByLabel(/PROD — I understand/).check();
 await page.getByRole('button',{name:'Run statements',exact:true}).click();
 await expect(page.getByText('18446744073709551615',{exact:true})).toBeVisible();
 await expect(page.locator('.statement-outcome')).toHaveText(['#1 succeeded','#2 succeeded','#3 failed']);
 await expect(page.getByText(/rejected statement 3 of 3 \(unique constraint\) · SQLSTATE 23000/)).toBeVisible();
 await page.getByRole('tab',{name:'Result 2 · statement 2'}).click();
 await expect(page.getByText('12345678901234567890.4500',{exact:true})).toBeVisible();
 await page.screenshot({path:`test-results/live-execution-${theme}.png`,fullPage:true});
 // R-4: with saved SQL and no live work a reload is not held; a running live job is.
 const reloadHeld=()=>page.evaluate(()=>{const event=new Event('beforeunload',{cancelable:true});window.dispatchEvent(event);return event.defaultPrevented;});
 // Cancel reports confirmation separately from the (unknown) effects.
 await page.locator('.cm-content').fill('SELECT SLEEP(30); SELECT 1');
 await page.getByRole('button',{name:'Save SQL',exact:true}).click();
 expect(await reloadHeld()).toBe(false);
 await page.getByRole('button',{name:'Run query',exact:true}).click();
 await expect(page.getByRole('button',{name:'Cancel',exact:true})).toBeVisible();
 expect(await reloadHeld()).toBe(true);
 await page.getByRole('button',{name:'Cancel',exact:true}).click();
 await expect(page.getByText(/Outcome unknown at statement 1 of 2 \(cancellation confirmed; effects not\)/)).toBeVisible();
 await expect(page.locator('.statement-outcome')).toHaveText(['#1 unknown','#2 not run']);
 // B-11: rows discarded natively after the interrupt are reported as incomplete.
 await expect(page.getByText(/Results of statement 1 after the interrupt were not delivered; the rows shown are incomplete\./)).toBeVisible();
 // Transactions: begin, run inside, roll back (server-confirmed in the stub).
 expect(await reloadHeld()).toBe(false);
 await page.getByRole('button',{name:'Begin transaction',exact:true}).click();
 await expect(page.getByText('Transaction open',{exact:true})).toBeVisible();
 // R-8: the lease text states both limits and does not assert a rollback.
 const transactionBar=page.getByRole('group',{name:'Transaction'});
 await expect(transactionBar).toContainText('Ends after 60 s idle or 15 min; outcome shown on Commit, Rollback or Disconnect');
 await expect(transactionBar).not.toContainText('Rolled back after');
 // R-4: an open transaction holds a reload and warns on native close; Cancel keeps it open.
 expect(await reloadHeld()).toBe(true);
 await page.evaluate(()=>window.dispatchEvent(new Event('stoneql-close-requested')));
 const closeDialog=page.getByRole('alertdialog');
 await expect(closeDialog).toContainText('A live transaction is open or a live statement is running.');
 await expect(closeDialog).toContainText('listed the next time StoneQL starts, until you acknowledge it');
 await closeDialog.getByRole('button',{name:'Cancel',exact:true}).click();
 await expect.poll(()=>page.evaluate(()=>(window as any).liveLog.filter((e:any)=>e.command==='resolve_close').map((e:any)=>e.payload.action))).toEqual(['prompted','cancel']);
 await expect(page.getByText('Transaction open',{exact:true})).toBeVisible();
 await page.locator('.cm-content').fill('SELECT big FROM t');
 await page.getByRole('button',{name:'Run query',exact:true}).click();
 await expect(page.getByText('Transaction failed · rollback only',{exact:true})).toBeVisible();
 await expect(page.getByRole('button',{name:'Commit',exact:true})).toBeDisabled();
 const submit=await page.evaluate(()=>(window as any).liveLog.filter((e:any)=>e.command==='submit_job').at(-1).payload);
 expect(submit.leaseId).toBe('lease_1');
 await page.getByRole('button',{name:'Rollback',exact:true}).click();
 await expect(page.getByText(/Transaction rolled back \(confirmed by the server\)/)).toBeVisible();
 await expect(page.getByRole('button',{name:'Begin transaction',exact:true})).toBeVisible();
 expect(await page.evaluate(()=>(window as any).liveLog.filter((e:any)=>e.command==='submit_job').length)).toBe(3);
});

// Requirement 1, V-3 and B-3 against a stubbed host: the read-write option follows the native
// gate; a job whose connection failed never reads as success; disconnecting an open
// transaction asks first and then shows the recorded outcome.
for(const theme of ['light','dark'])test(`${theme}: read-write gate, failed-connection job and disconnect with an open transaction`,async({page})=>{
 await page.addInitScript(theme=>{
  localStorage.setItem('stoneql-theme',theme);
  const log:any[]=[];(window as any).liveLog=log;
  // The gate is off until the test turns the stub's host report on (a reload re-bootstraps).
  const gate=sessionStorage.getItem('gate')==='on';
  let lease:string|null=null;let closed=false;
  (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
   const request=JSON.parse(args.message);const p=request.payload;log.push({command,payload:p});
   let payload:any;
   if(command==='host_info')payload={runtime:'desktop',appVersion:'0.1.0',sessionId:'fixture_session',nonce:p.nonce,simulated:true,...(gate?{readWriteSql:true}:{})};
   else if(command==='contract_probe')payload={values:p.values,simulated:true};
   else if(command==='list_unresolved_outcomes')payload={outcomes:[],limit:16};
   else if(command==='connect_direct')payload={handle:'00000000-0000-0000-0000-000000000002',engineVersion:'MySQL fixture 8.4.6',database:p.database,expiresAfterSeconds:3600,access:p.access??'read_only'};
   else if(command==='disconnect_database'){closed=true;payload={connected:false};}
   else if(command==='database_status')payload={connected:true};
   else if(command==='submit_job')payload={jobId:'job_failed',statementCount:2};
   else if(command==='fetch_job')payload={jobId:p.jobId,state:'finished',outcome:'failed',cancellation:'none',serverCancelSent:false,deadlineExceeded:false,statements:[0,1].map(index=>({index,status:'not_run',inTransaction:false,affectedRows:'0',resultSets:0,failure:null,resultsWithheld:false})),batch:null,lease:null,done:true};
   else if(command==='begin_transaction'){lease='lease_9';payload={leaseId:lease,idleSeconds:60};}
   else if(command==='end_transaction')payload=closed?{outcome:'rolled_back',reason:'closed',commitSent:false,rollbackSent:true}:{outcome:'rolled_back',reason:'requested',commitSent:false,rollbackSent:true};
   else throw{code:'INVALID_REQUEST'};
   return{version:1,requestId:request.requestId,payload};
  }}};
 },theme);
 await page.goto('/');
 await newLiveConnection(page);
 const option=page.getByLabel('Session access',{exact:true}).locator('option[value="read_write"]');
 await expect(option).toHaveJSProperty('disabled',true);
 await expect(option).toHaveText(/pending safety approval/);
 await page.screenshot({path:`test-results/live-gate-${theme}.png`,fullPage:true});
 // Turn the stubbed host's gate on (a host report, not renderer state) and start again.
 await page.evaluate(()=>sessionStorage.setItem('gate','on'));
 await page.reload();
 await newLiveConnection(page);
 await page.getByPlaceholder('e.g. Local development').fill('Orders dev');
 await expect(page.getByLabel('Session access',{exact:true}).locator('option[value="read_write"]')).toHaveJSProperty('disabled',false);
 await page.getByLabel('Session access',{exact:true}).selectOption('read_write');
 await page.getByRole('button',{name:'Save & connect',exact:true}).click();
 await expect(page.getByTestId('live-access')).toHaveText('Read-write');
 // V-3: the job's connection could not be opened; nothing reads as success.
 await page.locator('.cm-content').fill('SELECT 1; SELECT 2');
 await page.getByRole('button',{name:'Run query',exact:true}).click();
 await expect(page.getByText("The job's connection could not be opened; nothing was sent.",{exact:true})).toBeVisible();
 await expect(page.locator('.statement-outcome')).toHaveText(['#1 not run','#2 not run']);
 await expect(page.getByText(/executed successfully/)).toHaveCount(0);
 // B-3: disconnecting with an open transaction asks first, then shows the outcome.
 await page.getByRole('button',{name:'Begin transaction',exact:true}).click();
 await expect(page.getByText('Transaction open',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Disconnect',exact:true}).click();
 await expect(page.getByRole('alertdialog')).toContainText('open transaction');
 expect(await page.evaluate(()=>(window as any).liveLog.some((e:any)=>e.command==='disconnect_database'))).toBe(false);
 await page.getByRole('button',{name:'Disconnect and roll back',exact:true}).click();
 await expect(page.getByText(/Transaction rolled back \(confirmed by the server\)\. Its connection was closed\./)).toBeVisible();
 const commands=await page.evaluate(()=>(window as any).liveLog.map((e:any)=>e.command));
 expect(commands.slice(commands.indexOf('disconnect_database'))).toEqual(['disconnect_database','end_transaction']);
 await page.screenshot({path:`test-results/live-disconnect-lease-${theme}.png`,fullPage:true});
});
