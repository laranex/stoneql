import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createPinia } from 'pinia';
import { nextTick } from 'vue';
import {
  hostRenderer,
  hostNode,
  findAll,
  textContent,
  loadVue,
  loadModule,
} from './helpers/vueHost.ts';
const preferences = new Map<string, string>();
let osDark = true;
const listeners = new Set<(event: any) => void>();
const classes = new Set<string>();
Object.assign(globalThis, {
  Document: class {},
  ShadowRoot: class {},
  requestAnimationFrame: (fn: any) => setTimeout(fn, 0),
  getComputedStyle: () => ({
    transitionDelay: '0s',
    transitionDuration: '0s',
    animationDelay: '0s',
    animationDuration: '0s',
  }),
  localStorage: {
    getItem: (key: string) => preferences.get(key) ?? null,
    setItem: (key: string, value: string) => preferences.set(key, value),
  },
  window: {
    getComputedStyle: () => ({
      transitionDelay: '0s',
      transitionDuration: '0s',
      animationDelay: '0s',
      animationDuration: '0s',
    }),
    innerHeight: 1000,
    confirm: () => true,
    addEventListener() {},
    removeEventListener() {},
    matchMedia: () => ({
      get matches() {
        return osDark;
      },
      addEventListener(_name: string, fn: any) {
        listeners.add(fn);
      },
      removeEventListener(_name: string, fn: any) {
        listeners.delete(fn);
      },
    }),
  },
  document: {
    body: { offsetHeight: 0, style: {} },
    documentElement: {
      style: {},
      classList: {
        toggle: (name: string, on: boolean) => (on ? classes.add(name) : classes.delete(name)),
      },
    },
  },
});
const App = await loadVue('../../src/App.vue');
test('actual Structure Add/Change handlers render the new dialog and save VARCHAR, DECIMAL, and AFTER ordering', async () => {
  const root = hostNode('root'),
    app = hostRenderer.createApp(App);
  app.use(createPinia());
  app.mount(root);
  const find = (predicate: any) => findAll(root, predicate)[0]!;
  const button = (label: string) => {
    const n = find((n: any) => n.type === 'button' && textContent(n).trim() === label);
    assert.ok(n, `button ${label}`);
    return n;
  };
  const click = async (n: any) => {
    await n.props.onClick?.({ preventDefault() {}, stopPropagation() {} });
    await nextTick();
  };
  const set = async (label: string, value: any) => {
    const n = find((n: any) => n.props['aria-label'] === label);
    assert.ok(n, `field ${label}`);
    n.props['onUpdate:modelValue'](value);
    n.props.onChange?.({ target: n });
    await nextTick();
  };
  const dialog = () =>
    find(
      (n: any) =>
        n.props.role === 'dialog' && ['Add column', 'Edit column'].includes(n.props['aria-label']),
    );
  const save = async () => {
    const form = findAll(dialog(), (n) => n.type === 'form')[0]!;
    form.props.onSubmit({ preventDefault() {} });
    await nextTick();
  };
  await click(button('Structure'));
  await click(button('Add column'));
  assert.equal(dialog().props['aria-label'], 'Add column');
  const options = findAll(dialog(), (n) => n.type === 'option').map(textContent);
  assert.ok(options.includes('VARCHAR'));
  assert.ok(!options.includes('varchar(255)'));
  await set('Column name', 'nickname');
  await set('Data type', 'VARCHAR');
  await set('Length', 100);
  await set('Column position', 'after');
  await set('After column', 'name');
  await save();
  assert.equal(dialog(), undefined);
  let grid = find((n) => n.type === 'table' && n.props.class?.includes('structure-grid'));
  let rows = findAll(grid, (n) => n.type === 'tr');
  assert.ok(textContent(rows[3]!).includes('nickname'));
  assert.ok(textContent(rows[3]!).includes('varchar(100)'));
  const change = findAll(rows[3]!, (n) => n.type === 'button' && textContent(n) === 'Change')[0]!;
  await click(change);
  assert.equal(dialog().props['aria-label'], 'Edit column');
  assert.equal(find((n) => n.props['aria-label'] === 'Length').value, 100);
  await set('Length', 120);
  await save();
  grid = find((n) => n.type === 'table' && n.props.class?.includes('structure-grid'));
  assert.ok(textContent(grid).includes('varchar(120)'));
  await click(button('Add column'));
  await set('Column name', 'amount');
  await set('Data type', 'DECIMAL');
  await set('Precision', 12);
  await set('Scale', 2);
  await set('Column position', 'after');
  await set('After column', 'nickname');
  await save();
  grid = find((n) => n.type === 'table' && n.props.class?.includes('structure-grid'));
  rows = findAll(grid, (n) => n.type === 'tr');
  assert.ok(textContent(rows[4]!).includes('amount'));
  assert.ok(textContent(rows[4]!).includes('decimal(12,2)'));
  app.unmount();
});
test('first launch resolves OS theme, footer toggle persists manual preference, and Settings has no Appearance section', async () => {
  preferences.clear();
  classes.clear();
  osDark = true;
  const root = hostNode('root'),
    app = hostRenderer.createApp(App);
  app.use(createPinia());
  app.mount(root);
  let toggle = findAll(root, (n) => n.props['aria-label'] === 'Switch to light theme')[0]!;
  assert.ok(toggle);
  assert.ok(classes.has('dark'));
  assert.equal(preferences.has('dbadmin-theme'), false);
  toggle.props.onClick();
  await nextTick();
  assert.equal(preferences.get('dbadmin-theme'), 'light');
  assert.equal(classes.has('dark'), false);
  osDark = true;
  listeners.forEach((fn) => fn({ matches: true }));
  await nextTick();
  assert.equal(classes.has('dark'), false);
  const settings = findAll(root, (n) => n.type === 'button' && n.props.title === 'Settings')[0]!;
  settings.props.onClick();
  await nextTick();
  const dialog = findAll(
    root,
    (n) => n.props.role === 'dialog' && n.props['aria-label'] === 'Settings',
  )[0]!;
  assert.ok(dialog);
  assert.ok(!textContent(dialog).includes('Appearance'));
  assert.ok(textContent(dialog).includes('Default rows per page'));
  assert.ok(textContent(dialog).includes('Enable keyboard shortcuts'));
  app.unmount();
  const reopened = hostRenderer.createApp(App);
  reopened.use(createPinia());
  const nextRoot = hostNode('root');
  reopened.mount(nextRoot);
  assert.equal(classes.has('dark'), false);
  assert.ok(findAll(nextRoot, (n) => n.props['aria-label'] === 'Switch to dark theme').length);
  reopened.unmount();
});

