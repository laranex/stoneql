import {test,expect} from '@playwright/test';
// M006 Contract 4.5: unknown outcomes the desktop host recorded before a reload are listed by the
// new renderer, prominently, until the user acknowledges each one. The stub keeps the host's
// state in sessionStorage, which (like the native process) survives the page reload.
for(const theme of ['light','dark']) test(`${theme}: unknown outcomes stay listed across a reload until acknowledged`,async({page})=>{
  await page.addInitScript((theme)=>{
    localStorage.setItem('stoneql-theme',theme);
    const state=()=>JSON.parse(sessionStorage.getItem('hostOutcomes')||'{"outcomes":[],"acks":[]}');
    const save=(value:unknown)=>sessionStorage.setItem('hostOutcomes',JSON.stringify(value));
    (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
      const request=JSON.parse(args.message), host=state();
      let payload:unknown;
      if(command==='host_info'){
        // The confirmed reload rotates the session; the reset interrupts a running job, whose
        // outcome is unknown.
        if(request.payload.nonce.startsWith('reload_'))host.outcomes.push({outcomeId:'job_9',database:'shop',secondsAgo:1,transaction:null,job:{statementCount:2,unknownStatement:1,inTransaction:false,cancellation:'confirmed',serverCancelSent:true,deadlineExceeded:false}},{outcomeId:'lease_9',database:'shop',secondsAgo:1,transaction:{outcome:'unknown',reason:'closed',commitSent:false,rollbackSent:true},job:null});
        payload={runtime:'desktop',appVersion:'0.1.0',sessionId:`session_${request.payload.nonce}`,nonce:request.payload.nonce,simulated:true};
      } else if(command==='contract_probe') payload={simulated:true,values:request.payload.values};
      else if(command==='list_unresolved_outcomes') payload={outcomes:host.outcomes,limit:16};
      else if(command==='acknowledge_outcome'){
        host.acks.push(request.payload);
        host.outcomes=host.outcomes.filter((o:any)=>o.outcomeId!==request.payload.outcomeId);
        payload={outcomes:host.outcomes,limit:16};
      } else throw {code:'INTERNAL',message:command};
      save(host);
      return {version:1,requestId:request.requestId,payload};
    }}};
  },theme);
  await page.goto('/');
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  const panel=page.getByRole('alert',{name:'Unknown outcomes'});
  await expect(panel).toHaveCount(0);
  await page.evaluate(()=>window.dispatchEvent(new Event('stoneql-reload-requested')));
  const dialog=page.getByRole('alertdialog');
  await expect(dialog).toContainText('listed after the reload, and after a restart, until you acknowledge it');
  const reloaded=page.waitForEvent('load');
  await dialog.getByRole('button',{name:'Reload StoneQL',exact:true}).click();
  await reloaded;
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  await expect(panel).toBeVisible();
  await expect(panel).toContainText('Outcome unknown — the server may or may not have applied this work.');
  const entries=panel.locator('.unresolved-outcome');
  await expect(entries).toHaveCount(2);
  await expect(entries.nth(0)).toContainText('Before a reload');
  await expect(entries.nth(0)).toContainText('SQL job on “shop” at statement 2 of 2');
  await expect(entries.nth(1)).toContainText('Transaction on “shop” was closed');
  await expect(entries.nth(1)).toContainText('ROLLBACK may have been sent and was not confirmed.');
  for(const entry of [entries.nth(0),entries.nth(1)]){
    await expect(entry).toContainText('Outcome unknown — the server may or may not have applied it.');
    await expect(entry).not.toContainText(/committed|rolled back|succeeded/i);
  }
  // Prominent in both themes: the danger colour on its own background, not the page's text colour.
  const colours=await entries.nth(0).evaluate((el)=>{const s=getComputedStyle(el);return [s.color,s.backgroundColor,getComputedStyle(document.body).color];});
  expect(colours[0]).not.toBe(colours[1]);
  expect(colours[0]).not.toBe(colours[2]);
  await page.screenshot({path:`test-results/unresolved-outcomes-${theme}.png`});
  // Another reload without acknowledgement keeps both.
  await page.reload();
  await expect(entries).toHaveCount(2);
  await entries.nth(0).getByRole('button',{name:/^Acknowledge/}).click();
  await expect(entries).toHaveCount(1);
  await expect(entries.nth(0)).toContainText('Transaction on “shop”');
  await page.reload();
  await expect(entries).toHaveCount(1);
  await entries.nth(0).getByRole('button',{name:/^Acknowledge/}).click();
  await expect(panel).toHaveCount(0);
  await page.reload();
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  await expect(panel).toHaveCount(0);
  const acks=await page.evaluate(()=>JSON.parse(sessionStorage.getItem('hostOutcomes')!).acks);
  expect(acks.map((a:any)=>Object.keys(a).sort().join(','))).toEqual(['outcomeId,sessionId','outcomeId,sessionId']);
  expect(acks.map((a:any)=>a.outcomeId)).toEqual(['job_9','lease_9']);
});

