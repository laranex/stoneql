import {test,expect} from '@playwright/test';
for(const theme of ['light','dark']) test(`${theme}: native close uses shared unsaved dialog and preserves cancelled edits`,async({page})=>{
  await page.addInitScript((theme)=>{
    localStorage.setItem('stoneql-theme',theme);
    (window as any).nativeCloseActions=[];
    (window as any).__TAURI__={core:{invoke:async(command:string,args:{message:string})=>{
      const request=JSON.parse(args.message);
      const payload=command==='host_info'?{runtime:'desktop',appVersion:'0.1.0',sessionId:'native_test_session',nonce:request.payload.nonce,simulated:true}
        :command==='resolve_close'?((window as any).nativeCloseActions.push(request.payload.action),{closed:request.payload.action==='confirm'})
        :command==='list_unresolved_outcomes'?{outcomes:[],limit:16}
        :{simulated:true,values:request.payload.values};
      return {version:1,requestId:request.requestId,payload};
    }}};
  },theme);
  await page.goto('/');
  await expect(page.locator('.footer-session')).toContainText('Not connected');
  // The desktop app has no sample workspace; an unsaved new-connection draft is the pending edit.
  await page.getByTitle('Manage connections',{exact:true}).click();
  await page.getByRole('button',{name:'New connection',exact:true}).click();
  await page.getByPlaceholder('e.g. Local development').fill('Unsaved 13');
  await page.evaluate(()=>window.dispatchEvent(new Event('stoneql-close-requested')));
  const dialog=page.getByRole('alertdialog');
  await expect(dialog).toContainText('Close StoneQL and discard unsaved changes?');
  await page.screenshot({path:`test-results/native-close-${theme}.png`});
  await dialog.getByRole('button',{name:'Cancel',exact:true}).click();
  await expect.poll(()=>page.evaluate(()=>(window as any).nativeCloseActions)).toContain('cancel');
  await expect(page.getByPlaceholder('e.g. Local development')).toHaveValue('Unsaved 13');
  await page.evaluate(()=>window.dispatchEvent(new Event('stoneql-close-requested')));
  await dialog.getByRole('button',{name:'Close StoneQL',exact:true}).click();
  await expect.poll(()=>page.evaluate(()=>(window as any).nativeCloseActions)).toContain('confirm');
});
