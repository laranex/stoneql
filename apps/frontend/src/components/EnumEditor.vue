<script setup lang="ts">
import { computed } from 'vue';
import type { Column, Engine } from '../types';
import { choiceDefinition } from '../services/valueValidation';
const props = withDefaults(
  defineProps<{
    column: Column;
    engine: Engine;
    modelValue?: string;
    label: string;
    disabled?: boolean;
    invalid?: boolean;
    describedby?: string;
  }>(),
  { disabled: false, invalid: false, modelValue: '' },
);
const emit = defineEmits<{ 'update:modelValue': [value: string] }>();
const definition = computed(() => choiceDefinition(props.column, props.engine));
const selected = computed(() => (props.modelValue === '' ? [] : props.modelValue.split(',')));
const unknown = computed(() =>
  definition.value?.kind === 'set'
    ? selected.value.filter((v) => !definition.value!.values.includes(v))
    : !definition.value?.values.includes(props.modelValue) && props.modelValue !== ''
      ? [props.modelValue]
      : [],
);
function change(event: Event) {
  const select = event.target as HTMLSelectElement;
  emit(
    'update:modelValue',
    definition.value?.kind === 'set'
      ? Array.from(select.selectedOptions)
          .map((o) => o.value)
          .join(',')
      : select.value,
  );
}
</script>
<template>
  <select
    :value="definition?.kind === 'set' ? selected : modelValue"
    :multiple="definition?.kind === 'set'"
    :size="definition?.kind === 'set' ? Math.max(2, Math.min(8, definition.values.length + unknown.length)) : undefined"
    :aria-label="label"
    :disabled="disabled"
    :aria-invalid="invalid || undefined"
    :aria-describedby="describedby"
    :class="{ 'set-select': definition?.kind === 'set' }"
    @change="change"
  >
    <option v-if="definition?.kind === 'enum' && !definition.values.includes('')" value="" disabled>
      Choose a value
    </option>
    <option v-for="value in definition?.values" :key="value" :value="value">{{ value }}</option>
    <option v-for="value in unknown" :key="`existing-${value}`" :value="value">
      {{ value }} (not in current schema)
    </option>
  </select>
</template>
