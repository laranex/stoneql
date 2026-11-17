import {test,expect} from '@playwright/test';
import { newLiveConnection } from './desktop';
for(const theme of ['light','dark'])test(`${theme}: explicit native live connection, exact results, errors, disconnect and persistence`,async({page})=>{
 await page.addInitScript(theme=>{
  localStorage.setItem('stoneql-theme',theme);
  let sequence=0;const handles=new Set<string>();
  (window as any).liveCommands=[];
  (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
   const request=JSON.parse(args.message);const p=request.payload;(window as any).liveCommands.push(command);
   let payload:any;
   if(command==='host_info')payload={runtime:'desktop',appVersion:'0.1.0',sessionId:'fixture_session',nonce:p.nonce,simulated:true,sqlite:true};
   else if(command==='contract_probe')payload={values:p.values,simulated:true};
   else if(command==='list_unresolved_outcomes')payload={outcomes:[],limit:16};
   else if(command==='connect_direct'){
    const handle=`00000000-0000-0000-0000-${String(++sequence).padStart(12,'0')}`;handles.add(handle);
    payload={handle,engineVersion:'MySQL fixture 8.4.6',database:p.database,expiresAfterSeconds:3600};
   }else if(command==='disconnect_database'){handles.delete(p.handle);payload={connected:false};}
   else if(command==='read_only_query'){
    if(!handles.has(p.handle))throw{code:'FORBIDDEN'};
    if(p.sql.includes('UPDATE'))throw{code:'UNSUPPORTED_SQL'};
    if(p.sql.includes('information_schema'))payload={index:0,affectedRows:'0',truncated:false,columns:[{name:'name',databaseType:'VARCHAR'},{name:'kind',databaseType:'VARCHAR'}],rows:[]};else
    payload={index:0,affectedRows:'0',truncated:p.sql.includes('bounded'),columns:[{name:'value',databaseType:'BIGINT UNSIGNED'},{name:'value',databaseType:'DECIMAL'},{name:'n',databaseType:'VARCHAR'}],rows:[[{kind:'integer',text:p.handle.endsWith('000000000002')?'18446744073709551615':'9223372036854775807'},{kind:'decimal',text:'12345678901234567890.4500'},{kind:'null'}]]};
   }else if(command==='database_status')payload={connected:handles.has(p.handle)};
   else throw{code:'INVALID_REQUEST'};
   return{version:1,requestId:request.requestId,payload};
  }}};
 },theme);
 await page.goto('/');
 await newLiveConnection(page);
 await page.getByPlaceholder('e.g. Local development').fill('Live fixture');
 await page.getByLabel('Password',{exact:true}).fill('synthetic-password');
 // A new connection to this computer starts with TLS off; this one verifies TLS with a session CA.
 await page.getByTestId('tls-option').getByRole('checkbox',{name:'Use TLS'}).check();
 await page.getByLabel('Trusted CA certificate',{exact:true}).setInputFiles({name:'ca.pem',mimeType:'application/x-pem-file',buffer:Buffer.from('-----BEGIN CERTIFICATE-----\nsynthetic-public-root\n-----END CERTIFICATE-----')});
 await expect(page.getByText('CA certificate loaded for this session.',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Test connection',exact:true}).click();
 await expect(page.getByText(/Test session closed/)).toBeVisible();
 await page.getByRole('button',{name:'Save & connect',exact:true}).click();
 await expect(page.getByRole('dialog',{name:'Connection setup'})).toHaveCount(0);
 await expect(page.locator('.footer-session')).toContainText('Live read-only · connected');
 // Database level: SQL and a whole-database Export (Import needs a read-write connection).
 await expect(page.locator('.view-tabs [role=tab]')).toHaveText(['SQL','Export']);
 await expect(page.locator('.table-tree button')).toHaveCount(0);
 await expect(page.locator('.database-context-link')).toContainText('acme_store');
 await expect(page.getByText('No matching tables',{exact:true})).toHaveCount(0);
 await page.locator('.cm-content').fill('SELECT value,value,n FROM known_table');
 // Ctrl/Cmd+Enter is CodeMirror's Mod-Enter on every platform; the editor must run, not add a line.
 await page.locator('.cm-content').press('ControlOrMeta+Enter');
 await expect(page.getByText('18446744073709551615',{exact:true})).toBeVisible();
 await expect(page.getByText('12345678901234567890.4500',{exact:true})).toBeVisible();
 await expect(page.locator('.data-grid thead th')).toHaveText(['value','value','n']);
 await expect(page.getByText(/executed successfully \(live, read-only/)).toBeVisible();
 await page.screenshot({path:`test-results/live-${theme}-results.png`,fullPage:true});
 await newLiveConnection(page);
 await page.getByPlaceholder('e.g. Local development').fill('Live peer');
 await page.getByRole('button',{name:'Save & connect',exact:true}).click();
 await expect(page.getByText('18446744073709551615',{exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'Run query',exact:true}).click();
 await expect(page.getByText('9223372036854775807',{exact:true})).toBeVisible();
 await page.getByRole('tab',{name:/Live fixture/}).click();
 await expect(page.getByText('18446744073709551615',{exact:true})).toBeVisible();
 await expect(page.getByText('9223372036854775807',{exact:true})).toHaveCount(0);

 await page.locator('.cm-content').fill('SELECT bounded FROM known_table');
 await page.getByRole('button',{name:'Run query',exact:true}).click();
 await expect(page.getByText(/Result limited to 1,000 rows/)).toBeVisible();
 await page.locator('.cm-content').fill('UPDATE known_table SET value=1');
 await page.getByRole('button',{name:'Run query',exact:true}).click();
 await expect(page.getByText('This read-only SQL form is not supported.',{exact:true})).toBeVisible();
 await expect(page.getByText('18446744073709551615',{exact:true})).toHaveCount(0);
 await page.getByRole('button',{name:'Disconnect',exact:true}).click();
 await expect(page.getByRole('button',{name:'Run query',exact:true})).toBeDisabled();
 await page.getByRole('tab',{name:/Live peer/}).click();
 await expect(page.locator('.footer-session')).toContainText('connected');
 await expect(page.getByText('9223372036854775807',{exact:true})).toBeVisible();
 await page.getByRole('tab',{name:/Live fixture/}).click();
 const serialized=await page.evaluate(()=>localStorage.getItem('stoneql-connections')!);
 expect(serialized).not.toContain('synthetic-password');expect(serialized).not.toContain('synthetic-public-root');expect(serialized).toContain('"mode":"live"');
 await page.reload();
 expect(await page.evaluate(()=>(window as any).liveCommands)).not.toContain('connect_direct');
 await page.getByTitle('Manage connections',{exact:true}).click();
 await page.locator('.connection-row').filter({hasText:'Live fixture'}).getByRole('button',{name:'Open'}).click();
 await expect(page.locator('.footer-session')).toContainText('Live read-only · disconnected');
 await expect(page.getByRole('button',{name:'Run query',exact:true})).toBeDisabled();
 await page.getByRole('button',{name:'Connect',exact:true}).click();
 await expect(page.getByLabel('Password',{exact:true})).toHaveValue('');
 await page.screenshot({path:`test-results/live-${theme}-connection.png`,fullPage:true});
 await page.setViewportSize({width:800,height:600});
 await expect(page.getByRole('button',{name:'Save & connect',exact:true})).toBeInViewport();
 await expect(page.getByRole('button',{name:'Cancel',exact:true})).toBeInViewport();
});
for(const theme of ['light','dark'])test(`${theme}: live SQLite opens a natively granted file without exposing a path`,async({page})=>{
 await page.addInitScript(theme=>{
  localStorage.setItem('stoneql-theme',theme);
  (window as any).liveCalls=[];
  (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
   const request=JSON.parse(args.message);const p=request.payload;(window as any).liveCalls.push({command,payload:p});
   let payload:any;
   if(command==='host_info')payload={runtime:'desktop',appVersion:'0.1.0',sessionId:'fixture_session',nonce:p.nonce,simulated:true,sqlite:true};
   else if(command==='contract_probe')payload={values:p.values,simulated:true};
   else if(command==='list_unresolved_outcomes')payload={outcomes:[],limit:16};
   else if(command==='pick_sqlite_file')payload={grant:'opaque_fixture_grant',fileName:'inventory.sqlite'};
   else if(command==='connect_sqlite'){if(p.grant!=='opaque_fixture_grant')throw{code:'FORBIDDEN'};payload={handle:'00000000-0000-0000-0000-000000000009',engineVersion:'SQLite 3.51.3',database:'inventory.sqlite',expiresAfterSeconds:3600};}
   else if(command==='disconnect_database')payload={connected:false};
   else if(command==='database_status')payload={connected:true};
   else if(command==='read_only_query'){
    if(!p.sql.startsWith('SELECT'))throw{code:'UNSUPPORTED_SQL'};
    payload={index:0,affectedRows:'0',truncated:false,columns:[{name:'stored',databaseType:'INTEGER'},{name:'stored',databaseType:'NULL'}],rows:[[{kind:'text',text:'not a number'},{kind:'integer',text:'-9223372036854775808'}]]};
   }else throw{code:'INVALID_REQUEST'};
   return{version:1,requestId:request.requestId,payload};
  }}};
 },theme);
 await page.goto('/');
 await newLiveConnection(page);
 await page.getByLabel('Database engine',{exact:true}).selectOption('SQLite');
 await page.getByPlaceholder('e.g. Local development').fill('Live file');
 await expect(page.getByLabel('Trusted CA certificate',{exact:true})).toHaveCount(0);
 await expect(page.getByLabel('Host',{exact:true})).toHaveCount(0);
 await expect(page.getByPlaceholder('/path/to/database.sqlite')).toHaveCount(0);
 await expect(page.getByText('No file chosen for this app session',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Choose file…',exact:true}).click();
 await expect(page.getByText('inventory.sqlite',{exact:true})).toBeVisible();
 await page.screenshot({path:`test-results/live-sqlite-${theme}-dialog.png`,fullPage:true});
 await page.getByRole('button',{name:'Save & connect',exact:true}).click();
 await expect(page.getByRole('dialog',{name:'Connection setup'})).toHaveCount(0);
 await expect(page.locator('.footer-session')).toContainText('Live read-only · connected');
 await expect(page.getByText('SQLite 3.51.3',{exact:true})).toBeVisible();
 await expect(page.locator('.view-tabs [role=tab]')).toHaveText('SQL');
 await page.locator('.cm-content').fill('SELECT stored, stored FROM affinity');
 await page.getByRole('button',{name:'Run query',exact:true}).click();
 await expect(page.getByText('not a number',{exact:true})).toBeVisible();
 await expect(page.getByText('-9223372036854775808',{exact:true})).toBeVisible();
 await page.screenshot({path:`test-results/live-sqlite-${theme}-results.png`,fullPage:true});
 const calls=await page.evaluate(()=>(window as any).liveCalls);
 expect(calls.find((call:any)=>call.command==='connect_sqlite').payload).toEqual({grant:'opaque_fixture_grant',sessionId:'fixture_session'});
 expect(JSON.stringify(calls)).not.toContain('/');
 const serialized=await page.evaluate(()=>localStorage.getItem('stoneql-connections')!);
 expect(serialized).toContain('inventory.sqlite');expect(serialized).not.toContain('opaque_fixture_grant');
});
for(const theme of ['light','dark'])test(`${theme}: live SSH requires explicit host-key trust, keeps secrets out of profiles and refuses a changed key`,async({page})=>{
 await page.addInitScript(theme=>{
  localStorage.setItem('stoneql-theme',theme);
  (window as any).liveCalls=[];let references=0;(window as any).rotateHostKey=false;let enrolled=false;
  const fingerprint='SHA256:9Xl0yQZzJ8m2kKpz1bq5N2f3S5gkT0r9u1g6vXy3K0M';
  (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
   const request=JSON.parse(args.message);const p=request.payload;(window as any).liveCalls.push({command,payload:p});
   let payload:any;
   if(command==='host_info')payload={runtime:'desktop',appVersion:'0.1.0',sessionId:'fixture_session',nonce:p.nonce,simulated:true,sqlite:true};
   else if(command==='contract_probe')payload={values:p.values,simulated:true};
   else if(command==='list_unresolved_outcomes')payload={outcomes:[],limit:16};
   else if(command==='ssh_host_key')payload={algorithm:'ssh-ed25519',publicKey:'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl',fingerprint:'SHA256:9Xl0yQZzJ8m2kKpz1bq5N2f3S5gkT0r9u1g6vXy3K0M'};
   else if(command==='ssh_host_key_status')payload={trusted:enrolled?{algorithm:'ssh-ed25519',fingerprint}:null};
   else if(command==='trust_ssh_host_key'){if(p.fingerprint!==fingerprint)throw{code:'INVALID_REQUEST'};enrolled=true;payload={trusted:{algorithm:'ssh-ed25519',fingerprint}};}
   else if(command==='forget_ssh_host_key'){enrolled=false;payload={trusted:null};}
   else if(command==='store_secret')payload={reference:`00000000-0000-4000-8000-${String(++references).padStart(12,'0')}`};
   else if(command==='delete_secret')payload={reference:p.reference};
   else if(command==='connect_direct'){
    if(p.ssh&&'hostKey' in p.ssh)throw{code:'INVALID_REQUEST'};
    if(p.ssh&&!enrolled)throw{code:'SSH_HOST_KEY_UNKNOWN'};
    if((window as any).rotateHostKey)throw{code:'SSH_HOST_KEY_CHANGED'};
    payload={handle:'00000000-0000-0000-0000-000000000005',engineVersion:'MySQL fixture 8.4.6',database:p.database,expiresAfterSeconds:3600};
   }else if(command==='disconnect_database')payload={connected:false};
   else if(command==='database_status')payload={connected:true};
   else throw{code:'INVALID_REQUEST'};
   return{version:1,requestId:request.requestId,payload};
  }}};
 },theme);
 await page.goto('/');
 await newLiveConnection(page);
 await page.getByPlaceholder('e.g. Local development').fill('Live bastion');
 await page.getByLabel('Password',{exact:true}).fill('synthetic-db-password');
 await page.getByRole('button',{name:'SSH Tunnel'}).click();
 await page.getByPlaceholder('bastion.example.com').fill('bastion.internal');
 await page.getByLabel('SSH username',{exact:true}).fill('deploy');
 await page.getByLabel('SSH authentication',{exact:true}).selectOption('password');
 await page.getByLabel('SSH password',{exact:true}).fill('synthetic-ssh-password');
 await page.getByLabel('Remember SSH password').check();
 await expect(page.getByText('Not enrolled · connections are refused until you verify and trust the key')).toBeVisible();
 await page.getByRole('button',{name:'Test connection',exact:true}).click();
 const check=page.getByRole('group',{name:'Verify SSH host key'});
 await expect(check).toContainText('SHA256:9Xl0yQZzJ8m2kKpz1bq5N2f3S5gkT0r9u1g6vXy3K0M');
 await expect(page.getByText('Verify and trust the SSH host key, then test again.')).toBeVisible();
 expect((await page.evaluate(()=>(window as any).liveCalls)).some((c:any)=>c.command==='connect_direct')).toBe(false);
 await page.screenshot({path:`test-results/live-ssh-${theme}-host-key.png`,fullPage:true});
 await check.getByRole('button',{name:'Trust this host key'}).click();
 await expect(page.getByText(/^Trusted · SHA256:9Xl0/)).toBeVisible();
 await page.getByRole('button',{name:'Test connection',exact:true}).click();
 await expect(page.getByText(/Verified SSH host key and TLS connection successful/)).toBeVisible();
 await page.getByRole('button',{name:'Save & connect',exact:true}).click();
 await expect(page.getByRole('dialog',{name:'Connection setup'})).toHaveCount(0);
 await expect(page.locator('.footer-session')).toContainText('Live read-only · connected');
 const calls=await page.evaluate(()=>(window as any).liveCalls);
 const connect=calls.filter((c:any)=>c.command==='connect_direct').at(-1).payload;
 expect(connect.ssh).toEqual({host:'bastion.internal',port:22,username:'deploy',auth:{method:'password',password:'synthetic-ssh-password'}});
 expect(calls.find((c:any)=>c.command==='trust_ssh_host_key').payload).toEqual({host:'bastion.internal',port:22,fingerprint:'SHA256:9Xl0yQZzJ8m2kKpz1bq5N2f3S5gkT0r9u1g6vXy3K0M',sessionId:'fixture_session'});
 expect(calls.filter((c:any)=>c.command==='store_secret').map((c:any)=>c.payload.purpose)).toEqual(['ssh_password']);
 const serialized=await page.evaluate(()=>localStorage.getItem('stoneql-connections')!);
 for(const secret of ['synthetic-db-password','synthetic-ssh-password','remember'])expect(serialized).not.toContain(secret);
 expect(serialized).toContain('"passwordRef":"00000000-0000-4000-8000-000000000001"');
 // F-01: host-key trust lives in the native store only.
 expect(serialized).not.toContain('SHA256:9Xl0yQZzJ8m2kKpz1bq5N2f3S5gkT0r9u1g6vXy3K0M');
 expect(serialized).not.toContain('hostKey');
 // The server key rotates: a hard failure; the dialog never offers to trust the new key.
 await page.evaluate(()=>{(window as any).rotateHostKey=true;});
 await page.getByRole('button',{name:'Disconnect',exact:true}).click();
 await page.getByRole('button',{name:'Connect',exact:true}).click();
 await expect(page.getByLabel('SSH password',{exact:true})).toHaveValue('');
 await expect(page.getByLabel('SSH password',{exact:true})).toHaveAttribute('placeholder','Saved in Keychain');
 await page.getByLabel('Password',{exact:true}).fill('synthetic-db-password');
 await page.getByRole('button',{name:'Test connection',exact:true}).click();
 await expect(page.getByText(/SSH host key changed — connection refused/)).toBeVisible();
 await expect(page.getByRole('button',{name:'Trust this host key'})).toHaveCount(0);
 await expect(page.getByRole('button',{name:'Forget host key',exact:true})).toBeVisible();
 await page.screenshot({path:`test-results/live-ssh-${theme}-changed.png`,fullPage:true});
 await page.setViewportSize({width:800,height:600});
 await expect(page.getByRole('button',{name:'Save & connect',exact:true})).toBeInViewport();
 await expect(page.getByRole('button',{name:'Cancel',exact:true})).toBeInViewport();
});
