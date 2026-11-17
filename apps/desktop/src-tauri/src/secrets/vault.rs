//! Linux fallback provider: StoneQL's encrypted file vault, used only when no Secret Service is
//! on the session bus. One file per reference in a private directory
//! (`$XDG_DATA_HOME/<identifier>/credentials/`, 0700), each sealed with AES-256-GCM under a key
//! derived from the user's master password with Argon2id.
//!
//! Threat model: it protects saved passwords in a copied or backed-up file. It does not protect
//! against code running as the same user while StoneQL is unlocked, nor against a weak master
//! password. The derived key lives only in memory for the app session (zeroized on drop); the
//! master password is never stored.
//!
//! `header` (96 bytes): magic, Argon2id memory (KiB), iterations, lanes (u32 LE), 16-byte salt,
//! then a 12-byte nonce and the sealed 32-byte verifier whose AAD is everything before it, so a
//! wrong password or an edited header is detected without trying items.
//! `<reference>.item`: magic, 12-byte nonce, sealed value; the AAD binds the format version,
//! service and reference, so a file copied or renamed to another reference does not open.
use super::{SecretError, UnlockError};
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use argon2::{Algorithm, Argon2, Params, Version};
use nix::{
    errno::Errno,
    fcntl::{AT_FDCWD, OFlag, openat, renameat},
    sys::stat::Mode,
    unistd::{UnlinkatFlags, unlinkat},
};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use zeroize::Zeroizing;

const HEADER: &str = "header";
const HEADER_MAGIC: &[u8; 8] = b"SQLVLT1\0";
const ITEM_MAGIC: &[u8; 8] = b"SQLITM1\0";
const HEADER_LEN: usize = 8 + 12 + 16 + 12 + 32 + 16;
/// Proposed: about 64 MiB and 3 passes (measured on the build server, see the Linux plan).
const MEMORY_KIB: u32 = 64 * 1024;
const ITERATIONS: u32 = 3;
const LANES: u32 = 1;
/// Minimum master-password length (characters), approved 2026-11-17.
pub const MIN_PASSWORD_CHARS: usize = 8;
/// Upper bound on an item file (a 16 KiB secret plus its JSON scope and the AEAD overhead).
const MAX_ITEM_BYTES: u64 = 64 * 1024;
/// Fixed delay after a wrong master password; there is no lockout and nothing is wiped.
const WRONG_PASSWORD_DELAY: Duration = Duration::from_secs(1);

const SAFE: OFlag = OFlag::O_NOFOLLOW
    .union(OFlag::O_NONBLOCK)
    .union(OFlag::O_NOCTTY)
    .union(OFlag::O_CLOEXEC);
const PRIVATE_FILE: Mode = Mode::S_IRUSR.union(Mode::S_IWUSR);