// F-3: a job the reload's reset detached (it outran the release bound) records its unknown
// outcome after the new renderer first listed. The host reports it pending; the renderer lists
// again until nothing is pending, so the outcome appears without any user action.
for(const theme of ['light','dark']) test(`${theme}: an unknown outcome recorded after the reload appears without user action`,async({page})=>{
  await page.addInitScript((theme)=>{
    localStorage.setItem('stoneql-theme',theme);
    const state=()=>JSON.parse(sessionStorage.getItem('hostLate')||'{"lists":0,"rotated":false}');
    (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
      const request=JSON.parse(args.message), host=state();
      let payload:unknown;
      if(command==='host_info'){
        if(request.payload.nonce.startsWith('reload_'))host.rotated=true;
        payload={runtime:'desktop',appVersion:'0.1.0',sessionId:`session_${request.payload.nonce}`,nonce:request.payload.nonce,simulated:true};
      } else if(command==='contract_probe') payload={simulated:true,values:request.payload.values};
      else if(command==='list_unresolved_outcomes'){
        // After the rotation the detached job is still ending for the first two lists.
        const late=host.rotated&&++host.lists>=3;
        payload={outcomes:late?[{outcomeId:'job_late',database:'shop',secondsAgo:0,transaction:null,job:{statementCount:1,unknownStatement:0,inTransaction:false,cancellation:'requested',serverCancelSent:true,deadlineExceeded:false}}]:[],limit:16,pending:host.rotated&&!late?1:0};
      } else throw {code:'INTERNAL',message:command};
      sessionStorage.setItem('hostLate',JSON.stringify(host));
      return {version:1,requestId:request.requestId,payload};
    }}};
  },theme);
  await page.goto('/');
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  const panel=page.getByRole('alert',{name:'Unknown outcomes'});
  await page.evaluate(()=>window.dispatchEvent(new Event('stoneql-reload-requested')));
  const reloaded=page.waitForEvent('load');
  await page.getByRole('alertdialog').getByRole('button',{name:'Reload StoneQL',exact:true}).click();
  await reloaded;
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  await expect.poll(()=>page.evaluate(()=>JSON.parse(sessionStorage.getItem('hostLate')!).lists)).toBeGreaterThanOrEqual(1);
  await expect(panel).toBeVisible({timeout:10_000});
  await expect(panel.locator('.unresolved-outcome')).toHaveCount(1);
  await expect(panel).toContainText('SQL job on “shop” at statement 1 of 1');
  await expect(panel).not.toContainText(/committed|rolled back|succeeded/i);
  await page.screenshot({path:`test-results/unresolved-late-${theme}.png`});
  // Polling stopped once nothing was pending.
  const lists=await page.evaluate(()=>JSON.parse(sessionStorage.getItem('hostLate')!).lists);
  await page.waitForTimeout(2500);
  expect(await page.evaluate(()=>JSON.parse(sessionStorage.getItem('hostLate')!).lists)).toBe(lists);
});

