import { defineStore } from 'pinia';
import { ref } from 'vue';
import { defaultExport } from '../types/export';
import type { ExportConfig } from '../types/export';
export const useExportStore = defineStore('export', () => {
  const preferences = ref<Record<string, ExportConfig>>({});
  function get(key: string, table: string) {
    if (!preferences.value[key]) {
      const c = defaultExport();
      c.tables = table ? [table] : [];
      preferences.value[key] = c;
    }
    return preferences.value[key]!;
  }
  return { preferences, get };
});
