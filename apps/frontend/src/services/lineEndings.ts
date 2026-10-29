/** How a stored value breaks lines; `keep` maps edited breaks back onto the original mixed endings. */
export type LineEnding = 'lf' | 'crlf' | 'cr';
export type LineEndingChoice = LineEnding | 'keep';
const BREAK = /\r\n|\r|\n/g;
const CHARS: Record<LineEnding, string> = { lf: '\n', crlf: '\r\n', cr: '\r' };

export function hasLineBreak(value: unknown): value is string {
  return typeof value === 'string' && /[\r\n]/.test(value);
}
function breaks(value: string): string[] {
  return value.match(BREAK) ?? [];
}
/** The single style a value uses, `mixed`, or null when it has no line breaks. */
export function lineEndingStyle(value: string): LineEnding | 'mixed' | null {
  const found = new Set(breaks(value));
  if (!found.size) return null;
  if (found.size > 1) return 'mixed';
  const [only] = found;
  return only === '\r\n' ? 'crlf' : only === '\r' ? 'cr' : 'lf';
}
/** What a browser textarea reports for a value: every CRLF and CR becomes LF. */
export function normalizeLineEndings(value: string): string {
  return value.replace(BREAK, '\n');
}
/** The choice an editor starts with: the original's own style, `keep` for mixed, LF when it had none. */
export function defaultLineEnding(original: string): LineEndingChoice {
  const style = lineEndingStyle(original);
  return style === 'mixed' ? 'keep' : (style ?? 'lf');
}
/**
 * Rebuilds the stored string from LF-normalized textarea text. Unchanged text with the default
 * choice returns the original exactly. `keep` gives the n-th break the original n-th ending and
 * returns null when the number of breaks changed, because then no ending can be inferred.
 */
export function applyLineEndings(text: string, choice: LineEndingChoice, original: string): string | null {
  if (text === normalizeLineEndings(original) && choice === defaultLineEnding(original)) return original;
  if (choice !== 'keep') return text.replace(/\n/g, CHARS[choice]);
  const endings = breaks(original);
  if ((text.match(/\n/g)?.length ?? 0) !== endings.length) return null;
  let i = 0;
  return text.replace(/\n/g, () => endings[i++]!);
}
