import {test,expect} from '@playwright/test';
// The desktop host refuses a reload while it holds a lease or job and dispatches
// `stoneql-reload-requested`; the renderer confirms in-app and rotates the session before reloading.
for(const theme of ['light','dark']) test(`${theme}: native reload refusal asks in-app and rotates the session only on confirm`,async({page})=>{
  await page.addInitScript((theme)=>{
    localStorage.setItem('stoneql-theme',theme);
    (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
      const request=JSON.parse(args.message);
      if(command==='host_info'){
        const log=JSON.parse(sessionStorage.getItem('hostInfoNonces')||'[]');log.push(request.payload.nonce);sessionStorage.setItem('hostInfoNonces',JSON.stringify(log));
      }
      const payload=command==='host_info'?{runtime:'desktop',appVersion:'0.1.0',sessionId:`session_${request.payload.nonce}`,nonce:request.payload.nonce,simulated:true}
        :command==='list_unresolved_outcomes'?{outcomes:[],limit:16}
        :{simulated:true,values:request.payload.values};
      return {version:1,requestId:request.requestId,payload};
    }}};
  },theme);
  await page.goto('/');
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  const nonces=()=>page.evaluate(()=>JSON.parse(sessionStorage.getItem('hostInfoNonces')||'[]') as string[]);
  expect(await nonces()).toHaveLength(1);
  // The desktop app has no sample workspace; an unsaved new-connection draft is the pending edit.
  await page.getByTitle('Manage connections',{exact:true}).click();
  await page.getByRole('button',{name:'New connection',exact:true}).click();
  await page.getByPlaceholder('e.g. Local development').fill('Unsaved 17');
  await page.evaluate(()=>{(window as any).pageMarker=true;});
  await page.evaluate(()=>window.dispatchEvent(new Event('stoneql-reload-requested')));
  const dialog=page.getByRole('alertdialog');
  await expect(dialog).toContainText('Reloading StoneQL ends it');
  await expect(dialog).toContainText('Unsaved changes are discarded too.');
  await page.screenshot({path:`test-results/native-reload-${theme}.png`});
  await dialog.getByRole('button',{name:'Cancel',exact:true}).click();
  await expect(dialog).toHaveCount(0);
  expect(await nonces()).toHaveLength(1);
  expect(await page.evaluate(()=>(window as any).pageMarker)).toBe(true);
  await expect(page.getByPlaceholder('e.g. Local development')).toHaveValue('Unsaved 17');
  await page.evaluate(()=>window.dispatchEvent(new Event('stoneql-reload-requested')));
  const reloaded=page.waitForEvent('load');
  await dialog.getByRole('button',{name:'Reload StoneQL',exact:true}).click();
  await reloaded;
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  expect(await page.evaluate(()=>(window as any).pageMarker)).toBeUndefined();
  const log=await nonces();
  // Bootstrap, the confirmed rotation (before the page went away), then the new page's bootstrap.
  expect(log).toHaveLength(3);
  expect(log[1]).toMatch(/^reload_/);
});
