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
/** SET members in schema order, then saved members the current schema no longer lists. */
const members = computed(() => [...(definition.value?.values ?? []), ...unknown.value]);
function choose(event: Event) {
  emit('update:modelValue', (event.target as HTMLSelectElement).value);
}
/** Toggles one SET member; the value stays comma-separated in member order. */
function toggle(member: string, checked: boolean) {
  emit(
    'update:modelValue',
    members.value
      .filter((value) => (value === member ? checked : selected.value.includes(value)))
      .join(','),
  );
}
</script>
<template>
  <div
    v-if="definition?.kind === 'set'"
    class="set-choices"
    role="group"
    :aria-label="label"
    :aria-invalid="invalid || undefined"
    :aria-describedby="describedby"
    :class="{ disabled }"
  >
    <label v-for="member in members" :key="member" class="compact-choice">
      <input
        type="checkbox"
        :checked="selected.includes(member)"
        :disabled="disabled"
        @change="toggle(member, ($event.target as HTMLInputElement).checked)"
      />
      <span>{{ member === '' ? '(empty member)' : member }}</span>
      <span v-if="unknown.includes(member)" class="muted">(not in current schema)</span>
    </label>
    <span v-if="!members.length" class="muted">No SET members in the schema</span>
  </div>
  <select
    v-else
    :value="modelValue"
    :aria-label="label"
    :disabled="disabled"
    :aria-invalid="invalid || undefined"
    :aria-describedby="describedby"
    @change="choose"
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