// Durable journal: outcomes recorded before StoneQL restarted (quit or crash) are listed as such,
// apart from those recorded before a reload; a damaged journal and a journal that cannot be
// written are reported in the same prominent panel.
for(const theme of ['light','dark']) test(`${theme}: outcomes from before a restart and journal problems are shown`,async({page})=>{
  await page.addInitScript((theme)=>{
    localStorage.setItem('stoneql-theme',theme);
    const state=()=>JSON.parse(sessionStorage.getItem('hostJournal')||'null')??{journal:'write_failed',outcomes:[
      {outcomeId:'lease_r',engine:'mariadb',database:'shop',secondsAgo:7200,restored:true,appExited:true,transaction:{outcome:'unknown',reason:'closed',commitSent:false,rollbackSent:false},job:null,journalDamage:null},
      {outcomeId:'job_r',engine:'postgresql',database:'ledger',secondsAgo:600,restored:true,appExited:true,transaction:null,job:{statementCount:3,unknownStatement:1,inTransaction:false,cancellation:'none',serverCancelSent:false,deadlineExceeded:false},journalDamage:null},
      {outcomeId:'damage_r',engine:null,database:'',secondsAgo:5,restored:true,appExited:false,transaction:null,job:null,journalDamage:{unreadableRecords:2,preservedAs:'unknown-outcomes.journal.damaged-1-abcd'}},
      {outcomeId:'lease_now',engine:'mysql',database:'shop',secondsAgo:3,restored:false,appExited:false,transaction:{outcome:'unknown',reason:'requested',commitSent:true,rollbackSent:false},job:null,journalDamage:null},
    ]};
    (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
      const request=JSON.parse(args.message), host=state();
      let payload:unknown;
      if(command==='host_info') payload={runtime:'desktop',appVersion:'0.1.0',sessionId:`session_${request.payload.nonce}`,nonce:request.payload.nonce,simulated:true};
      else if(command==='contract_probe') payload={simulated:true,values:request.payload.values};
      else if(command==='list_unresolved_outcomes') payload={outcomes:host.outcomes,limit:16,pending:0,journal:host.journal};
      else if(command==='acknowledge_outcome'){
        host.outcomes=host.outcomes.filter((o:any)=>o.outcomeId!==request.payload.outcomeId);
        // The acknowledgement was written: the journal is writable again.
        host.journal=null;
        payload={outcomes:host.outcomes,limit:16,pending:0,journal:host.journal};
      } else throw {code:'INTERNAL',message:command};
      sessionStorage.setItem('hostJournal',JSON.stringify(host));
      return {version:1,requestId:request.requestId,payload};
    }}};
  },theme);
  await page.goto('/');
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  const panel=page.getByRole('alert',{name:'Unknown outcomes'});
  await expect(panel).toBeVisible();
  await expect(panel).toContainText('Kept, also across restarts, until you acknowledge it.');
  const problem=panel.locator('.journal-problem');
  await expect(problem).toContainText('The unknown-outcome journal could not be saved (for example, the disk is full).');
  await expect(problem).toContainText('New live writes and transactions are refused');
  await expect(problem).toContainText('read-only queries still work');
  const entries=panel.locator('.unresolved-outcome');
  await expect(entries).toHaveCount(4);
  await expect(entries.nth(0)).toContainText('Before a restart');
  await expect(entries.nth(0)).toContainText('Transaction on MariaDB “shop” was still open when StoneQL quit or crashed.');
  await expect(entries.nth(0)).toContainText('StoneQL never sent COMMIT');
  await expect(entries.nth(0)).toContainText('Recorded 2 h ago, before StoneQL restarted.');
  await expect(entries.nth(1)).toContainText('Before a restart');
  await expect(entries.nth(1)).toContainText('SQL job on PostgreSQL “ledger” was running when StoneQL quit or crashed.');
  await expect(entries.nth(1)).toContainText('Statement 2 of 3 may have been sent (the statements before it were sent); later ones were not.');
  await expect(entries.nth(2)).toContainText('Unknown-outcome journal');
  await expect(entries.nth(2)).toContainText('2 recorded outcomes could not be read and are not listed.');
  await expect(entries.nth(2)).toContainText('kept unchanged as “unknown-outcomes.journal.damaged-1-abcd”');
  await expect(entries.nth(3)).toContainText('Before a reload');
  await expect(entries.nth(3)).toContainText('Transaction on MySQL “shop” was ended.');
  await expect(entries.nth(3)).toContainText('Recorded less than a minute ago.');
  await expect(entries.nth(3)).not.toContainText('before StoneQL restarted');
  for(const index of [0,1,3]){
    await expect(entries.nth(index)).toContainText('Outcome unknown — the server may or may not have applied it.');
    await expect(entries.nth(index)).not.toContainText(/committed|rolled back|succeeded/i);
  }
  const colours=await problem.evaluate((el)=>{const s=getComputedStyle(el);return [s.color,s.backgroundColor,getComputedStyle(document.body).color];});
  expect(colours[0]).not.toBe(colours[1]);
  expect(colours[0]).not.toBe(colours[2]);
  await page.screenshot({path:`test-results/unresolved-restart-${theme}.png`});
  // Acknowledging the damage report hides it; the journal is reported writable again.
  await entries.nth(2).getByRole('button',{name:/^Acknowledge/}).click();
  await expect(entries).toHaveCount(3);
  await expect(problem).toHaveCount(0);
  await page.reload();
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  await expect(entries).toHaveCount(3);
  await expect(panel).not.toContainText('Unknown-outcome journal');
});