pub struct Vault {
    dir: PathBuf,
    key: Mutex<Option<Zeroizing<[u8; 32]>>>,
}
impl Vault {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            key: Mutex::new(None),
        }
    }
    /// `$XDG_DATA_HOME` (absolute) or `~/.local/share`, then `<identifier>/credentials`.
    pub fn default_dir(identifier: &str) -> Option<PathBuf> {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .map(|home| home.join(".local/share"))
            })?;
        Some(base.join(identifier).join("credentials"))
    }
    pub fn unlocked(&self) -> bool {
        self.key.lock().unwrap().is_some()
    }
    /// Whether a vault header exists. Does not create the directory.
    pub fn exists(&self) -> Result<bool, SecretError> {
        if !self.dir.exists() {
            return Ok(false);
        }
        Ok(read_private(&open_dir(&self.dir, false)?, HEADER, HEADER_LEN as u64)?.is_some())
    }

    /// Creates the vault (`create`) or unlocks the existing one. The password is zeroized by
    /// the caller's `Zeroizing`; only the derived key is kept.
    pub fn unlock(&self, password: &str, create: bool) -> Result<(), UnlockError> {
        let dir = open_dir(&self.dir, create).map_err(|_| UnlockError::Failed)?;
        let header =
            read_private(&dir, HEADER, HEADER_LEN as u64).map_err(|_| UnlockError::Failed)?;
        let key = match (header, create) {
            (None, true) => {
                if password.chars().count() < MIN_PASSWORD_CHARS {
                    return Err(UnlockError::Invalid);
                }
                let mut salt = [0u8; 16];
                random(&mut salt).map_err(|_| UnlockError::Failed)?;
                let mut header = Vec::with_capacity(HEADER_LEN);
                header.extend_from_slice(HEADER_MAGIC);
                for value in [MEMORY_KIB, ITERATIONS, LANES] {
                    header.extend_from_slice(&value.to_le_bytes());
                }
                header.extend_from_slice(&salt);
                let key = derive(password, &salt, MEMORY_KIB, ITERATIONS, LANES)
                    .map_err(|_| UnlockError::Failed)?;
                let sealed = seal(&key, &[0u8; 32], &verifier_aad(&header))
                    .map_err(|_| UnlockError::Failed)?;
                header.extend_from_slice(&sealed);
                write_atomic(&dir, HEADER, &header).map_err(|_| UnlockError::Failed)?;
                key
            }
            (Some(header), false) => {
                let (memory, iterations, lanes, salt) =
                    parse_header(&header).ok_or(UnlockError::Rejected)?;
                let key = derive(password, salt, memory, iterations, lanes)
                    .map_err(|_| UnlockError::Failed)?;
                let prefix = &header[..36];
                match open(&key, &header[36..], &verifier_aad(prefix)) {
                    Ok(verifier) if verifier[..] == [0u8; 32] => key,
                    _ => {
                        std::thread::sleep(WRONG_PASSWORD_DELAY);
                        return Err(UnlockError::Rejected);
                    }
                }
            }
            _ => return Err(UnlockError::Invalid),
        };
        *self.key.lock().unwrap() = Some(key);
        Ok(())
    }

    /// Deletes the header and every item (only the vault's own files), then the directory if
    /// it is empty, and forgets the key. Saved passwords in the vault are lost.
    pub fn reset(&self) -> Result<(), SecretError> {
        *self.key.lock().unwrap() = None;
        if !self.dir.exists() {
            return Ok(());
        }
        let dir = open_dir(&self.dir, false)?;
        for entry in fs::read_dir(&self.dir).map_err(|_| SecretError::Failed)? {
            let name = entry.map_err(|_| SecretError::Failed)?.file_name();
            let Some(name) = name.to_str() else { continue };
            let ours = name == HEADER
                || name == "header.tmp"
                || name
                    .strip_suffix(".item")
                    .or_else(|| name.strip_suffix(".tmp"))
                    .is_some_and(account_ok);
            if ours {
                remove(&dir, name)?;
            }
        }
        dir.sync_all().map_err(|_| SecretError::Failed)?;
        let _ = fs::remove_dir(&self.dir);
        Ok(())
    }

    pub fn set(&self, service: &str, account: &str, value: &[u8]) -> Result<(), SecretError> {
        if (value.len() + ITEM_MAGIC.len() + 12 + 16) as u64 > MAX_ITEM_BYTES {
            return Err(SecretError::Failed);
        }
        let key = self.key()?;
        let dir = open_dir(&self.dir, false)?;
        let mut file = ITEM_MAGIC.to_vec();
        file.extend_from_slice(&seal(&key, value, &item_aad(service, account)?)?);
        write_atomic(&dir, &item_name(account)?, &file)
    }
    pub fn get(&self, service: &str, account: &str) -> Result<Vec<u8>, SecretError> {
        // Without a vault (never created, or reset) the item is gone: no point asking to create one.
        if !self.unlocked() && !self.exists()? {
            return Err(SecretError::NotFound);
        }
        let key = self.key()?;
        let dir = open_dir(&self.dir, false)?;
        let file = read_private(&dir, &item_name(account)?, MAX_ITEM_BYTES)?
            .ok_or(SecretError::NotFound)?;
        let sealed = file.strip_prefix(ITEM_MAGIC).ok_or(SecretError::Failed)?;
        open(&key, sealed, &item_aad(service, account)?)
            .map(|value| value.to_vec())
            .map_err(|_| SecretError::Failed)
    }
    /// Deleting needs no key: forgetting a reference must work while the vault is locked.
    pub fn delete(&self, account: &str) -> Result<(), SecretError> {
        if !self.dir.exists() {
            return Err(SecretError::NotFound);
        }
        let dir = open_dir(&self.dir, false)?;
        remove(&dir, &item_name(account)?)?;
        dir.sync_all().map_err(|_| SecretError::Failed)
    }

    fn key(&self) -> Result<Zeroizing<[u8; 32]>, SecretError> {
        self.key.lock().unwrap().clone().ok_or(SecretError::Locked)
    }
}

