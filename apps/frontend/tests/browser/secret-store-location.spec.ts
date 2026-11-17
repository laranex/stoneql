import {test,expect} from '@playwright/test';
/**
 * Linux: each saved password's placeholder names the store that holds it (the reference's UUID
 * version: 4 Secret Service, 8 encrypted file), and the note names where a new password goes.
 * Here the database password was saved in the encrypted file before a keyring was running.
 */
for(const theme of ['light','dark'])test(`${theme}: saved-password placeholders name the store holding each password`,async({page})=>{
 await page.addInitScript(theme=>{
  localStorage.setItem('stoneql-theme',theme);
  localStorage.setItem('stoneql-connections',JSON.stringify({version:1,groups:[],connections:[{
   id:'live-1',name:'Stores fixture',mode:'live',engine:'MySQL',host:'db.internal',port:3306,user:'app',database:'shop',
   passwordRef:'00000000-0000-8000-8000-000000000001',
   transport:{type:'ssh',host:'bastion.internal',port:22,user:'deploy',auth:'password',passwordRef:'00000000-0000-4000-8000-000000000002'},
  }]}));
  (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
   const request=JSON.parse(args.message);const p=request.payload;
   let payload:any;
   if(command==='host_info')payload={runtime:'desktop',appVersion:'0.1.0',sessionId:'fixture_session',nonce:p.nonce,simulated:true};
   else if(command==='contract_probe')payload={values:p.values,simulated:true};
   else if(command==='list_unresolved_outcomes')payload={outcomes:[],limit:16};
   else if(command==='secret_store_status')payload={kind:'secret_service',vaultExists:true,unlocked:false};
   else if(command==='ssh_host_key_status')payload={trusted:null};
   else throw{code:'INVALID_REQUEST'};
   return{version:1,requestId:request.requestId,payload};
  }}};
 },theme);
 await page.goto('/');
 await page.getByRole('button',{name:'Connect',exact:true}).click();
 await expect(page.getByRole('dialog',{name:'Connection setup'})).toBeVisible();
 await expect(page.getByLabel('Password',{exact:true})).toHaveAttribute('placeholder','Saved in encrypted file');
 await expect(page.getByLabel('SSH password',{exact:true})).toHaveAttribute('placeholder','Saved in keyring');
 await expect(page.getByTestId('secret-store')).toHaveText('New passwords are saved in the desktop keyring (Secret Service).');
 await expect(page.getByRole('dialog',{name:'Connection setup'})).not.toContainText('Keychain');
 await page.screenshot({path:`test-results/secret-store-location-${theme}.png`});
});
