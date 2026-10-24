import type { ObjectDirective } from 'vue';
interface Options {
  close: () => unknown;
  returnFocus?: HTMLElement;
}
const stack: HTMLElement[] = [];
const cleanup = new WeakMap<HTMLElement, () => void>();
const selectors =
  'button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),a[href],[tabindex]:not([tabindex="-1"])';
function attach(element: HTMLElement, options: Options) {
  if (!element.ownerDocument?.defaultView) return;
  const document = element.ownerDocument,
    previous = options.returnFocus || (document.activeElement as HTMLElement);
  const visible = () =>
    Array.from(element.querySelectorAll<HTMLElement>(selectors)).filter(
      (el) => el.getClientRects().length > 0 && el.tabIndex >= 0,
    );
  const first = () =>
    element.querySelector<HTMLElement>('[data-initial-focus],[autofocus]') ||
    visible()[0] ||
    element;
  element.tabIndex = -1;
  stack.push(element);
  queueMicrotask(() => {
    if (element.isConnected && stack.at(-1) === element) first().focus();
  });
  const key = (event: KeyboardEvent) => {
    if (stack.at(-1) !== element) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopImmediatePropagation();
      options.close();
      return;
    }
    if (event.key === 'Tab') {
      const controls = visible(),
        current = document.activeElement,
        index = controls.indexOf(current as HTMLElement);
      if (!controls.length) {
        event.preventDefault();
        element.focus();
      } else if (event.shiftKey && index <= 0) {
        event.preventDefault();
        controls.at(-1)!.focus();
      } else if (!event.shiftKey && (index === controls.length - 1 || index < 0)) {
        event.preventDefault();
        controls[0]!.focus();
      }
    }
  };
  const focus = (event: FocusEvent) => {
    if (stack.at(-1) === element && !element.contains(event.target as Node)) first().focus();
  };
  document.addEventListener('keydown', key, true);
  document.addEventListener('focusin', focus, true);
  cleanup.set(element, () => {
    document.removeEventListener('keydown', key, true);
    document.removeEventListener('focusin', focus, true);
    const index = stack.indexOf(element);
    if (index >= 0) stack.splice(index, 1);
    queueMicrotask(() => {
      if (previous?.isConnected) previous.focus();
      else stack.at(-1)?.focus();
    });
  });
}
export const dialog: ObjectDirective<HTMLElement, Options> = {
  mounted: (el, binding) => attach(el, binding.value),
  beforeUnmount(el) {
    cleanup.get(el)?.();
    cleanup.delete(el);
  },
};