test('active Connection Manager creates, renames, moves and deletes groups while retaining connections and workspaces', async () => {
  preferences.clear();
  const pinia = createPinia(),
    root = hostNode('root'),
    app = hostRenderer.createApp(App);
  app.use(pinia);
  app.mount(root);
  const { useWorkspace } = await loadModule('../../src/stores/workspace.ts');
  const store = useWorkspace(pinia);
  const click = async (n: any) => {
    assert.ok(n);
    await n.props.onClick?.({ preventDefault() {} });
    await nextTick();
  };
  const named = (label: string) => findAll(root, (n) => n.props['aria-label'] === label)[0]!;
  const button = (label: string) =>
    findAll(root, (n) => n.type === 'button' && textContent(n).trim() === label)[0]!;
  await click(findAll(root, (n) => n.type === 'button' && n.props.class === 'subtle')[0]);
  await click(button('New group'));
  named('Group name').props['onUpdate:modelValue']('Project Alpha');
  await nextTick();
  findAll(root, (n) => n.type === 'form' && n.props.class === 'group-editor')[0]!.props.onSubmit({
    preventDefault() {},
  });
  await nextTick();
  assert.equal(store.groups[0].name, 'Project Alpha');
  const id = store.groups[0].id;
  named('Move Local development to group').props.onChange({ target: { value: id } });
  await nextTick();
  assert.equal(store.connections[0].groupId, id);
  const workspace = store.workspace;
  const rows = workspace.databases[0].tables[0].rows;
  await click(named('Rename group Project Alpha'));
  named('Group name').props['onUpdate:modelValue']('Project Renamed');
  await nextTick();
  findAll(root, (n) => n.type === 'form' && n.props.class === 'group-editor')[0]!.props.onSubmit({
    preventDefault() {},
  });
  await nextTick();
  assert.equal(store.groups[0].name, 'Project Renamed');
  store.createGroup('Other');
  store.reorderGroup(id, 1);
  store.moveConnection('staging', id);
  store.reorderConnection('staging', -1);
  store.connections[0].color = '#8e7ba1';
  store.connections[0].environment = 'Production';
  store.connections[0].password = 'not-persisted';
  await nextTick();
  const saved = JSON.parse(preferences.get('dbadmin-connections')!);
  assert.equal(saved.connections[0].id, 'staging');
  assert.equal(saved.connections[0].color, '#8e7ba1');
  assert.equal(saved.connections[0].environment, 'Production');
  assert.ok(!preferences.get('dbadmin-connections')!.includes('not-persisted'));
  await click(named('Delete group Project Renamed'));
  await click(button('Delete group'));
  assert.equal(store.connections.length, 2);
  assert.ok(store.connections.every((c: any) => c.groupId === ''));
  assert.equal(store.workspace, workspace);
  assert.equal(store.workspace.databases[0].tables[0].rows, rows);
  const reloaded = useWorkspace(createPinia());
  assert.equal(reloaded.connections.length, 2);
  assert.equal(reloaded.connections[0].id, 'staging');
  assert.equal(reloaded.connections[0].color, '#8e7ba1');
  assert.equal(reloaded.groups[0].name, 'Other');
  app.unmount();
});
for (const theme of ['light', 'dark'])
  test(`${theme}: Explorer status has no redundant connection action and keys have no empty placeholders`, async () => {
    preferences.clear();
    preferences.set('dbadmin-theme', theme);
    const root = hostNode('root'),
      app = hostRenderer.createApp(App);
    app.use(createPinia());
    app.mount(root);
    const status = findAll(root, (n) => n.props.class === 'sidebar-bottom')[0]!;
    assert.ok(textContent(status).includes('Connected'));
    assert.ok(textContent(status).includes('MySQL 8.0.36'));
    assert.equal(findAll(status, (n) => n.type === 'button').length, 0);
    assert.ok(
      findAll(root, (n) => n.type === 'button' && n.props.title === 'Manage connections').length,
    );
    const structure = findAll(
      root,
      (n) => n.type === 'button' && textContent(n).trim() === 'Structure',
    )[0]!;
    await structure.props.onClick();
    await nextTick();
    const grid = findAll(
      root,
      (n) => n.type === 'table' && n.props.class?.includes('structure-grid'),
    )[0]!;
    const rows = findAll(grid, (n) => n.type === 'tr');
    assert.equal(
      findAll(
        rows[1]!,
        (n) => n.props.role === 'img' && String(n.props['aria-label']).startsWith('Primary key'),
      ).length,
      1,
    );
    assert.equal(findAll(rows[2]!, (n) => n.props.role === 'img').length, 0);
    assert.equal(
      findAll(
        rows[2]!,
        (n) => n.props.class === 'column-keys' || n.props.class === 'schema-icon-slot',
      ).length,
      0,
    );
    assert.equal(classes.has('dark'), theme === 'dark');
    app.unmount();
  });
