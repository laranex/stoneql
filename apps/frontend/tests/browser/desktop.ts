import type { Page } from '@playwright/test';
/**
 * The desktop app has no samples or Mock mode: a new connection is live, with blank username and
 * database. Fills both with the fixture values these specs assert on.
 */
export async function newLiveConnection(page: Page, user = 'root', database = 'acme_store') {
  await page.getByTitle('Manage connections', { exact: true }).click();
  await page.getByRole('button', { name: 'New connection', exact: true }).click();
  await page.getByRole('textbox', { name: 'Username' }).fill(user);
  await page.getByRole('textbox', { name: 'Default database' }).fill(database);
  // New connections remember the password by default; these stubbed hosts have no keychain.
  await page.getByRole('checkbox', { name: 'Remember database password' }).uncheck();
}
