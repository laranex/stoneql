import type { SecretStoreKindDto, SecretStoreStatus } from '../../../../contracts/generated/v1';
import { nativeDatabase } from './nativeHost.ts';
/** Where saved passwords go on this system, and the master-password vault's state (Linux fallback). */
export const secretStoreStatus = () => nativeDatabase<SecretStoreStatus>('secret_store_status', {});
/** Creates (`create`) or unlocks StoneQL's encrypted file. The password crosses to the host once. */
export const unlockSecretStore = (password: string, create: boolean) => nativeDatabase<SecretStoreStatus>('unlock_secret_store', { password, create });
/** Deletes the encrypted file and every password saved in it (forgotten master password). */
export const resetSecretStore = () => nativeDatabase<SecretStoreStatus>('reset_secret_store', {});
/** Minimum master-password length; the host enforces the same bound. */
export const MIN_MASTER_PASSWORD = 8;
/** Where a new password will be saved (the connection dialog's note). */
const notes: Record<SecretStoreKindDto, string> = {
  keychain: 'New passwords are saved in the macOS Keychain.',
  secret_service: 'New passwords are saved in the desktop keyring (Secret Service).',
  encrypted_file: 'New passwords are saved in StoneQL’s encrypted file (master password): no desktop keyring is running.',
  unavailable: 'Passwords cannot be saved on this system.',
};
export const secretStoreNote = (kind: SecretStoreKindDto | undefined) => (kind ? notes[kind] ?? '' : '');
/**
 * The store holding an existing reference, given where new saves go (`kind`, undefined when
 * unknown). Version-8 UUIDs are only made by StoneQL's encrypted file (Linux); version-4 ones are
 * the OS store: the Keychain on macOS, the Secret Service on Linux. Undefined when it cannot be told.
 */
export function referenceStore(reference: string | undefined, kind: SecretStoreKindDto | undefined): SecretStoreKindDto | undefined {
  const version = /^[0-9a-f]{8}-[0-9a-f]{4}-([0-9a-f])[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$/i.exec(reference ?? '')?.[1];
  if (version === '8') return 'encrypted_file';
  if (version !== '4') return undefined;
  if (kind === 'keychain') return 'keychain';
  return kind === 'secret_service' || kind === 'encrypted_file' ? 'secret_service' : undefined;
}
const placeholders: Partial<Record<SecretStoreKindDto, string>> = { keychain: 'Saved in Keychain', secret_service: 'Saved in keyring', encrypted_file: 'Saved in encrypted file' };
/** Placeholder of a password field whose password is already saved under `reference`. */
export const savedPlaceholder = (reference: string | undefined, kind: SecretStoreKindDto | undefined) =>
  placeholders[referenceStore(reference, kind) ?? 'unavailable'] ?? 'Saved password';
/** A validation problem, or '' when the entry (and, when creating, its repeat) can be submitted. */
export function masterPasswordProblem(password: string, repeat: string, create: boolean): string {
  if (!password) return 'Enter the master password.';
  if (!create) return '';
  if ([...password].length < MIN_MASTER_PASSWORD) return `Use at least ${MIN_MASTER_PASSWORD} characters.`;
  return password === repeat ? '' : 'The two entries do not match.';
}
