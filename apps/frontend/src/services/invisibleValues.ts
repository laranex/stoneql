/** A run of stored text, or a display-only marker standing in for one invisible character. */
export type ValueSegment =
  | { text: string; marker?: never; raw?: never; label?: never }
  | { marker: string; raw: string; label: string; text?: never };

/** CRLF is one line break, so it gets one marker; both glyphs keep it distinct from a lone LF. */
const MARKERS: Record<string, [marker: string, label: string]> = {
  '\t': ['⇥', 'tab'],
  '\n': ['↵', 'line feed'],
  '\r': ['␍', 'carriage return'],
  '\r\n': ['␍↵', 'CRLF'],
};
const INVISIBLE = /[\t\n\r]/;

/**
 * Splits a cell value into text and marker segments for display only; the raw value is never
 * altered. Each marker carries the character it replaces (for copying) and a spoken label.
 * Returns null (the common fast path) when there is nothing to mark.
 */
export function invisibleSegments(value: unknown): ValueSegment[] | null {
  if (typeof value !== 'string' || !INVISIBLE.test(value)) return null;
  const segments: ValueSegment[] = [];
  let start = 0;
  for (const match of value.matchAll(/\r\n|[\t\n\r]/g)) {
    if (match.index > start) segments.push({ text: value.slice(start, match.index) });
    const [marker, label] = MARKERS[match[0]]!;
    segments.push({ marker, raw: match[0], label });
    start = match.index + match[0].length;
  }
  if (start < value.length) segments.push({ text: value.slice(start) });
  return segments;
}

/**
 * Plain text for a copied grid selection: markers become the characters they stand for, cells are
 * tab-separated and rows newline-separated. Returns null when the selection holds no marker, so
 * the browser's own copy applies.
 */
export function rawSelectionText(fragment: DocumentFragment): string | null {
  if (!fragment.querySelector('[data-raw]')) return null;
  let out = '';
  const walk = (node: Node) => {
    if (node.nodeType === 3) return void (out += node.nodeValue);
    if (node.nodeType !== 1 && node.nodeType !== 11) return;
    const element = node as Element;
    if (element.nodeType === 1) {
      const raw = element.getAttribute('data-raw');
      if (raw !== null) return void (out += raw);
      if (/^T[DH]$/.test(element.tagName) && element.previousElementSibling) out += '\t';
      if (element.tagName === 'TR' && out) out += '\n';
    }
    node.childNodes.forEach(walk);
  };
  walk(fragment);
  return out;
}
