import { loadConnectionPreferences } from './connectionPreferences.ts';

type PreferenceStorage = Pick<Storage, 'getItem' | 'setItem'>;
/** Copy only validated preferences; retain legacy keys for rollback and never overwrite StoneQL. */
export function migrateLegacyPreferences(storage: PreferenceStorage): void {
  const copy = (suffix: string, transform: (raw: string) => string | undefined) => {
    try {
      if (storage.getItem(`stoneql-${suffix}`) !== null) return;
      const raw = storage.getItem(`dbadmin-${suffix}`);
      if (raw === null) return;
      const value = transform(raw);
      if (value !== undefined) storage.setItem(`stoneql-${suffix}`, value);
    } catch { /* Unavailable storage or malformed legacy data must not prevent startup. */ }
  };
  copy('theme', raw => ['light', 'dark', 'system'].includes(raw) ? raw : undefined);
  copy('default-page-size', raw => ['15', '25', '50', '100', '250', '500'].includes(raw) ? raw : undefined);
  copy('connections', raw => {
    const result = loadConnectionPreferences({ getItem: () => raw }, []);
    return result.error ? undefined : JSON.stringify(result);
  });
  copy('grid-widths:v1', raw => {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return;
    const result = Object.fromEntries(Object.entries(parsed).flatMap(([key, widths]) => {
      if (!widths || typeof widths !== 'object' || Array.isArray(widths)) return [];
      const values = Object.entries(widths).filter(([, value]) => typeof value === 'number' && Number.isFinite(value));
      return [[key, Object.fromEntries(values)]];
    }));
    return JSON.stringify(result);
  });
}
