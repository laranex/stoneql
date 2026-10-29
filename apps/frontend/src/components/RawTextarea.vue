<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import {
  applyLineEndings,
  defaultLineEnding,
  lineEndingStyle,
  normalizeLineEndings,
  type LineEndingChoice,
} from '../services/lineEndings';
// A textarea's value always reads back with LF line breaks, so it holds LF-normalized text and the
// stored string is rebuilt from the original and the visible line-ending choice; never from `.value`.
const props = withDefaults(
  defineProps<{
    modelValue?: string;
    label: string;
    disabled?: boolean;
    invalid?: boolean;
    describedby?: string;
    rows?: number;
  }>(),
  { modelValue: '', disabled: false, invalid: false, rows: 2 },
);
const emit = defineEmits<{ 'update:modelValue': [value: string] }>();
/** Set while the edited text cannot get line endings without an explicit choice; blocks saving. */
const issue = defineModel<string>('issue', { default: '' });
const original = ref(''),
  text = ref(''),
  choice = ref<LineEndingChoice>('lf');
// Our last emit, so its echo does not reset the editor. Cleared on every prop change: an emit equal to
// the current prop fires no watcher, and a stale marker would hide a later external value.
let emitted: string | undefined;
watch(
  () => props.modelValue,
  (value) => {
    const echo = value === emitted;
    emitted = undefined;
    if (echo) return;
    original.value = value;
    text.value = normalizeLineEndings(value);
    choice.value = defaultLineEnding(value);
    issue.value = '';
  },
  { immediate: true },
);
const mixed = computed(() => lineEndingStyle(original.value) === 'mixed');
const showChoice = computed(() => /[\r\n]/.test(original.value) || text.value.includes('\n'));
function update() {
  const raw = applyLineEndings(text.value, choice.value, original.value);
  issue.value = raw === null ? 'The number of lines changed; choose LF, CRLF or CR line endings.' : '';
  emitted = raw ?? text.value;
  emit('update:modelValue', emitted);
}
// The textarea model is the LF text it really holds, so Vue never rewrites it (or moves the caret).
const edited = computed({
  get: () => text.value,
  set(value: string) {
    text.value = normalizeLineEndings(value);
    update();
  },
});
const ending = computed({
  get: () => choice.value,
  set(value: LineEndingChoice) {
    choice.value = value;
    update();
  },
});
</script>
<template>
  <textarea
    v-model="edited"
    :aria-label="label"
    :disabled="disabled"
    :aria-invalid="invalid || undefined"
    :aria-describedby="describedby"
    :rows="rows"
  ></textarea
  ><select
    v-if="showChoice"
    v-model="ending"
    class="line-ending-select"
    aria-label="Line endings"
    title="Line endings written when this value is saved"
    :disabled="disabled"
    :aria-invalid="!!issue || undefined"
    :aria-describedby="issue ? describedby : undefined"
  >
    <option v-if="mixed" value="keep">Mixed (as stored)</option>
    <option value="lf">LF</option>
    <option value="crlf">CRLF</option>
    <option value="cr">CR</option>
  </select>
</template>
