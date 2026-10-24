import { readFileSync, existsSync } from 'node:fs';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { dirname, resolve, extname } from 'node:path';
import { stripTypeScriptTypes } from 'node:module';
import { createRenderer } from 'vue';
import { parse, compileScript } from '@vue/compiler-sfc';
export interface HostNode {
  type: string;
  tagName: string;
  text: string;
  props: Record<string, any>;
  children: HostNode[];
  parent?: HostNode;
  value: any;
  [key: string]: any;
}
export function hostNode(type: string, text = ''): HostNode {
  const n: HostNode = {
    type,
    tagName: type.toUpperCase(),
    text,
    props: {},
    children: [],
    value: '',
    classList: { add() {}, remove() {} },
    style: {},
    addEventListener() {},
    removeEventListener() {},
    setAttribute() {},
    removeAttribute() {},
    getAttribute(name: string) {
      return this.props[name];
    },
    focus() {},
    getRootNode() {
      return { activeElement: null };
    },
  };
  let storedValue: unknown = '';
  Object.defineProperty(n, 'value', {
    get() {
      if (n.type === 'select') {
        if (n.selectedIndex >= 0 && n.options[n.selectedIndex])
          return n.options[n.selectedIndex].value;
        const selected = n.children.find((child) => child.type === 'option' && child.selected);
        if (selected) return selected.value;
      }
      if (n.type === 'option' && n.props.value === undefined) return n.text;
      return storedValue;
    },
    set(value) {
      storedValue = value;
    },
    configurable: true,
  });
  Object.defineProperty(n, 'options', {
    get() {
      return n.children.filter((child) => child.type === 'option');
    },
  });
  return n;
}
export const hostRenderer = createRenderer<HostNode, HostNode>({
  insertStaticContent(content, parent, anchor) {
    const n = hostNode('#static', content.replace(/<[^>]*>/g, ''));
    const i = anchor ? parent.children.indexOf(anchor) : -1;
    parent.children.splice(i < 0 ? parent.children.length : i, 0, n);
    n.parent = parent;
    return [n, n];
  },
  createElement: (type) => hostNode(type),
  createText: (text) => hostNode('#text', text),
  createComment: (text) => hostNode('#comment', text),
  setText: (n, text) => {
    n.text = text;
  },
  setElementText: (n, text) => {
    n.text = text;
    n.children = [];
  },
  patchProp: (n, key, _old, value) => {
    n.props[key] = value;
    if (['value', 'multiple', 'checked', 'selected'].includes(key)) n[key] = value;
  },
  insert(n, parent, anchor) {
    if (n.parent) n.parent.children = n.parent.children.filter((x) => x !== n);
    const i = anchor ? parent.children.indexOf(anchor) : -1;
    parent.children.splice(i < 0 ? parent.children.length : i, 0, n);
    n.parent = parent;
  },
  remove(n) {
    if (n.parent) n.parent.children = n.parent.children.filter((x) => x !== n);
    n.parent = undefined;
  },
  parentNode: (n) => n.parent ?? null,
  nextSibling(n) {
    const siblings = n.parent?.children ?? [];
    return siblings[siblings.indexOf(n) + 1] ?? null;
  },
});
export function findAll(root: HostNode, predicate: (n: HostNode) => boolean): HostNode[] {
  return [
    ...(predicate(root) ? [root] : []),
    ...root.children.flatMap((child) => findAll(child, predicate)),
  ];
}
export function textContent(root: HostNode): string {
  return root.text + root.children.map(textContent).join('');
}
const modules = new Map<string, string>();
function sourceModule(filename: string): string {
  const cached = modules.get(filename);
  if (cached) return cached;
  const source = readFileSync(filename, 'utf8');
  const code = filename.endsWith('.vue')
    ? compileScript(parse(source, { filename }).descriptor, {
        id: filename,
        inlineTemplate: true,
        templateOptions: { compilerOptions: { hoistStatic: false } },
      }).content
    : source;
  const js = stripTypeScriptTypes(code);
  const resolved = js.replace(/from\s*(["'])([^"']+)\1/g, (_m, _q, specifier: string) => {
    let target: string;
    if (specifier.startsWith('.')) {
      let file = resolve(dirname(filename), specifier);
      if (!extname(file) && existsSync(file + '.ts')) file += '.ts';
      target = /\.(ts|vue)$/.test(file) ? sourceModule(file) : pathToFileURL(file).href;
    } else target = import.meta.resolve(specifier);
    return `from ${JSON.stringify(target)}`;
  });
  const url = `data:text/javascript;base64,${Buffer.from(resolved).toString('base64')}`;
  modules.set(filename, url);
  return url;
}
export async function loadVue(path: string) {
  return (await import(sourceModule(fileURLToPath(new URL(path, import.meta.url))))).default;
}
export async function loadModule(path: string) {
  return import(sourceModule(fileURLToPath(new URL(path, import.meta.url))));
}
