import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import { createRenderer, defineComponent, h, nextTick, ref } from 'vue';
import { parse, compileScript } from '@vue/compiler-sfc';

// Vue's host renderer exercises real component events and reactive updates without a browser.
interface Node {
  type: string;
  text: string;
  props: Record<string, any>;
  children: Node[];
  parent?: Node;
}
const node = (type: string, text = ''): Node => ({ type, text, props: {}, children: [] });
const renderer = createRenderer<Node, Node>({
  createElement: (type) => node(type),
  createText: (text) => node('#text', text),
  createComment: (text) => node('#comment', text),
  setText: (target, text) => {
    target.text = text;
  },
  setElementText: (target, text) => {
    target.text = text;
    target.children = [];
  },
  patchProp: (target, key, _previous, value) => {
    target.props[key] = value;
  },
  insert(child, parent, anchor) {
    if (child.parent) child.parent.children = child.parent.children.filter((n) => n !== child);
    const index = anchor ? parent.children.indexOf(anchor) : -1;
    parent.children.splice(index < 0 ? parent.children.length : index, 0, child);
    child.parent = parent;
  },
  remove(child) {
    if (child.parent) child.parent.children = child.parent.children.filter((n) => n !== child);
    child.parent = undefined;
  },
  parentNode: (child) => child.parent ?? null,
  nextSibling(child) {
    const siblings = child.parent?.children ?? [];
    return siblings[siblings.indexOf(child) + 1] ?? null;
  },
});
function find(root: Node, predicate: (n: Node) => boolean): Node | undefined {
  if (predicate(root)) return root;
  for (const child of root.children) {
    const match = find(child, predicate);
    if (match) return match;
  }
}
async function component(name: string) {
  const source = readFileSync(new URL(`../src/components/${name}.vue`, import.meta.url), 'utf8');
  const { descriptor } = parse(source);
  const script = compileScript(descriptor, { id: name, inlineTemplate: true });
  const js = stripTypeScriptTypes(script.content).replace(
    /from (["'])([^"']+)\1/g,
    (_match, _quote, specifier) => `from ${JSON.stringify(import.meta.resolve(specifier))}`,
  );
  return (await import(`data:text/javascript;base64,${Buffer.from(js).toString('base64')}`))
    .default;
}
const ResultNotice = await component('ResultNotice');
const SqlQueryPreview = await component('SqlQueryPreview');
for (const kind of ['success', 'warning', 'error']) {
  test(`${kind} notification dismisses immediately and returns on a new notification`, async () => {
    const version = ref(1),
      root = node('root');
    const app = renderer.createApp(
      defineComponent(
        () => () => h(ResultNotice, { kind, notificationKey: version.value }, () => 'Query status'),
      ),
    );
    app.mount(root);
    const dismiss = find(root, (n) => n.props['aria-label'] === 'Dismiss query notification');
    assert.ok(dismiss);
    assert.equal(dismiss.props.type, 'button');
    dismiss.props.onClick();
    await nextTick();
    assert.equal(
      find(root, (n) => n.props['aria-label'] === 'Dismiss query notification'),
      undefined,
    );
    assert.equal(
      find(root, (n) => n.props.role === 'status' || n.props.role === 'alert'),
      undefined,
    );
    version.value++;
    await nextTick();
    assert.ok(find(root, (n) => n.props['aria-label'] === 'Dismiss query notification'));
    app.unmount();
  });
}
test('SQL preview collapses, preserves its slot content, and keeps editing independent', async () => {
  const expanded = ref(true),
    sql = ref('SELECT * FROM users;'),
    root = node('root');
  let edited = 0;
  const app = renderer.createApp(
    defineComponent(
      () => () =>
        h(
          SqlQueryPreview,
          {
            expanded: expanded.value,
            'onUpdate:expanded': (value: boolean) => {
              expanded.value = value;
            },
            onEdit: () => {
              edited++;
            },
          },
          () => sql.value,
        ),
    ),
  );
  app.mount(root);
  let toggle = find(root, (n) => n.props['aria-label'] === 'Collapse SQL query preview')!;
  assert.equal(toggle.props['aria-expanded'], true);
  assert.ok(toggle.props['aria-controls']);
  const edit = find(root, (n) => n.type === 'button' && n.props.class === 'preview-edit');
  assert.ok(edit);
  edit.props.onClick();
  assert.equal(edited, 1);
  toggle.props.onClick();
  await nextTick();
  assert.equal(expanded.value, false);
  assert.equal(
    find(root, (n) => n.props.class === 'preview-edit'),
    undefined,
  );
  toggle = find(root, (n) => n.props['aria-label'] === 'Expand SQL query preview')!;
  assert.equal(toggle.props['aria-expanded'], false);
  sql.value = 'SELECT * FROM orders LIMIT 10;';
  await nextTick();
  assert.equal(expanded.value, false);
  toggle.props.onClick();
  await nextTick();
  assert.equal(expanded.value, true);
  assert.ok(find(root, (n) => n.type === '#text' && n.text === sql.value));
  app.unmount();
});
const SchemaIcon = await component('SchemaIcon');
for (const [kind, label] of [
  ['primary', 'Primary key'],
  ['unique', 'Unique key'],
  ['foreign', 'Foreign key'],
  ['index', 'Normal index'],
]) {
  test(`${label} uses a descriptive accessible tooltip`, () => {
    const root = node('root'),
      app = renderer.createApp(SchemaIcon, { kind, detail: 'example' });
    app.mount(root);
    const icon = find(root, (n) => n.props.role === 'img')!;
    assert.equal(icon.props.title, `${label}: example`);
    assert.equal(icon.props['aria-label'], `${label}: example`);
    assert.ok(String(icon.props.class).includes(`schema-icon-${kind}`));
    app.unmount();
  });
}
