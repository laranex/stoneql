<script setup lang="ts">
import { ref, watch } from 'vue';
import { X, CheckCircle2, AlertCircle } from 'lucide-vue-next';
const props = withDefaults(
  defineProps<{ kind?: 'success' | 'warning' | 'error'; notificationKey?: string | number }>(),
  { kind: 'success', notificationKey: '' },
);
const dismissed = ref(false);
watch(
  () => [props.notificationKey, props.kind],
  () => {
    dismissed.value = false;
  },
);
</script>
<template>
  <div
    v-if="!dismissed"
    class="notice result-notice"
    :class="kind"
    :role="kind === 'success' ? 'status' : 'alert'"
  >
    <CheckCircle2 v-if="kind === 'success'" :size="15" /><AlertCircle v-else :size="15" />
    <span class="notice-message"><slot /></span><slot name="end" />
    <button
      type="button"
      class="notice-dismiss"
      aria-label="Dismiss query notification"
      title="Dismiss notification"
      @click="dismissed = true"
    >
      <X :size="13" aria-hidden="true" />
    </button>
  </div>
</template>
