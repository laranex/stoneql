import {test,expect} from '@playwright/test';
test('StoneQL loads legacy theme and sanitized saved profiles through the actual application',async({page})=>{
 await page.addInitScript(()=>{
  localStorage.setItem('dbadmin-theme','dark');
  localStorage.setItem('dbadmin-connections',JSON.stringify([{id:'legacy',name:'Legacy workspace',engine:'MySQL',host:'localhost',port:3306,user:'reader',password:'legacy-secret',database:'acme_store',transport:{type:'direct'}}]));
 });
 await page.goto('/');
 await expect(page).toHaveTitle('StoneQL — StoneAge SQL Admin');
 await expect(page.locator('.bottom-status strong')).toHaveText('StoneQL');
 await expect(page.locator('html')).toHaveClass(/dark/);
 await page.getByTitle('Manage connections',{exact:true}).click();
 await expect(page.locator('.connection-row').filter({hasText:'Legacy workspace'})).toBeVisible();
 const migrated=await page.evaluate(()=>localStorage.getItem('stoneql-connections'));
 expect(migrated).not.toContain('legacy-secret');
 await page.getByRole('button',{name:'Close connection manager',exact:true}).click();
 await page.getByRole('button',{name:'Help',exact:true}).click();
 const about=page.getByRole('dialog',{name:'About StoneQL'});
 await expect(about.getByText('StoneAge SQL Admin',{exact:true})).toBeVisible();
 await expect(about.getByText('Old habits. New engine.',{exact:true})).toBeVisible();
 await page.screenshot({path:'test-results/stoneql-dark-branding.png',fullPage:true});
 await page.getByRole('button',{name:'Close help',exact:true}).click();
 await page.getByRole('button',{name:'Switch to light theme',exact:true}).click();
 await page.getByRole('button',{name:'Help',exact:true}).click();
 await page.screenshot({path:'test-results/stoneql-light-branding.png',fullPage:true});
});
