import type { SecretStoreKindDto, SecretStoreStatus } from '../../../../contracts/generated/v1';
import { nativeDatabase } from './nativeHost.ts';
/** Where saved passwords go on this system, and the master-password vault's state (Linux fallback). */
export const secretStoreStatus = () => nativeDatabase<SecretStoreStatus>('secret_store_status', {});
/** Creates (`create`) or unlocks StoneQL's encrypted file. The password crosses to the host once. */
export const unlockSecretStore = (password: string, create: boolean) => nativeDatabase<SecretStoreStatus>('unlock_secret_store', { password, create });
/** Deletes the encrypted file and every password saved in it (forgotten master password). */
export const resetSecretStore = () => nativeDatabase<SecretStoreStatus>('reset_secret_store', {});
/** Proposed minimum master-password length; the host enforces the same bound. */
export const MIN_MASTER_PASSWORD = 12;
const notes: Record<SecretStoreKindDto, string> = {
  keychain: 'Saved in the macOS Keychain.',
  secret_service: 'Saved in the desktop keyring (Secret Service).',
  encrypted_file: 'Saved in StoneQL’s encrypted file (master password): no desktop keyring is running.',
  unavailable: 'Passwords cannot be saved on this system.',
};
export const secretStoreNote = (kind: SecretStoreKindDto | undefined) => (kind ? notes[kind] ?? '' : '');
const placeholders: Partial<Record<SecretStoreKindDto, string>> = { secret_service: 'Saved in keyring', encrypted_file: 'Saved in encrypted file' };
export const savedPlaceholder = (kind: SecretStoreKindDto | undefined) => (kind && placeholders[kind]) || 'Saved in Keychain';
/** A validation problem, or '' when the entry (and, when creating, its repeat) can be submitted. */
export function masterPasswordProblem(password: string, repeat: string, create: boolean): string {
  if (!password) return 'Enter the master password.';
  if (!create) return '';
  if ([...password].length < MIN_MASTER_PASSWORD) return `Use at least ${MIN_MASTER_PASSWORD} characters.`;
  return password === repeat ? '' : 'The two entries do not match.';
}
