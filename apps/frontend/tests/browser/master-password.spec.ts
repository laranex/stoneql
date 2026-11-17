import {test,expect} from '@playwright/test';
/**
 * Linux fallback without a Secret Service: saving a password asks for a new master password
 * (twice, minimum length), the save is retried after unlocking, and the dialog states the store.
 * After a restart the first saved-password use asks to unlock; a wrong password is refused, and a
 * forgotten one can be reset after an explicit confirmation.
 */
for(const theme of ['light','dark'])test(`${theme}: master password create, unlock, wrong password and reset`,async({page})=>{
 await page.addInitScript(theme=>{
  localStorage.setItem('stoneql-theme',theme);
  const w=window as any;
  const state=JSON.parse(sessionStorage.getItem('vault')||'{"exists":false}');
  let unlocked=false,sequence=0;
  w.calls=[];
  const save=()=>sessionStorage.setItem('vault',JSON.stringify(state));
  w.__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
   const request=JSON.parse(args.message);const p=request.payload;w.calls.push({command,payload:p});
   const status=()=>({kind:'encrypted_file',vaultExists:state.exists,unlocked});
   let payload:any;
   if(command==='host_info')payload={runtime:'desktop',appVersion:'0.1.0',sessionId:'fixture_session',nonce:p.nonce,simulated:true};
   else if(command==='contract_probe')payload={values:p.values,simulated:true};
   else if(command==='list_unresolved_outcomes')payload={outcomes:[],limit:16};
   else if(command==='secret_store_status')payload=status();
   else if(command==='unlock_secret_store'){
    if(p.create!==!state.exists)throw{code:'INVALID_REQUEST'};
    if(state.exists&&p.password!==state.password)throw{code:'MASTER_PASSWORD_REJECTED'};
    if(!state.exists){state.exists=true;state.password=p.password;save();}
    unlocked=true;payload=status();
   }else if(command==='reset_secret_store'){state.exists=false;delete state.password;unlocked=false;save();payload=status();}
   else if(command==='store_secret'){if(!unlocked)throw{code:'SECRET_STORE_LOCKED'};payload={reference:`00000000-0000-8000-8000-${String(++sequence).padStart(12,'0')}`};}
   else if(command==='delete_secret')payload={reference:p.reference};
   else if(command==='connect_direct'){if(p.passwordRef&&!unlocked)throw{code:'SECRET_STORE_LOCKED'};payload={handle:'00000000-0000-0000-0000-000000000001',engineVersion:'MySQL fixture 8.4.6',database:p.database,expiresAfterSeconds:3600};}
   else if(command==='disconnect_database')payload={connected:false};
   else if(command==='database_status')payload={connected:true};
   else if(command==='read_only_query')payload={index:0,affectedRows:'0',truncated:false,columns:[{name:'name',databaseType:'VARCHAR'},{name:'kind',databaseType:'VARCHAR'}],rows:[]};
   else throw{code:'INVALID_REQUEST'};
   return{version:1,requestId:request.requestId,payload};
  }}};
 },theme);
 await page.goto('/');
 await page.getByTitle('Manage connections',{exact:true}).click();
 await page.getByRole('button',{name:'New connection',exact:true}).click();
 await page.getByPlaceholder('e.g. Local development').fill('Vault fixture');
 await page.getByRole('textbox',{name:'Username'}).fill('app');
 await page.getByRole('textbox',{name:'Default database'}).fill('shop');
 await page.getByLabel('Password',{exact:true}).fill('synthetic-db-password');
 await expect(page.getByRole('checkbox',{name:'Remember database password'})).toBeChecked();
 await expect(page.getByTestId('secret-store')).toHaveText('New passwords are saved in StoneQL’s encrypted file (master password): no desktop keyring is running.');

 // First save: no vault yet, so the user sets a master password (twice, at least 8 characters).
 await page.getByRole('button',{name:'Save',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'Set a master password'});
 await expect(dialog).toBeVisible();
 await expect(dialog.getByLabel('Master password',{exact:true})).toBeFocused();
 await dialog.getByLabel('Master password',{exact:true}).fill('short');
 await dialog.getByLabel('Repeat master password').fill('short');
 await dialog.getByRole('button',{name:'Create'}).click();
 await expect(dialog.getByRole('status')).toHaveText('Use at least 8 characters.');
 await dialog.getByLabel('Master password',{exact:true}).fill('synthetic-master-1');
 await dialog.getByLabel('Repeat master password').fill('synthetic-master-2');
 await dialog.getByRole('button',{name:'Create'}).click();
 await expect(dialog.getByRole('status')).toHaveText('The two entries do not match.');
 await page.screenshot({path:`test-results/master-password-${theme}-create.png`});
 await dialog.getByLabel('Master password',{exact:true}).fill('synthetic-master-1');
 await dialog.getByLabel('Repeat master password').fill('synthetic-master-1');
 await dialog.getByLabel('Repeat master password').press('Enter');
 await expect(dialog).toHaveCount(0);
 await expect(page.getByRole('dialog',{name:'Connection setup'})).toHaveCount(0);
 const calls=await page.evaluate(()=>(window as any).calls);
 expect(calls.filter((c:any)=>c.command==='store_secret').length).toBe(2);
 expect(calls.find((c:any)=>c.command==='unlock_secret_store').payload).toMatchObject({create:true});
 const serialized=await page.evaluate(()=>localStorage.getItem('stoneql-connections')!);
 expect(serialized).toContain('"passwordRef":"00000000-0000-8000-8000-000000000001"');
 for(const secret of ['synthetic-master-1','synthetic-db-password'])expect(serialized).not.toContain(secret);

 // A restart: the vault exists but is locked. Connecting with the saved password asks to unlock.
 await page.reload();
 await page.getByRole('button',{name:'Connect',exact:true}).click();
 await expect(page.getByLabel('Password',{exact:true})).toHaveAttribute('placeholder','Saved in encrypted file');
 await page.getByRole('button',{name:'Test connection',exact:true}).click();
 const unlock=page.getByRole('dialog',{name:'Unlock saved passwords'});
 await expect(unlock).toBeVisible();
 await expect(unlock.getByLabel('Repeat master password')).toHaveCount(0);
 await unlock.getByLabel('Master password',{exact:true}).fill('synthetic-wrong-pass');
 await unlock.getByRole('button',{name:'Unlock'}).click();
 await expect(unlock.getByRole('alert')).toHaveText('Incorrect master password. Try again.');
 await expect(unlock.getByLabel('Master password',{exact:true})).toHaveValue('');
 await page.screenshot({path:`test-results/master-password-${theme}-wrong.png`});
 // Escape cancels: nothing connects and the vault stays locked.
 await page.keyboard.press('Escape');
 await expect(unlock).toHaveCount(0);
 await expect(page.getByText('Saved passwords are locked. Enter the master password to use them.')).toBeVisible();
 await page.getByRole('button',{name:'Test connection',exact:true}).click();
 await expect(unlock).toBeVisible();

 // Forgotten password: reset only after a confirmation that states the loss.
 await unlock.getByRole('button',{name:'Reset saved passwords…'}).click();
 const confirm=page.getByRole('alertdialog',{name:'Reset saved passwords?'});
 await expect(confirm).toContainText('Every password saved in it is lost');
 await page.screenshot({path:`test-results/master-password-${theme}-reset.png`});
 await confirm.getByRole('button',{name:'Delete saved passwords'}).click();
 await expect(page.getByRole('dialog',{name:'Set a master password'})).toBeVisible();
 expect((await page.evaluate(()=>(window as any).calls)).some((c:any)=>c.command==='reset_secret_store')).toBe(true);
});
