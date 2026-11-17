//! Windows provider: one DPAPI-encrypted file per reference under `%APPDATA%\<service>\`.
//!
//! DPAPI (current-user scope) ties each file to the Windows account, so a copied file or another
//! account cannot decrypt it. Unlike the macOS Keychain it has no per-app access list: any program
//! running as the same Windows user can decrypt the files.
use super::SecretError;
use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};
use stoneql_dpapi::{protect, unprotect};

pub(super) fn set(service: &str, account: &str, value: &[u8]) -> Result<(), SecretError> {
    let path = item_path(service, account)?;
    let sealed = protect(value, &entropy(service, account)).map_err(|_| SecretError::Failed)?;
    let dir = path.parent().ok_or(SecretError::Failed)?;
    fs::create_dir_all(dir).map_err(|_| SecretError::Failed)?;
    let temp = dir.join(format!("{account}.tmp"));
    let written = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)?;
        file.write_all(&sealed)?;
        file.sync_all()?;
        // `rename` replaces an existing item atomically (MoveFileExW with MOVEFILE_REPLACE_EXISTING).
        fs::rename(&temp, &path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temp);
        return Err(SecretError::Failed);
    }
    Ok(())
}

pub(super) fn get(service: &str, account: &str) -> Result<Vec<u8>, SecretError> {
    let sealed = fs::read(item_path(service, account)?).map_err(not_found_or_failed)?;
    unprotect(&sealed, &entropy(service, account)).map_err(|_| SecretError::Failed)
}

pub(super) fn delete(service: &str, account: &str) -> Result<(), SecretError> {
    fs::remove_file(item_path(service, account)?).map_err(not_found_or_failed)
}

fn not_found_or_failed(error: io::Error) -> SecretError {
    if error.kind() == io::ErrorKind::NotFound {
        SecretError::NotFound
    } else {
        SecretError::Failed
    }
}

/// Accounts are opaque reference IDs; anything else could name a path outside the store.
fn item_path(service: &str, account: &str) -> Result<PathBuf, SecretError> {
    let safe = |name: &str| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_'))
            && !name.starts_with('.')
    };
    if !safe(service) || !safe(account) {
        return Err(SecretError::Failed);
    }
    let base = std::env::var_os("APPDATA").ok_or(SecretError::Failed)?;
    let base = Path::new(&base);
    if !base.is_absolute() {
        return Err(SecretError::Failed);
    }
    Ok(base.join(service).join(format!("{account}.bin")))
}

/// Binds each ciphertext to its service and reference, so a file renamed to another reference
/// does not decrypt.
fn entropy(service: &str, account: &str) -> Vec<u8> {
    format!("stoneql-secret-v1\0{service}\0{account}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> String {
        format!("me.thixpin.stoneql.test-{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn items_round_trip_and_are_encrypted_at_rest() {
        let service = service();
        let account = uuid::Uuid::new_v4().to_string();
        set(&service, &account, b"s3cret-value").unwrap();
        let on_disk = fs::read(item_path(&service, &account).unwrap()).unwrap();
        assert!(!on_disk.windows(12).any(|window| window == b"s3cret-value"));
        assert_eq!(get(&service, &account).unwrap(), b"s3cret-value");
        set(&service, &account, b"replaced").unwrap();
        assert_eq!(get(&service, &account).unwrap(), b"replaced");
        delete(&service, &account).unwrap();
        assert_eq!(get(&service, &account), Err(SecretError::NotFound));
        assert_eq!(delete(&service, &account), Err(SecretError::NotFound));
        let _ = fs::remove_dir(item_path(&service, &account).unwrap().parent().unwrap());
    }

    #[test]
    fn a_file_copied_to_another_reference_does_not_decrypt() {
        let service = service();
        let (first, second) = (
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        );
        set(&service, &first, b"value").unwrap();
        let (from, to) = (
            item_path(&service, &first).unwrap(),
            item_path(&service, &second).unwrap(),
        );
        fs::copy(&from, &to).unwrap();
        assert_eq!(get(&service, &second), Err(SecretError::Failed));
        let _ = fs::remove_dir_all(from.parent().unwrap());
    }

    #[test]
    fn names_that_could_leave_the_store_are_refused() {
        for account in ["", "..", "../x", "a\\b", "a/b", "C:x", ".hidden"] {
            assert_eq!(
                set("me.thixpin.stoneql.test", account, b"v"),
                Err(SecretError::Failed)
            );
        }
        assert_eq!(set("..", "a", b"v"), Err(SecretError::Failed));
    }
}
