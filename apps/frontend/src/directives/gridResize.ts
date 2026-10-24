import type { ObjectDirective } from 'vue';
import {
  clampWidth,
  defaultColumnWidth,
  readGridWidths,
  saveGridWidths,
} from '../services/gridWidths';
import type { GridColumn } from '../services/gridWidths';
interface Options {
  key: string;
  columns: GridColumn[];
  onError?: (message: string) => void;
}
interface Mounted {
  signature: string;
  dispose: () => void;
}
const mounted = new WeakMap<HTMLTableElement, Mounted>();
function attach(table: HTMLTableElement, options: Options) {
  // DOM layout is intentionally absent in SSR and Vue's non-browser test host.
  if (!table.ownerDocument?.defaultView) return;
  mounted.get(table)?.dispose();
  const columns = options.columns,
    headers = Array.from(table.tHead?.rows[0]?.cells ?? []);
  if (!columns.length || columns.length !== headers.length) return;
  const cleanups: (() => void)[] = [],
    widths = columns.map(defaultColumnWidth);
  let menu: HTMLDivElement | undefined,
    frame = 0,
    dragCleanup: (() => void) | undefined;
  function report(error: unknown) {
    options.onError?.(`Column widths were not saved: ${(error as Error).message}`);
  }
  try {
    const saved = readGridWidths(localStorage, options.key);
    columns.forEach((c, i) => {
      if (c.resizable !== false && saved[c.name]) widths[i] = saved[c.name]!;
    });
  } catch (error) {
    report(error);
  }
  const group = document.createElement('colgroup'),
    colNodes = columns.map((c, i) => {
      const col = document.createElement('col');
      col.style.width = widths[i] + 'px';
      group.append(col);
      return col;
    });
  table.prepend(group);
  table.classList.add('resizable-grid');
  function apply(i: number, width: number) {
    widths[i] = clampWidth(width);
    colNodes[i]!.style.width = widths[i] + 'px';
    table.style.width = widths.reduce((sum, w) => sum + w, 0) + 'px';
    headers[i]!.querySelector('[role="separator"]')?.setAttribute(
      'aria-valuenow',
      String(widths[i]),
    );
  }
  table.style.width = widths.reduce((sum, w) => sum + w, 0) + 'px';
  function save() {
    try {
      saveGridWidths(
        localStorage,
        options.key,
        Object.fromEntries(
          columns.flatMap((c, i) => (c.resizable === false ? [] : [[c.name, widths[i]!]])),
        ),
      );
    } catch (error) {
      report(error);
    }
  }
  function reset() {
    columns.forEach((c, i) => {
      widths[i] = defaultColumnWidth(c);
      colNodes[i]!.style.width = widths[i] + 'px';
      headers[i]!.querySelector('[role="separator"]')?.setAttribute(
        'aria-valuenow',
        String(widths[i]),
      );
    });
    table.style.width = widths.reduce((sum, w) => sum + w, 0) + 'px';
    try {
      saveGridWidths(localStorage, options.key, null);
    } catch (error) {
      report(error);
    }
    menu?.remove();
    menu = undefined;
  }
  columns.forEach((column, index) => {
    if (column.resizable === false) return;
    const header = headers[index]!,
      handle = document.createElement('span');
    handle.className = 'column-resizer';
    handle.tabIndex = 0;
    handle.setAttribute('role', 'separator');
    handle.setAttribute('aria-orientation', 'vertical');
    handle.setAttribute('aria-label', `Resize ${column.name} column`);
    handle.setAttribute('aria-valuemin', '64');
    handle.setAttribute('aria-valuemax', '600');
    handle.setAttribute('aria-valuenow', String(widths[index]));
    handle.title = 'Drag to resize; double-click to auto-fit. Arrow keys resize; Enter auto-fits.';
    function autofit() {
      const canvas = document.createElement('canvas'),
        context = canvas.getContext('2d');
      if (!context) return;
      context.font = getComputedStyle(table).font;
      let width = context.measureText(column.name).width + 45;
      for (const row of Array.from(table.tBodies[0]?.rows ?? [])) {
        const cell = row.cells[index];
        if (cell && !cell.querySelector('.cell-editor'))
          width = Math.max(
            width,
            context.measureText((cell.textContent ?? '').slice(0, 2000)).width + 26,
          );
      }
      apply(index, width);
      save();
    }
    const down = (event: PointerEvent) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      dragCleanup?.();
      const start = event.clientX,
        original = widths[index]!;
      handle.classList.add('dragging');
      const move = (e: PointerEvent) => {
        const next = original + e.clientX - start;
        cancelAnimationFrame(frame);
        frame = requestAnimationFrame(() => apply(index, next));
      };
      const end = (e: PointerEvent) => {
        cancelAnimationFrame(frame);
        apply(index, original + e.clientX - start);
        dragCleanup?.();
        save();
      };
      dragCleanup = () => {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', end);
        window.removeEventListener('pointercancel', cancel);
        handle.classList.remove('dragging');
        dragCleanup = undefined;
      };
      const cancel = () => dragCleanup?.();
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', end);
      window.addEventListener('pointercancel', cancel);
    };
    const dbl = (event: MouseEvent) => {
      event.preventDefault();
      event.stopPropagation();
      autofit();
    };
    const key = (event: KeyboardEvent) => {
      if (['ArrowLeft', 'ArrowRight', 'Enter'].includes(event.key)) {
        event.preventDefault();
        event.stopPropagation();
        if (event.key === 'Enter') autofit();
        else {
          apply(index, widths[index]! + (event.key === 'ArrowRight' ? 10 : -10));
          save();
        }
      }
    };
    const click = (event: Event) => event.stopPropagation();
    handle.addEventListener('pointerdown', down);
    handle.addEventListener('dblclick', dbl);
    handle.addEventListener('keydown', key);
    handle.addEventListener('click', click);
    header.append(handle);
    cleanups.push(() => {
      handle.remove();
      handle.removeEventListener('pointerdown', down);
      handle.removeEventListener('dblclick', dbl);
      handle.removeEventListener('keydown', key);
      handle.removeEventListener('click', click);
    });
  });
  const close = (event: Event) => {
    if (event instanceof KeyboardEvent && event.key !== 'Escape') return;
    if (menu && (event instanceof KeyboardEvent || !menu.contains(event.target as Node))) {
      menu.remove();
      menu = undefined;
    }
  };
  const context = (event: MouseEvent) => {
    event.preventDefault();
    menu?.remove();
    menu = document.createElement('div');
    menu.className = 'grid-context-menu';
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', 'Grid actions');
    const button = document.createElement('button');
    button.type = 'button';
    button.setAttribute('role', 'menuitem');
    button.textContent = 'Reset Column Widths';
    button.addEventListener('click', reset);
    menu.append(button);
    document.body.append(menu);
    menu.style.left =
      Math.max(0, Math.min(event.clientX, window.innerWidth - menu.offsetWidth - 8)) + 'px';
    menu.style.top =
      Math.max(0, Math.min(event.clientY, window.innerHeight - menu.offsetHeight - 8)) + 'px';
    button.focus();
  };
  table.addEventListener('contextmenu', context);
  window.addEventListener('pointerdown', close);
  window.addEventListener('keydown', close);
  mounted.set(table, {
    signature: JSON.stringify([options.key, columns]),
    dispose() {
      cancelAnimationFrame(frame);
      dragCleanup?.();
      cleanups.forEach((fn) => fn());
      group.remove();
      menu?.remove();
      table.removeEventListener('contextmenu', context);
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('keydown', close);
    },
  });
}
export const gridResize: ObjectDirective<HTMLTableElement, Options> = {
  mounted: (table, binding) => attach(table, binding.value),
  updated(table, binding) {
    if (
      mounted.get(table)?.signature !== JSON.stringify([binding.value.key, binding.value.columns])
    )
      attach(table, binding.value);
  },
  beforeUnmount(table) {
    mounted.get(table)?.dispose();
    mounted.delete(table);
  },
};
