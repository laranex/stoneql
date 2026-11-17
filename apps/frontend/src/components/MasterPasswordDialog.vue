<script setup lang="ts">
import { computed, ref } from 'vue';
import { X, KeyRound } from 'lucide-vue-next';
import { masterPasswordProblem, MIN_MASTER_PASSWORD } from '../services/secretStore';
import { dialog as vDialog } from '../directives/dialog';
/**
 * Master password for StoneQL's encrypted file of saved passwords (Linux without a Secret
 * Service). Create asks twice; unlock asks once per app session and offers a reset when the
 * password is forgotten. The entry is handed to the host and cleared here at once.
 */
const props = defineProps<{ mode: 'create' | 'unlock'; busy?: boolean; error?: string }>();
const emit = defineEmits<{ submit: [password: string]; cancel: []; reset: [] }>();
const password = ref(''), repeat = ref(''), touched = ref(false);
const create = computed(() => props.mode === 'create');
const problem = computed(() => masterPasswordProblem(password.value, repeat.value, create.value));
function submit() {
  touched.value = true;
  if (problem.value || props.busy) return;
  const value = password.value;
  password.value = ''; repeat.value = ''; touched.value = false;
  emit('submit', value);
}
function cancel() { if (!props.busy) emit('cancel'); }
</script>
<template>
  <div class="modal-overlay upper" @mousedown.self="cancel">
    <section class="modal master-modal" v-dialog="{ close: cancel }" role="dialog" aria-modal="true" aria-labelledby="master-title" data-testid="master-password-dialog">
      <header>
        <h2 id="master-title"><KeyRound :size="16" aria-hidden="true" /> {{ create ? 'Set a master password' : 'Unlock saved passwords' }}</h2>
        <button class="icon-button" type="button" aria-label="Close" :disabled="busy" @click="cancel"><X :size="17" /></button>
      </header>
      <form class="modal-body" @submit.prevent="submit">
        <p class="muted">{{ create
          ? `No desktop keyring is running, so StoneQL saves passwords in its own encrypted file. Choose a master password of at least ${MIN_MASTER_PASSWORD} characters. You enter it once each time StoneQL starts, and it cannot be recovered.`
          : 'Saved passwords are in StoneQL’s encrypted file. Enter the master password to use them until StoneQL quits.' }}</p>
        <label class="form-label">Master password<input v-model="password" data-initial-focus type="password" autocomplete="off" :disabled="busy" :aria-invalid="touched && !!problem" /></label>
        <label v-if="create" class="form-label">Repeat master password<input v-model="repeat" type="password" autocomplete="off" :disabled="busy" /></label>
        <p v-if="touched && problem" class="notice warning" role="status">{{ problem }}</p>
        <p v-if="error" class="notice error" role="alert">{{ error }}</p>
        <footer>
          <button v-if="!create" class="button reset-link" type="button" :disabled="busy" @click="emit('reset')" title="Forgot the master password? Deletes every password saved in the encrypted file">Reset saved passwords…</button>
          <button class="button" type="button" :disabled="busy" @click="cancel">Cancel</button>
          <button class="button primary" type="submit" :disabled="busy">{{ busy ? (create ? 'Creating…' : 'Unlocking…') : create ? 'Create' : 'Unlock' }}</button>
        </footer>
      </form>
    </section>
  </div>
</template>
<style scoped>
.master-modal { width: min(440px, calc(100vw - 32px)); }
.master-modal h2 { display: inline-flex; align-items: center; gap: 8px; }
.reset-link { margin-right: auto; }
</style>