test('actual Insert, Edit, and inline controls share schema validation and preserve ENUM/SET values', async () => {
  preferences.clear();
  preferences.set('dbadmin-theme', 'light');
  const pinia = createPinia(),
    root = hostNode('root'),
    app = hostRenderer.createApp(App);
  app.use(pinia);
  app.mount(root);
  const { useWorkspace } = await loadModule('../../src/stores/workspace.ts');
  const store = useWorkspace(pinia);
  const named = (label: string) => findAll(root, (n) => n.props['aria-label'] === label)[0]!;
  const button = (label: string) =>
    findAll(root, (n) => n.type === 'button' && textContent(n).trim() === label)[0]!;
  const click = async (n: any) => {
    assert.ok(n);
    await n.props.onClick?.({ preventDefault() {} });
    await nextTick();
  };
  const set = async (label: string, value: any) => {
    const n = named(label);
    assert.ok(n, label);
    if (n.props['onUpdate:modelValue']) n.props['onUpdate:modelValue'](value);
    else
      n.props.onChange({
        target: { value, selectedOptions: value.split(',').map((value: string) => ({ value })) },
      });
    await nextTick();
  };
  const submit = async () => {
    findAll(root, (n) => n.type === 'form' && n.props.class === 'record-form')[0]!.props.onSubmit({
      preventDefault() {},
    });
    await nextTick();
  };
  await click(button('Insert'));
  await set('Value for name', 'x'.repeat(256));
  await set('Value for email', 'validation@example.com');
  await submit();
  assert.equal(named('Value for name').props['aria-invalid'], true);
  assert.equal(store.table.rows.length, 128);
  await set('Value for name', 'Precise input');
  const form = findAll(root, (n) => n.type === 'form' && n.props.class === 'record-form')[0]!;
  const statusRow = findAll(form, (n) => n.type === 'tr').find(
    (n) => findAll(n, (x) => x.props.title === 'status').length,
  )!;
  const defaultInput = findAll(
    statusRow,
    (n) => n.type === 'input' && n.type === 'input' && n.props.type === 'checkbox',
  )[0]!;
  defaultInput.props['onUpdate:modelValue'](false);
  defaultInput.props.onChange?.({});
  await nextTick();
  await set('Value for status', 'inactive');
  const nullable = named('NULL for permissions');
  nullable.props['onUpdate:modelValue'](true);
  nullable.props.onChange?.({});
  await nextTick();
  await submit();
  assert.equal(store.table.rows.length, 129);
  assert.equal(store.table.rows.at(-1).status, 'inactive');
  assert.equal(store.table.rows.at(-1).permissions, null);
  function gridRow() {
    const grid = findAll(
      root,
      (n) => n.type === 'table' && String(n.props.class).includes('browse-grid'),
    )[0]!;
    return findAll(grid, (n) => n.type === 'tr')[1]!;
  }
  let row = gridRow(),
    cells = row.children.filter((n) => n.type === 'td');
  assert.equal(findAll(cells[5]!, (n) => n.props.class === 'enum-values').length, 0);
  assert.equal(findAll(cells[6]!, (n) => n.props.class === 'enum-values').length, 1);
  await cells[6]!.props.onDblclick();
  await nextTick();
  await set('Edit status', 'unknown');
  await click(named('Save cell edit'));
  assert.equal(store.table.rows[0].status, 'active');
  assert.ok(findAll(root, (n) => n.props.id === 'inline-value-error').length);
  await set('Edit status', 'inactive');
  await click(named('Save cell edit'));
  assert.equal(store.table.rows[0].status, 'inactive');
  store.table.columns.find((c: any) => c.name === 'status').type = "enum('active','suspended')";
  await nextTick();
  assert.equal(store.table.rows[0].status, 'inactive');
  row = gridRow();
  await click(findAll(row, (n) => n.type === 'button' && textContent(n) === 'Edit')[0]);
  assert.equal(named('Value for status').value, 'inactive');
  assert.ok(textContent(named('Value for status')).includes('not in current schema'));
  await submit();
  assert.ok(findAll(root, (n) => n.props.id === 'record-error-status').length);
  await set('Value for status', 'active');
  await submit();
  assert.equal(store.table.rows[0].status, 'active');
  app.unmount();
});
test('C1 row action labels exist and C4 Quick CSV has a direct policy-preserving recovery', async () => {
  preferences.clear();
  const root = hostNode('root'),
    app = hostRenderer.createApp(App);
  app.use(createPinia());
  app.mount(root);
  for (const label of ['Edit record 1', 'Copy record 1', 'Delete record 1'])
    assert.ok(findAll(root, (n) => n.props['aria-label'] === label).length);
  const button = (label: string) =>
    findAll(root, (n) => n.type === 'button' && textContent(n).trim() === label)[0]!;
  await button('Export').props.onClick();
  await nextTick();
  const format = findAll(root, (n) => n.props['aria-label'] === 'Export format')[0]!;
  format.props['onUpdate:modelValue']('CSV');
  await nextTick();
  const recovery = button('Use Custom CSV export');
  assert.ok(recovery);
  recovery.props.onClick();
  await nextTick();
  assert.equal(findAll(root, (n) => n.props['aria-label'] === 'Export content')[0]!.value, 'data');
  assert.equal(button('Simulate export').props.disabled, false);
  app.unmount();
});
test('H7 unsaved navigation uses the shared dialog and Cancel preserves the form', async () => {
  preferences.clear();
  const root = hostNode('root'),
    app = hostRenderer.createApp(App);
  app.use(createPinia());
  app.mount(root);
  const button = (label: string) =>
    findAll(root, (n) => n.type === 'button' && textContent(n).trim() === label)[0]!;
  await button('Insert').props.onClick();
  await nextTick();
  const field = findAll(root, (n) => n.props['aria-label'] === 'Value for name')[0]!;
  field.props['onUpdate:modelValue']('Unsaved draft');
  await nextTick();
  const pending = button('Browse').props.onClick();
  await nextTick();
  assert.ok(
    findAll(
      root,
      (n) => n.props.role === 'alertdialog' && n.props['aria-label'] === 'Unsaved changes',
    ).length,
  );
  const modal=findAll(root,n=>n.props.role==='alertdialog'&&n.props['aria-label']==='Unsaved changes')[0]!;
  findAll(modal,n=>n.type==='button'&&textContent(n).trim()==='Cancel')[0]!.props.onClick();
  await pending;
  await nextTick();
  assert.ok(findAll(root, (n) => n.props.class === 'record-form').length);
  const second = button('Browse').props.onClick();
  await nextTick();
  button('Discard changes').props.onClick();
  await second;
  await nextTick();
  assert.equal(findAll(root, (n) => n.props.class === 'record-form').length, 0);
  app.unmount();
});
for (const theme of ['light','dark']) test(`${theme}: H6 connection switching preserves page, tree state, query results and independent page size`, async()=>{
 preferences.clear(); preferences.set('dbadmin-theme',theme);
 const pinia=createPinia(), root=hostNode('root'), app=hostRenderer.createApp(App); app.use(pinia); app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'), store=useWorkspace(pinia);
 const original=store.activeId, other=store.connections.find((c:any)=>c.id!==original)!;
 store.workspace.page=3; store.workspace.treeFilter='users'; store.workspace.expandedDatabases=['acme_store','analytics']; store.workspace.queries[0].result={columns:['test'],rows:[{test:'saved result'}]};
 const tab=findAll(root,n=>n.type==='button'&&textContent(n).includes(other.name)&&n.props.onClick)[0]!;
 await tab.props.onClick(); await nextTick();
 store.pageSize=50; store.workspace.page=2; store.workspace.treeFilter='orders';
 const back=findAll(root,n=>n.type==='button'&&textContent(n).includes('Local development')&&n.props.onClick)[0]!;
 await back.props.onClick(); await nextTick();
 assert.equal(store.activeId,original); assert.equal(store.workspace.page,3); assert.equal(store.pageSize,25); assert.equal(store.workspace.treeFilter,'users');
 assert.deepEqual([...store.workspace.expandedDatabases],['acme_store','analytics']); assert.equal(store.workspace.queries[0].result.rows[0].test,'saved result');
 store.pageSize=100; await nextTick(); assert.equal(store.workspace.queries[0].result.rows[0].test,'saved result');
 store.defaultPageSize=250; await nextTick(); assert.equal(preferences.get('dbadmin-default-page-size'),'250'); assert.equal(store.pageSize,100);
 app.unmount();
});
for(const theme of ['light','dark']) test(`${theme}: H8 database/server breadcrumbs open actual compact overview and table actions`,async()=>{
 preferences.clear(); preferences.set('dbadmin-theme',theme);
 const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
 const breadcrumb=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;
 await findAll(breadcrumb,n=>n.type==='button'&&textContent(n).trim()==='acme_store')[0]!.props.onClick();await nextTick();
 assert.equal(store.workspace.scope,'database');assert.ok(findAll(root,n=>String(n.props.class).includes('overview-grid')).length);
 const nav=findAll(root,n=>n.props['aria-label']==='Workspace operations')[0]!;
 assert.deepEqual(findAll(nav,n=>n.type==='button').map(textContent),['Structure','SQL','Export','Import','Routines','Triggers','Events']);
 const overview=findAll(root,n=>String(n.props.class).includes('overview-grid'))[0]!;
 const orders=findAll(overview,n=>n.type==='tr').find(n=>textContent(n).includes('orders'))!;
 await findAll(orders,n=>n.type==='button'&&textContent(n)==='Structure')[0]!.props.onClick();await new Promise(resolve=>setImmediate(resolve));await nextTick();
 assert.equal(store.workspace.table,'orders');assert.equal(store.workspace.scope,'table');assert.equal(store.workspace.view,'Structure');
 const path=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;
 await findAll(path,n=>n.type==='button'&&textContent(n).trim()==='127.0.0.1')[0]!.props.onClick();await nextTick();
 assert.equal(store.workspace.scope,'server');assert.ok(textContent(root).includes('information_schema'));
 app.unmount();
});
test('H9 active Search resets operators, finds NULL and renders explicit NULL values',async()=>{
 preferences.clear();const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);store.table.rows[0].permissions=null;
 const button=(label:string)=>findAll(root,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 await button('Search').props.onClick();await nextTick();
 const operator=findAll(root,n=>n.props['aria-label']==='permissions operator')[0]!;
 operator.props['onUpdate:modelValue']('IS NULL');await nextTick();
 const form=findAll(root,n=>n.type==='form')[0]!;form.props.onSubmit({preventDefault(){}});await nextTick();
 assert.ok(textContent(root).includes('1 matching records'));assert.ok(findAll(root,n=>n.props.class==='null-value'&&textContent(n)==='NULL').length);
 button('Reset').props.onClick();await nextTick();assert.equal(findAll(root,n=>n.props['aria-label']==='permissions operator')[0]!.value,'contains');app.unmount();
});
test('C2/M13 actual Delete confirmation names the key and preserves referenced rows on failure',async()=>{
 preferences.clear();const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
 findAll(root,n=>n.props['aria-label']==='Delete record 1')[0]!.props.onClick();await nextTick();
 const modal=findAll(root,n=>n.props.role==='alertdialog')[0]!;
 assert.ok(textContent(modal).includes('id = 1'));
 findAll(modal,n=>n.type==='button'&&textContent(n).trim()==='Delete 1 record')[0]!.props.onClick();await nextTick();
 assert.equal(store.table.rows.length,128);assert.ok(textContent(modal).includes('RESTRICT'));assert.ok(textContent(modal).includes('orders.user_id'));app.unmount();
});
test('M12 actual connection setup saves an offline configuration without opening a workspace',async()=>{
 preferences.clear();const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia),active=store.activeId;
 const button=(label:string)=>findAll(root,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 await findAll(root,n=>n.type==='button'&&n.props.class==='subtle')[0]!.props.onClick();await nextTick();
 button('New connection').props.onClick();await nextTick();
 const label=(text:string)=>findAll(root,n=>n.type==='label'&&textContent(n).trim().startsWith(text))[0]!;
 const set=async(text:string,value:string)=>{findAll(label(text),n=>n.type==='input')[0]!.props['onUpdate:modelValue'](value);await nextTick();};
 await set('Connection name','Offline draft');await set('Host','invalid.offline.test');
 await button('Save').props.onClick();await nextTick();
 assert.equal(store.connections.at(-1).name,'Offline draft');assert.equal(store.activeId,active);assert.equal(store.opened.length,2);
 assert.ok(preferences.get('dbadmin-connections')!.includes('invalid.offline.test'));app.unmount();
});
test('H10 actual Import uses persistent errors and never partially inserts invalid CSV',async()=>{
 preferences.clear();const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
 const button=(label:string)=>findAll(root,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 await button('Import').props.onClick();await nextTick();
 const file=findAll(root,n=>n.type==='input'&&n.props.type==='file')[0]!;
 file.props.onChange({target:{files:[{name:'invalid.csv',size:80,text:async()=> 'name,email\nValid,unique@example.com\nInvalid,olivia.rhye@example.com'}]}});await nextTick();
 await findAll(root,n=>n.type==='button'&&n.props.role!=='tab'&&textContent(n).trim()==='Import')[0]!.props.onClick();await nextTick();
 assert.equal(store.table.rows.length,128);assert.ok(findAll(root,n=>n.props.role==='alert'&&textContent(n).includes('CSV record 3 (email)')).length);app.unmount();
});
test('C3 actual With selected Copy rejects UNIQUE collisions atomically',async()=>{
 preferences.clear();const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia),before=JSON.stringify(store.table.rows);
 const all=findAll(root,n=>n.props['aria-label']==='Select all visible records')[0]!;
 all.props.onChange({target:{checked:true}});await nextTick();
 const toolbar=findAll(root,n=>n.props.class==='selection-toolbar')[0]!;
 findAll(toolbar,n=>n.type==='button'&&textContent(n).trim()==='Copy')[0]!.props.onClick();await nextTick();
 assert.equal(JSON.stringify(store.table.rows),before);assert.ok(textContent(root).includes('Copy failed: email: This key value already exists.'), textContent(root).slice(-1000));app.unmount();
});
for (const theme of ['light','dark']) test(`${theme}: active Table Structure creates, reorders, edits and drops a composite index`,async()=>{
 preferences.clear();preferences.set('dbadmin-theme',theme);const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
 const button=(label:string,scope=root)=>findAll(scope,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 const named=(label:string)=>findAll(root,n=>n.props['aria-label']===label)[0]!;
 const set=async(label:string,value:string)=>{named(label).props['onUpdate:modelValue'](value);await nextTick();};
 await button('Structure').props.onClick();await nextTick();button('Add index').props.onClick();await nextTick();
 await set('Index name','name_status');await set('Index column 1','name');button('Add key column').props.onClick();await nextTick();await set('Index column 2','status');
 named('Move index column 2 up').props.onClick();await nextTick();
 const submit=async()=>{const modal=findAll(root,n=>n.props.role==='dialog'&&['Add index','Edit index'].includes(n.props['aria-label']))[0]!;findAll(modal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();};
 await submit();assert.deepEqual(store.table.indexes.find((i:any)=>i.name==='name_status').columns.map((p:any)=>p.column),['status','name']);
 named('Edit index name_status').props.onClick();await nextTick();await set('Index name','status_name');await submit();assert.ok(store.table.indexes.some((i:any)=>i.name==='status_name'));
 named('Drop index status_name').props.onClick();await nextTick();button('Drop index',findAll(root,n=>n.props.role==='alertdialog')[0]!).props.onClick();await nextTick();assert.ok(!store.table.indexes.some((i:any)=>i.name==='status_name'));
 app.unmount();
});
for (const theme of ['light','dark']) test(`${theme}: database Routines creates/edits a function, validates a simulated call and confirms deletion`,async()=>{
 preferences.clear();preferences.set('dbadmin-theme',theme);const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
 const button=(label:string,scope=root)=>findAll(scope,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 const named=(label:string)=>findAll(root,n=>n.props['aria-label']===label)[0]!;
 const set=async(label:string,value:string)=>{named(label).props['onUpdate:modelValue'](value);await nextTick();};
 const breadcrumb=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;
 await button('acme_store',breadcrumb).props.onClick();await nextTick();await button('Routines').props.onClick();await nextTick();button('Add routine').props.onClick();await nextTick();
 await set('Object name','calculate_total');await set('Routine type','function');await set('Return type','DECIMAL(12,2)');await set('SQL definition','RETURN subtotal;');button('Add parameter').props.onClick();await nextTick();await set('Parameter name 1','subtotal');await set('Parameter type 1','DECIMAL(12,2)');
 const submit=async()=>{const modal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(modal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();};
 await submit();assert.equal(store.database.routines.find((r:any)=>r.name==='calculate_total').kind,'function');
 named('Edit calculate_total').props.onClick();await nextTick();assert.equal(named('Return type').value,'DECIMAL(12,2)');await set('Return type','DECIMAL(14,2)');await submit();assert.equal(store.database.routines.find((r:any)=>r.name==='calculate_total').returns,'DECIMAL(14,2)');
 named('Execute calculate_total').props.onClick();await nextTick();await set('Parameter subtotal','invalid');await submit();assert.ok(findAll(root,n=>n.props.role==='alert'&&textContent(n).includes('subtotal')).length);
 await set('Parameter subtotal','123.45');await submit();assert.ok(textContent(root).includes('Body not executed'));named('Close object dialog').props.onClick();await nextTick();
 named('Drop calculate_total').props.onClick();await nextTick();button('Drop function',findAll(root,n=>n.props.role==='alertdialog')[0]!).props.onClick();await nextTick();assert.ok(!store.database.routines.some((r:any)=>r.name==='calculate_total'));
 app.unmount();
});
test('actual database Structure creates and browses a read-only view through the existing grid',async()=>{
 preferences.clear();const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
 const button=(label:string,scope=root)=>findAll(scope,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 const set=async(label:string,value:string)=>{findAll(root,n=>n.props['aria-label']===label)[0]!.props['onUpdate:modelValue'](value);await nextTick();};
 const breadcrumb=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;await button('acme_store',breadcrumb).props.onClick();await nextTick();button('Create view').props.onClick();await nextTick();
 await set('Object name','customer_names');await set('SQL definition','SELECT id, name FROM users LIMIT 2;');const modal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(modal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();
 const overview=findAll(root,n=>String(n.props.class).includes('overview-grid'))[0]!;await button('customer_names',overview).props.onClick();await new Promise(resolve=>setImmediate(resolve));await nextTick();
 assert.equal(store.table.kind,'view');assert.equal(findAll(root,n=>n.props['aria-label']==='Delete record 1').length,0);button('Refresh').props.onClick();await nextTick();assert.equal(store.table.rows.length,2);assert.ok(textContent(root).includes('Read-only view'));
 const nav=findAll(root,n=>n.props['aria-label']==='Workspace operations')[0]!;assert.ok(!textContent(nav).includes('Insert'));assert.equal(findAll(root,n=>n.props['aria-label']==='Table records').length,1);
 await button('Structure',nav).props.onClick();await nextTick();assert.ok(textContent(root).includes('SELECT id, name FROM users LIMIT 2;'));button('Definition / Edit').props.onClick();await nextTick();button('Edit definition').props.onClick();await nextTick();assert.equal(findAll(root,n=>n.props['aria-label']==='SQL definition')[0]!.value,'SELECT id, name FROM users LIMIT 2;');
 await set('Object name','customer_names_v2');await set('SQL definition','SELECT id FROM users LIMIT 1;');const editModal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(editModal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();assert.equal(store.table.name,'customer_names_v2');assert.equal(store.table.rows.length,0);
 app.unmount();
});
for(const theme of ['light','dark'])test(`${theme}: database trigger and event Create/Edit/Drop handlers update active mock state`,async()=>{
 preferences.clear();preferences.set('dbadmin-theme',theme);const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
 const button=(label:string,scope=root)=>findAll(scope,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 const named=(label:string)=>findAll(root,n=>n.props['aria-label']===label)[0]!;
 const set=async(label:string,value:any)=>{named(label).props['onUpdate:modelValue'](value);await nextTick();};
 const submit=async()=>{const modal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(modal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();};
 const breadcrumb=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;await button('acme_store',breadcrumb).props.onClick();await nextTick();await button('Triggers').props.onClick();await nextTick();button('Add trigger').props.onClick();await nextTick();
 await set('Object name','audit_new');await set('Trigger table','users');await set('Trigger event','UPDATE');await set('SQL definition','SET @audit = 1;');await submit();assert.equal(store.database.triggers.find((t:any)=>t.name==='audit_new').event,'UPDATE');
 named('Edit audit_new').props.onClick();await nextTick();await set('Trigger timing','AFTER');await submit();assert.equal(store.database.triggers.find((t:any)=>t.name==='audit_new').timing,'AFTER');
 named('Drop audit_new').props.onClick();await nextTick();button('Drop trigger',findAll(root,n=>n.props.role==='alertdialog')[0]!).props.onClick();await nextTick();assert.ok(!store.database.triggers.some((t:any)=>t.name==='audit_new'));
 await button('Events').props.onClick();await nextTick();button('Add event').props.onClick();await nextTick();await set('Object name','cleanup');await set('Event schedule','EVERY 0 DAY');await set('SQL definition','DELETE FROM sessions;');await submit();assert.ok(findAll(root,n=>n.props.role==='alert'&&textContent(n).includes('positive')).length);await set('Event schedule','EVERY 2 DAY');await submit();
 named('Edit cleanup').props.onClick();await nextTick();assert.equal(named('Event schedule').value,'EVERY 2 DAY');await set('Event schedule','EVERY 3 HOUR');await submit();assert.equal(store.database.events.find((e:any)=>e.name==='cleanup').schedule,'EVERY 3 HOUR');
 named('Drop cleanup').props.onClick();await nextTick();button('Drop event',findAll(root,n=>n.props.role==='alertdialog')[0]!).props.onClick();await nextTick();assert.ok(!store.database.events.some((e:any)=>e.name==='cleanup'));app.unmount();
});
test('PostgreSQL materialized view refresh and method selection work in actual Structure; SQLite hides routines/events',async()=>{
 preferences.clear();const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
 store.connect('staging');await nextTick();
 const button=(label:string,scope=root)=>findAll(scope,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 const named=(label:string)=>findAll(root,n=>n.props['aria-label']===label)[0]!;
 const path=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;await button('acme_store',path).props.onClick();await nextTick();
 assert.ok(!findAll(root,n=>n.props.role==='tab').map(textContent).includes('Events'));assert.ok(textContent(root).includes('Materialized view'));
 const overview=findAll(root,n=>String(n.props.class).includes('overview-grid'))[0]!;await button('customer_snapshot',overview).props.onClick();await new Promise(resolve=>setImmediate(resolve));await nextTick();
 await button('Structure').props.onClick();await nextTick();button('Refresh materialized view (mock)').props.onClick();await nextTick();assert.ok(store.table.refreshedAt);button('Add index').props.onClick();await nextTick();assert.deepEqual(named('Index method').children.filter(n=>n.type==='option').map(textContent),['BTREE','HASH','GIST','SPGIST','GIN','BRIN']);button('Cancel',findAll(root,n=>n.props.role==='dialog')[0]!).props.onClick();await nextTick();
 const sqlite={...store.connections[0],id:'sqlite-test',name:'SQLite test',engine:'SQLite'};store.connections.push(sqlite);store.connect(sqlite.id);await nextTick();
 const path2=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;await button('acme_store',path2).props.onClick();await nextTick();const tabs=findAll(root,n=>n.props['aria-label']==='Workspace operations')[0]!;assert.ok(!textContent(tabs).includes('Routines'));assert.ok(!textContent(tabs).includes('Events'));assert.ok(textContent(tabs).includes('Triggers'));app.unmount();
});
test('actual database Create table opens the existing column editor workflow',async()=>{
 preferences.clear();const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
 const button=(label:string,scope=root)=>findAll(scope,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 const path=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;await button('acme_store',path).props.onClick();await nextTick();button('Create table').props.onClick();await nextTick();
 findAll(root,n=>n.props['aria-label']==='Object name')[0]!.props['onUpdate:modelValue']('new_records');await nextTick();const modal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(modal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();
 assert.equal(store.table.name,'new_records');assert.equal(store.workspace.view,'Structure');assert.equal(store.table.rows.length,0);assert.equal(store.table.columns[0].autoIncrement,true);button('Add column').props.onClick();await nextTick();assert.ok(findAll(root,n=>n.props.role==='dialog'&&n.props['aria-label']==='Add column').length);app.unmount();
});
for(const theme of ['light','dark']) test(`${theme}: actual index type controls save MySQL Fulltext and Spatial definitions`,async()=>{
 preferences.clear();preferences.set('dbadmin-theme',theme);const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);store.selectTable('acme_store','products');await nextTick();
 const button=(label:string,scope=root)=>findAll(scope,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 const named=(label:string)=>findAll(root,n=>n.props['aria-label']===label)[0]!;
 const set=async(label:string,value:string)=>{named(label).props['onUpdate:modelValue'](value);named(label).props.onChange?.({});await nextTick();};
 await button('Structure').props.onClick();await nextTick();
 for(const [kind,column,method] of [['fulltext','name','fulltext'],['spatial','location','rtree']]){
  button('Add index').props.onClick();await nextTick();await set('Index name',`test_${kind}`);await set('Index type',kind!);await set('Index column 1',column!);assert.equal(named('Index method').value,method);assert.equal(named('Index method').props.disabled,true);
  const modal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(modal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();assert.equal(store.table.indexes.find((i:any)=>i.name===`test_${kind}`).method,method);
 }
 app.unmount();
});
test('empty database Structure can create the first table through the active overview',async()=>{
 preferences.clear();const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
 const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);store.workspace.databases.push({name:'empty_schema',tables:[]});await nextTick();
 const button=(label:string,scope=root)=>findAll(scope,n=>n.type==='button'&&textContent(n).trim()===label)[0]!;
 let path=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;await button('127.0.0.1',path).props.onClick();await nextTick();const overview=findAll(root,n=>String(n.props.class).includes('overview-grid'))[0]!;await button('empty_schema',overview).props.onClick();await new Promise(resolve=>setImmediate(resolve));await nextTick();assert.equal(store.database.name,'empty_schema');assert.ok(textContent(root).includes('No tables or views'));
 button('Create table').props.onClick();await nextTick();findAll(root,n=>n.props['aria-label']==='Object name')[0]!.props['onUpdate:modelValue']('first_table');await nextTick();const modal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(modal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();assert.equal(store.table.name,'first_table');assert.equal(store.workspace.view,'Structure');app.unmount();
});
for (const theme of ['light', 'dark']) test(`${theme}: a failed routine retry removes the previous successful call notice`, async () => {
  preferences.clear(); preferences.set('dbadmin-theme', theme);
  const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
  try {
    const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
    store.workspace.scope='database';store.workspace.view='Routines';await nextTick();
    findAll(root,n=>n.props['aria-label']==='Execute find_customer')[0]!.props.onClick();await nextTick();
    const field=findAll(root,n=>n.props['aria-label']==='Parameter customer_id')[0]!;
    const submit=async()=>{const modal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(modal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();};
    field.props['onUpdate:modelValue']('1');await submit();
    assert.ok(findAll(root,n=>n.props.role==='status'&&textContent(n).includes('input(s) validated')).length);
    field.props['onUpdate:modelValue']('invalid');await submit();
    assert.ok(findAll(root,n=>n.props.role==='alert'&&textContent(n).includes('customer_id')).length);
    assert.equal(findAll(root,n=>n.props.role==='status'&&textContent(n).includes('input(s) validated')).length,0);
  } finally { app.unmount(); }
});
for (const theme of ['light', 'dark']) test(`${theme}: switching a descending B-tree draft to Fulltext or Hash clears the inapplicable direction`, async () => {
  preferences.clear(); preferences.set('dbadmin-theme', theme);
  const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
  try {
    const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
    store.selectTable('acme_store','products');store.workspace.view='Structure';await nextTick();
    const button=(name:string)=>findAll(root,n=>n.type==='button'&&textContent(n).trim()===name)[0]!;
    const named=(name:string)=>findAll(root,n=>n.props['aria-label']===name)[0]!;
    const set=async(name:string,value:string)=>{named(name).props['onUpdate:modelValue'](value);named(name).props.onChange?.({});await nextTick();};
    button('Add index').props.onClick();await nextTick();
    await set('Index name','fulltext_retry');await set('Index column 1','name');await set('Index order 1','DESC');await set('Index type','fulltext');
    assert.equal(named('Index order 1').props.disabled,true);assert.equal(named('Index order 1').value,'ASC');
    const modal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(modal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();
    assert.ok(store.table.indexes.some((index:any)=>index.name==='fulltext_retry'&&index.kind==='fulltext'));
    store.connect('staging');store.selectTable('acme_store','products');store.workspace.view='Structure';await nextTick();
    button('Add index').props.onClick();await nextTick();await set('Index name','hash_retry');await set('Index column 1','name');await set('Index order 1','DESC');await set('Index method','hash');
    assert.equal(named('Index order 1').props.disabled,true);assert.equal(named('Index order 1').value,'ASC');
    const pgModal=findAll(root,n=>n.props.role==='dialog')[0]!;findAll(pgModal,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();assert.ok(store.table.indexes.some((index:any)=>index.name==='hash_retry'&&index.method==='hash'));

  } finally { app.unmount(); }
});
for (const theme of ['light', 'dark']) test(`${theme}: dropping the last table returns to usable database Structure`, async () => {
  preferences.clear(); preferences.set('dbadmin-theme', theme);
  const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
  try {
    const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
    store.workspace.databases.push({name:'disposable',tables:[]});store.workspace.database='disposable';store.createTable('only_table');store.workspace.view='Operations';await nextTick();
    const button=(name:string,scope=root)=>findAll(scope,n=>n.type==='button'&&textContent(n).trim()===name)[0]!;
    button('Drop table').props.onClick();await nextTick();
    const confirm=findAll(root,n=>n.props.role==='alertdialog')[0]!;button('Drop table',confirm).props.onClick();await nextTick();
    assert.equal(store.database.tables.length,0);assert.equal(store.workspace.scope,'database');assert.equal(store.workspace.view,'Structure');
    assert.ok(button('Create table'));
  } finally { app.unmount(); }
});
for (const engine of ['MySQL','MariaDB','PostgreSQL','SQLite']) test(`${engine}: table and database object lifecycle handlers remain consistent in both themes`, async () => {
  for (const theme of ['light','dark']) {
    preferences.clear();preferences.set('dbadmin-theme',theme);
    const pinia=createPinia(),root=hostNode('root'),app=hostRenderer.createApp(App);app.use(pinia);app.mount(root);
    try {
      const {useWorkspace}=await loadModule('../../src/stores/workspace.ts'),store=useWorkspace(pinia);
      const id=`verify-${engine}`;store.connections.push({...store.connections[0],id,name:`Verification ${engine}`,engine});store.connect(id);store.workspace.scope='database';store.workspace.view='Structure';await nextTick();
      const button=(name:string,scope=root)=>{const item=findAll(scope,n=>n.type==='button'&&textContent(n).trim()===name)[0];assert.ok(item,`button ${name}`);return item;};
      const named=(name:string)=>{const item=findAll(root,n=>n.props['aria-label']===name)[0];assert.ok(item,`control ${name}`);return item;};
      const click=async(item:any)=>{await item.props.onClick?.({preventDefault(){},stopPropagation(){}});await nextTick();};
      const set=async(name:string,value:any)=>{const item=named(name);item.props['onUpdate:modelValue'](value);item.props.onChange?.({target:item});await nextTick();};
      const modal=()=>findAll(root,n=>n.props.role==='dialog')[0]!;
      const submit=async(scope=modal())=>{findAll(scope,n=>n.type==='form')[0]!.props.onSubmit({preventDefault(){}});await nextTick();};
      const confirm=async(label:string)=>{await click(button(label,findAll(root,n=>n.props.role==='alertdialog')[0]!));};
      const database=async()=>{const path=findAll(root,n=>n.props['aria-label']==='Database breadcrumb')[0]!;await click(button('acme_store',path));};
      await click(button('Create table'));await set('Object name','lifecycle_records');await submit();
      await click(button('Add column'));await set('Column name','entry_value');if(engine!=='SQLite')await set('Length',30);else assert.equal(findAll(root,n=>n.props['aria-label']==='Length').length,0);await submit();
      await click(button('Add index'));await set('Index name','lifecycle_lookup');await set('Index column 1','entry_value');await submit();
      await click(button('Insert'));const nullable=findAll(root,n=>n.props['aria-label']==='NULL for entry_value')[0];if(nullable){nullable.props['onUpdate:modelValue'](false);nullable.props.onChange?.({});await nextTick();}await set('Value for entry_value','first');await submit(findAll(root,n=>n.props.class==='record-form')[0]!);assert.equal(store.table.rows[0].entry_value,'first');
      await click(named('Edit record 1'));await set('Value for entry_value','edited');await submit(findAll(root,n=>n.props.class==='record-form')[0]!);assert.equal(store.table.rows[0].entry_value,'edited');
      await click(button('Operations'));
      const renameForm=findAll(root,n=>n.type==='form'&&textContent(n).includes('Rename table'))[0]!;
      findAll(renameForm,n=>n.type==='input')[0]!.props['onUpdate:modelValue']('lifecycle_renamed');await submit(renameForm);assert.equal(store.table.name,'lifecycle_renamed');
      const copyForm=findAll(root,n=>n.type==='form'&&textContent(n).includes('Copy table'))[0]!;
      findAll(copyForm,n=>n.type==='input'&&n.props.type!=='checkbox')[0]!.props['onUpdate:modelValue']('lifecycle_copy');await submit(copyForm);
      const copy=store.database.tables.find((t:any)=>t.name==='lifecycle_copy');assert.ok(copy);assert.equal(copy.rows[0].entry_value,'edited');assert.notEqual(copy.rows[0],store.table.rows[0]);
      if(engine==='PostgreSQL'||engine==='SQLite') {
        const names=store.database.tables.flatMap((t:any)=>t.indexes||[]).filter((index:any)=>engine==='PostgreSQL'||index.kind!=='primary').map((index:any)=>index.name);
        assert.equal(new Set(names).size,names.length,'copy must not duplicate schema-level index names');
      }
      findAll(copyForm,n=>n.type==='input'&&n.props.type!=='checkbox')[0]!.props['onUpdate:modelValue']('active_customers');await submit(copyForm);
      assert.equal(store.database.tables.filter((t:any)=>t.name==='active_customers').length,0,'copy must not collide with a view name');
      await click(button('Truncate table'));await confirm('Truncate table');assert.equal(store.table.rows.length,0);
      await click(button('Drop table'));await confirm('Drop table');assert.ok(!store.database.tables.some((t:any)=>t.name==='lifecycle_renamed'));
      await database();await click(button('Create view'));await set('Object name','lifecycle_view');await set('SQL definition','SELECT id FROM users LIMIT 2;');await submit();
      const overview=findAll(root,n=>String(n.props.class).includes('overview-grid'))[0]!;await click(button('lifecycle_view',overview));await new Promise(resolve=>setImmediate(resolve));await nextTick();await click(button('Refresh'));assert.equal(store.table.rows.length,2);assert.equal(store.table.kind,'view');
      await click(button('Structure'));await click(button('Definition / Edit'));await click(button('Edit definition'));await set('SQL definition','SELECT id FROM users LIMIT 1;');await submit();await click(button('Refresh snapshot (mock)'));assert.equal(store.table.rows.length,1);await click(button('Drop view'));await confirm('Drop view');assert.ok(!store.database.views.some((v:any)=>v.name==='lifecycle_view'));
      const nav=findAll(root,n=>n.props['aria-label']==='Workspace operations')[0]!;
      assert.equal(textContent(nav).includes('Routines'),engine!=='SQLite');assert.equal(textContent(nav).includes('Events'),engine==='MySQL'||engine==='MariaDB');
      if(engine!=='SQLite') {
        await click(button('Routines'));await click(button('Add routine'));await set('Object name','lifecycle_function');await set('Routine type','function');await set('Return type','INT');await set('SQL definition',engine==='PostgreSQL'?'SELECT input_value;':'RETURN input_value;');await click(button('Add parameter'));await set('Parameter name 1','input_value');await submit();
        await click(named('Execute lifecycle_function'));await set('Parameter input_value','7');await submit();assert.ok(textContent(root).includes('Body not executed'));await click(named('Close object dialog'));
        await click(named('Edit lifecycle_function'));await set('Return type','BIGINT');await submit();assert.equal(store.database.routines.find((r:any)=>r.name==='lifecycle_function').returns,'BIGINT');await click(named('Drop lifecycle_function'));await confirm('Drop function');assert.ok(!store.database.routines.some((r:any)=>r.name==='lifecycle_function'));
      }
      await click(button('Triggers'));await click(button('Add trigger'));await set('Object name','lifecycle_trigger');await set('Trigger table','lifecycle_copy');await set('SQL definition',engine==='PostgreSQL'?'EXECUTE FUNCTION audit_user_changes();':'BEGIN SELECT 1; END');await submit();await click(named('Edit lifecycle_trigger'));await set('Trigger timing','AFTER');await submit();assert.equal(store.database.triggers.find((t:any)=>t.name==='lifecycle_trigger').timing,'AFTER');await click(named('Drop lifecycle_trigger'));await confirm('Drop trigger');assert.ok(!store.database.triggers.some((t:any)=>t.name==='lifecycle_trigger'));
      if(engine==='MySQL'||engine==='MariaDB') {await click(button('Events'));await click(button('Add event'));await set('Object name','lifecycle_event');await set('SQL definition','DELETE FROM sessions;');await submit();await click(named('Edit lifecycle_event'));await set('Event schedule','EVERY 2 HOUR');const enabled=findAll(modal(),n=>n.type==='input'&&n.props.type==='checkbox')[0]!;enabled.props['onUpdate:modelValue'](true);await submit();assert.equal(store.database.events.find((e:any)=>e.name==='lifecycle_event').schedule,'EVERY 2 HOUR');assert.equal(store.database.events.find((e:any)=>e.name==='lifecycle_event').enabled,true);await click(named('Drop lifecycle_event'));await confirm('Drop event');assert.ok(!store.database.events.some((e:any)=>e.name==='lifecycle_event'));}
      if(engine==='PostgreSQL') {
        await click(button('Structure'));await click(button('Create view'));await set('View type','materialized-view');await set('Object name','lifecycle_snapshot');await set('SQL definition','SELECT id FROM users LIMIT 2;');await submit();const grid=findAll(root,n=>String(n.props.class).includes('overview-grid'))[0]!;await click(button('lifecycle_snapshot',grid));await new Promise(resolve=>setImmediate(resolve));await nextTick();await click(button('Structure'));await click(button('Refresh materialized view (mock)'));assert.equal(store.table.rows.length,2);await click(button('Add index'));await set('Index name','snapshot_lookup');await set('Index type','unique');await submit();assert.ok(store.table.indexes.some((i:any)=>i.name==='snapshot_lookup'));await click(named('Drop index snapshot_lookup'));await confirm('Drop index');await click(button('Definition / Edit'));await click(button('Edit definition'));await set('SQL definition','SELECT id FROM users LIMIT 1;');await submit();assert.equal(store.table.rows.length,0);await click(button('Refresh materialized view (mock)'));assert.equal(store.table.rows.length,1);await click(button('Drop view'));await confirm('Drop materialized-view');assert.ok(!store.database.views.some((v:any)=>v.name==='lifecycle_snapshot'));
      }
    } finally { app.unmount(); }
  }
});