fn random(buffer: &mut [u8]) -> Result<(), SecretError> {
    getrandom::fill(buffer).map_err(|_| SecretError::Failed)
}
fn derive(
    password: &str,
    salt: &[u8],
    memory: u32,
    iterations: u32,
    lanes: u32,
) -> Result<Zeroizing<[u8; 32]>, SecretError> {
    let params =
        Params::new(memory, iterations, lanes, Some(32)).map_err(|_| SecretError::Failed)?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| SecretError::Failed)?;
    Ok(key)
}
/// Refuses parameters outside sane bounds, so an edited header cannot weaken the KDF or make
/// unlocking exhaust memory.
fn parse_header(header: &[u8]) -> Option<(u32, u32, u32, &[u8])> {
    if header.len() != HEADER_LEN || &header[..8] != HEADER_MAGIC {
        return None;
    }
    let word = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
    let (memory, iterations, lanes) = (word(8), word(12), word(16));
    let sane = (MEMORY_KIB..=1024 * 1024).contains(&memory)
        && (ITERATIONS..=16).contains(&iterations)
        && (1..=8).contains(&lanes);
    sane.then_some((memory, iterations, lanes, &header[20..36]))
}
fn verifier_aad(prefix: &[u8]) -> Vec<u8> {
    [b"stoneql-vault-verifier-v1\0".as_slice(), prefix].concat()
}
fn item_aad(service: &str, account: &str) -> Result<Vec<u8>, SecretError> {
    if service.contains('\0') || !account_ok(account) {
        return Err(SecretError::Failed);
    }
    Ok(format!("stoneql-vault-item-v1\0{service}\0{account}").into_bytes())
}
/// `nonce || ciphertext` under a fresh random nonce.
fn seal(key: &[u8; 32], value: &[u8], aad: &[u8]) -> Result<Vec<u8>, SecretError> {
    let mut nonce = [0u8; 12];
    random(&mut nonce)?;
    let cipher = Aes256Gcm::new(key.into());
    let sealed = cipher
        .encrypt(&Nonce::from(nonce), Payload { msg: value, aad })
        .map_err(|_| SecretError::Failed)?;
    Ok([nonce.as_slice(), &sealed].concat())
}
fn open(key: &[u8; 32], sealed: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>, SecretError> {
    if sealed.len() < 12 + 16 {
        return Err(SecretError::Failed);
    }
    let (nonce, ciphertext) = sealed.split_at(12);
    let nonce: [u8; 12] = nonce.try_into().unwrap();
    Aes256Gcm::new(key.into())
        .decrypt(
            &Nonce::from(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| SecretError::Failed)
}

/// References are hyphenated UUIDs; anything else could name another file.
fn account_ok(account: &str) -> bool {
    uuid::Uuid::parse_str(account).is_ok_and(|id| id.hyphenated().to_string() == account)
}
fn item_name(account: &str) -> Result<String, SecretError> {
    if !account_ok(account) {
        return Err(SecretError::Failed);
    }
    Ok(format!("{account}.item"))
}

/// Owned by this user, the expected type and no group or other bits (checked on the open
/// descriptor, as the outcome journal does).
fn private(meta: &fs::Metadata, dir: bool) -> Result<(), SecretError> {
    let kind = if dir { meta.is_dir() } else { meta.is_file() };
    if !kind || meta.uid() != nix::unistd::geteuid().as_raw() || meta.mode() & 0o077 != 0 {
        return Err(SecretError::Failed);
    }
    Ok(())
}
/// Opens the vault directory without following a final symlink and validates it by
/// descriptor; `create` makes it (0700) first. Every file operation is relative to it.
fn open_dir(path: &Path, create: bool) -> Result<File, SecretError> {
    if create {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| SecretError::Failed)?;
        }
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {
                if let Some(parent) = path.parent() {
                    File::open(parent)
                        .and_then(|parent| parent.sync_all())
                        .map_err(|_| SecretError::Failed)?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(SecretError::Failed),
        }
    }
    let dir = match openat(
        AT_FDCWD,
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | SAFE,
        Mode::empty(),
    ) {
        Ok(fd) => File::from(fd),
        Err(Errno::ENOENT) => return Err(SecretError::NotFound),
        Err(_) => return Err(SecretError::Failed),
    };
    private(&dir.metadata().map_err(|_| SecretError::Failed)?, true)?;
    Ok(dir)
}
/// A private regular file's contents (at most `limit` bytes), `None` when it does not exist.
fn read_private(dir: &File, name: &str, limit: u64) -> Result<Option<Vec<u8>>, SecretError> {
    let file = match openat(dir, name, OFlag::O_RDONLY | SAFE, Mode::empty()) {
        Ok(fd) => File::from(fd),
        Err(Errno::ENOENT) => return Ok(None),
        Err(_) => return Err(SecretError::Failed),
    };
    private(&file.metadata().map_err(|_| SecretError::Failed)?, false)?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SecretError::Failed)?;
    if bytes.len() as u64 > limit {
        return Err(SecretError::Failed);
    }
    Ok(Some(bytes))
}
/// Temporary file (0600, exclusive), fsync, rename over `name`, then fsync the directory.
fn write_atomic(dir: &File, name: &str, bytes: &[u8]) -> Result<(), SecretError> {
    let temp = format!("{}.tmp", name.trim_end_matches(".item"));
    let _ = unlinkat(dir, temp.as_str(), UnlinkatFlags::NoRemoveDir);
    let flags = OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | SAFE;
    let written = (|| {
        let mut file = File::from(openat(dir, temp.as_str(), flags, PRIVATE_FILE).ok()?);
        file.write_all(bytes).ok()?;
        file.sync_all().ok()?;
        renameat(dir, temp.as_str(), dir, name).ok()?;
        dir.sync_all().ok()
    })();
    if written.is_none() {
        let _ = unlinkat(dir, temp.as_str(), UnlinkatFlags::NoRemoveDir);
        return Err(SecretError::Failed);
    }
    Ok(())
}
fn remove(dir: &File, name: &str) -> Result<(), SecretError> {
    match unlinkat(dir, name, UnlinkatFlags::NoRemoveDir) {
        Ok(()) => Ok(()),
        Err(Errno::ENOENT) => Err(SecretError::NotFound),
        Err(_) => Err(SecretError::Failed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const SERVICE: &str = "me.thixpin.stoneql.test";
    const PASSWORD: &str = "synthetic master password";

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("stoneql-vault-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn vault(&self) -> Vault {
            Vault::new(self.0.join("app").join("credentials"))
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn reference() -> String {
        uuid::Uuid::new_v4().to_string()
    }
    fn created(temp: &Temp) -> Vault {
        let vault = temp.vault();
        vault.unlock(PASSWORD, true).unwrap();
        vault
    }

    #[test]
    fn items_round_trip_after_unlocking_with_the_master_password() {
        let temp = Temp::new();
        let vault = temp.vault();
        assert!(!vault.exists().unwrap());
        assert_eq!(
            vault.set(SERVICE, &reference(), b"v"),
            Err(SecretError::Locked)
        );
        assert_eq!(vault.get(SERVICE, &reference()), Err(SecretError::NotFound));
        assert_eq!(vault.unlock("7 chars", true), Err(UnlockError::Invalid));
        assert_eq!(vault.unlock(PASSWORD, false), Err(UnlockError::Invalid));
        vault.unlock(PASSWORD, true).unwrap();
        assert!(vault.exists().unwrap());
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&vault.dir), 0o700);
        let account = reference();
        vault.set(SERVICE, &account, b"synthetic-value").unwrap();
        assert_eq!(mode(&vault.dir.join(format!("{account}.item"))), 0o600);
        assert_eq!(mode(&vault.dir.join(HEADER)), 0o600);
        assert_eq!(vault.get(SERVICE, &account).unwrap(), b"synthetic-value");
        vault.set(SERVICE, &account, b"replaced").unwrap();
        assert_eq!(vault.get(SERVICE, &account).unwrap(), b"replaced");
        assert_eq!(vault.get(SERVICE, &reference()), Err(SecretError::NotFound));
        assert_eq!(
            vault.get("other.service", &account),
            Err(SecretError::Failed)
        );
        let too_large = vec![b'x'; MAX_ITEM_BYTES as usize];
        assert_eq!(
            vault.set(SERVICE, &reference(), &too_large),
            Err(SecretError::Failed)
        );

        // A new app session starts locked and unlocks with the same password.
        let next = temp.vault();
        assert!(!next.unlocked());
        assert_eq!(next.get(SERVICE, &account), Err(SecretError::Locked));
        assert_eq!(next.unlock(PASSWORD, true), Err(UnlockError::Invalid));
        next.unlock(PASSWORD, false).unwrap();
        assert_eq!(next.get(SERVICE, &account).unwrap(), b"replaced");
        next.delete(&account).unwrap();
        assert_eq!(next.get(SERVICE, &account), Err(SecretError::NotFound));
        assert_eq!(next.delete(&account), Err(SecretError::NotFound));
        // Only the header remains: no temporary or plaintext files.
        let names: Vec<_> = fs::read_dir(&next.dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [HEADER]);
    }

    #[test]
    fn a_wrong_master_password_is_refused_without_wiping_anything() {
        let temp = Temp::new();
        let account = reference();
        created(&temp).set(SERVICE, &account, b"value").unwrap();
        let vault = temp.vault();
        let started = std::time::Instant::now();
        assert_eq!(
            vault.unlock("synthetic wrong password", false),
            Err(UnlockError::Rejected)
        );
        assert!(started.elapsed() >= WRONG_PASSWORD_DELAY);
        assert!(!vault.unlocked());
        vault.unlock(PASSWORD, false).unwrap();
        assert_eq!(vault.get(SERVICE, &account).unwrap(), b"value");
    }

    #[test]
    fn copied_renamed_or_tampered_files_do_not_open() {
        let temp = Temp::new();
        let vault = created(&temp);
        let (first, second) = (reference(), reference());
        vault.set(SERVICE, &first, b"value").unwrap();
        let path = |account: &str| vault.dir.join(format!("{account}.item"));
        fs::copy(path(&first), path(&second)).unwrap();
        fs::set_permissions(path(&second), fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(vault.get(SERVICE, &second), Err(SecretError::Failed));
        let mut bytes = fs::read(path(&first)).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(path(&first), &bytes).unwrap();
        assert_eq!(vault.get(SERVICE, &first), Err(SecretError::Failed));

        // An edited header (salt or KDF parameters) reads as a wrong password; weaker
        // parameters are refused outright.
        let header = vault.dir.join(HEADER);
        let original = fs::read(&header).unwrap();
        for (at, value) in [(25usize, None), (8, Some(1024u32)), (12, Some(1))] {
            let mut edited = original.clone();
            match value {
                None => edited[at] ^= 1,
                Some(value) => edited[at..at + 4].copy_from_slice(&value.to_le_bytes()),
            }
            fs::write(&header, &edited).unwrap();
            assert_eq!(
                temp.vault().unlock(PASSWORD, false),
                Err(UnlockError::Rejected)
            );
        }
    }

    #[test]
    fn insecure_directories_files_and_symlinks_are_refused() {
        let temp = Temp::new();
        let vault = created(&temp);
        let account = reference();
        vault.set(SERVICE, &account, b"value").unwrap();
        let item = vault.dir.join(format!("{account}.item"));

        fs::set_permissions(&item, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(vault.get(SERVICE, &account), Err(SecretError::Failed));
        fs::set_permissions(&item, fs::Permissions::from_mode(0o600)).unwrap();

        fs::set_permissions(&vault.dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(vault.get(SERVICE, &account), Err(SecretError::Failed));
        assert_eq!(
            temp.vault().unlock(PASSWORD, false),
            Err(UnlockError::Failed)
        );
        fs::set_permissions(&vault.dir, fs::Permissions::from_mode(0o700)).unwrap();

        // A symlinked item, and a symlinked vault directory, are never followed.
        let other = reference();
        std::os::unix::fs::symlink(&item, vault.dir.join(format!("{other}.item"))).unwrap();
        assert_eq!(vault.get(SERVICE, &other), Err(SecretError::Failed));
        let linked = temp.0.join("linked");
        std::os::unix::fs::symlink(&vault.dir, &linked).unwrap();
        let through_link = Vault::new(linked);
        assert_eq!(through_link.exists(), Err(SecretError::Failed));
        assert_eq!(
            through_link.unlock(PASSWORD, false),
            Err(UnlockError::Failed)
        );

        for bad in ["", "..", "../x", "a/b", "not-a-uuid"] {
            assert_eq!(vault.set(SERVICE, bad, b"v"), Err(SecretError::Failed));
        }
    }

    #[test]
    fn the_key_and_values_never_reach_the_disk() {
        let temp = Temp::new();
        let vault = created(&temp);
        let account = reference();
        vault
            .set(SERVICE, &account, b"synthetic-plaintext-value")
            .unwrap();
        let key = vault.key().unwrap();
        for entry in fs::read_dir(&vault.dir).unwrap() {
            let bytes = fs::read(entry.unwrap().path()).unwrap();
            assert!(!bytes.windows(32).any(|window| window == key.as_slice()));
            assert!(!bytes.windows(9).any(|window| window == b"synthetic"));
            assert!(
                !bytes
                    .windows(9)
                    .any(|window| window == &PASSWORD.as_bytes()[..9])
            );
        }
    }

    #[test]
    fn reset_deletes_only_the_vault_files() {
        let temp = Temp::new();
        let vault = created(&temp);
        vault.set(SERVICE, &reference(), b"value").unwrap();
        let sibling = vault.dir.parent().unwrap().join("outcome-journal");
        fs::create_dir(&sibling).unwrap();
        fs::write(vault.dir.join("unrelated.txt"), b"keep").unwrap();
        vault.reset().unwrap();
        assert!(!vault.unlocked());
        assert!(sibling.exists());
        let names: Vec<_> = fs::read_dir(&vault.dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, ["unrelated.txt"]);
        assert!(!vault.exists().unwrap());
        fs::remove_file(vault.dir.join("unrelated.txt")).unwrap();
        vault.unlock(PASSWORD, true).unwrap();
        vault.reset().unwrap();
        assert!(!vault.dir.exists());
    }

    /// Prints the Argon2id cost on this machine (`--nocapture`); asserts only that it runs.
    #[test]
    #[ignore = "timing probe: cargo test -p stoneql-desktop --lib vault::tests::argon2_cost -- --ignored --nocapture"]
    fn argon2_cost() {
        let started = std::time::Instant::now();
        derive(PASSWORD, &[7u8; 16], MEMORY_KIB, ITERATIONS, LANES).unwrap();
        println!(
            "argon2id m={MEMORY_KIB}KiB t={ITERATIONS} p={LANES}: {:?}",
            started.elapsed()
        );
    }
}
