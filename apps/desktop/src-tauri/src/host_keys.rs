//! Native SSH host-key trust store (M005 F-01). Only native code reads or writes trust: the
//! renderer can ask for status, request enrollment of a key a native scan presented (after a
//! native confirmation) and forget an entry. Host keys are public, so the store needs integrity
//! rather than confidentiality; same-user tampering cannot unlock saved SSH passwords because
//! their scope is bound to the enrolled fingerprint (`secrets::ssh_password_scope`).
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use stoneql_runtime::{HostKey, describe_host_key};

/// Limits (Proposed, M005 alpha fix plan T4): entries, bytes per key and bytes per file.
pub const MAX_HOSTS: usize = 512;
pub const MAX_KEY_BYTES: usize = 8 * 1024;
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
pub const STORE_FILE: &str = "ssh-host-keys.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyStoreError {
    /// Store location unresolved or I/O failed; callers fail closed.
    Unavailable,
    /// Key unparsable, too large or the entry cap is reached.
    Invalid,
}

/// Trust entries keyed by `(lowercase host, port)`; one key per endpoint, shared by every
/// profile that uses it (as in `known_hosts`).
pub trait HostKeyStore: Send + Sync {
    fn get(&self, host: &str, port: u16) -> Result<Option<String>, HostKeyStoreError>;
    fn trust(&self, host: &str, port: u16, key: &str) -> Result<(), HostKeyStoreError>;
    fn forget(&self, host: &str, port: u16) -> Result<(), HostKeyStoreError>;
}

fn endpoint(host: &str, port: u16) -> (String, u16) {
    (host.to_ascii_lowercase(), port)
}
/// The fingerprint and algorithm are always derived from the key itself.
pub fn validated(key: &str) -> Result<HostKey, HostKeyStoreError> {
    if key.len() > MAX_KEY_BYTES {
        return Err(HostKeyStoreError::Invalid);
    }
    describe_host_key(key).map_err(|_| HostKeyStoreError::Invalid)
}

/// Test and fallback store kept in memory.
#[derive(Default)]
pub struct MemoryHostKeyStore(Mutex<HashMap<(String, u16), String>>);
impl HostKeyStore for MemoryHostKeyStore {
    fn get(&self, host: &str, port: u16) -> Result<Option<String>, HostKeyStoreError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| HostKeyStoreError::Unavailable)?
            .get(&endpoint(host, port))
            .cloned())
    }
    fn trust(&self, host: &str, port: u16, key: &str) -> Result<(), HostKeyStoreError> {
        let normalized = validated(key)?.public_key;
        let mut hosts = self.0.lock().map_err(|_| HostKeyStoreError::Unavailable)?;
        if hosts.len() >= MAX_HOSTS && !hosts.contains_key(&endpoint(host, port)) {
            return Err(HostKeyStoreError::Invalid);
        }
        hosts.insert(endpoint(host, port), normalized);
        Ok(())
    }
    fn forget(&self, host: &str, port: u16) -> Result<(), HostKeyStoreError> {
        self.0
            .lock()
            .map_err(|_| HostKeyStoreError::Unavailable)?
            .remove(&endpoint(host, port));
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileFormat {
    v: u32,
    hosts: Vec<FileEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEntry {
    host: String,
    port: u16,
    key: String,
}

/// Production store: `<app data dir>/ssh-host-keys.json`, directory 0700, file 0600, atomic
/// replace (temporary file, fsync, rename), writes serialized. A corrupt, oversized or unknown
/// version file is quarantined as `ssh-host-keys.json.corrupt-<unix seconds>` and the store
/// starts empty, so every host becomes unknown (fail closed); nothing is guessed or repaired.
pub struct FileHostKeyStore {
    path: PathBuf,
    lock: Mutex<()>,
}
impl FileHostKeyStore {
    pub fn new(directory: &Path) -> Self {
        Self {
            path: directory.join(STORE_FILE),
            lock: Mutex::new(()),
        }
    }
    fn load(&self) -> Result<HashMap<(String, u16), String>, HostKeyStoreError> {
        let metadata = match std::fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(_) => return Err(HostKeyStoreError::Unavailable),
        };
        let parsed = (metadata.is_file() && metadata.len() <= MAX_FILE_BYTES)
            .then(|| std::fs::read(&self.path).ok())
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<FileFormat>(&bytes).ok())
            .filter(|file| file.v == 1 && file.hosts.len() <= MAX_HOSTS)
            .and_then(|file| {
                let mut hosts = HashMap::new();
                for entry in file.hosts {
                    let key = validated(&entry.key).ok()?.public_key;
                    if hosts
                        .insert(endpoint(&entry.host, entry.port), key)
                        .is_some()
                    {
                        return None; // duplicate endpoint: ambiguous trust
                    }
                }
                Some(hosts)
            });
        match parsed {
            Some(hosts) => Ok(hosts),
            None => {
                self.quarantine()?;
                Ok(HashMap::new())
            }
        }
    }
    fn quarantine(&self) -> Result<(), HostKeyStoreError> {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or_default();
        let target = self
            .path
            .with_file_name(format!("{STORE_FILE}.corrupt-{seconds}"));
        std::fs::rename(&self.path, target).map_err(|_| HostKeyStoreError::Unavailable)
    }
    fn save(&self, hosts: &HashMap<(String, u16), String>) -> Result<(), HostKeyStoreError> {
        let directory = self.path.parent().ok_or(HostKeyStoreError::Unavailable)?;
        create_private_directory(directory)?;
        let mut entries: Vec<FileEntry> = hosts
            .iter()
            .map(|((host, port), key)| FileEntry {
                host: host.clone(),
                port: *port,
                key: key.clone(),
            })
            .collect();
        entries.sort_by(|a, b| (&a.host, a.port).cmp(&(&b.host, b.port)));
        let bytes = serde_json::to_vec(&FileFormat {
            v: 1,
            hosts: entries,
        })
        .map_err(|_| HostKeyStoreError::Unavailable)?;
        let temporary = directory.join(format!(".{STORE_FILE}.{}.tmp", uuid::Uuid::new_v4()));
        let written = (|| {
            let mut file = private_file(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.path)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&temporary);
            return Err(HostKeyStoreError::Unavailable);
        }
        Ok(())
    }
}
#[cfg(unix)]
fn create_private_directory(directory: &Path) -> Result<(), HostKeyStoreError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(|_| HostKeyStoreError::Unavailable)?;
    // An existing directory keeps its mode; only tighten our own store directory.
    let mode = std::fs::metadata(directory)
        .map_err(|_| HostKeyStoreError::Unavailable)?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| HostKeyStoreError::Unavailable)?;
    }
    Ok(())
}
#[cfg(not(unix))]
fn create_private_directory(directory: &Path) -> Result<(), HostKeyStoreError> {
    std::fs::create_dir_all(directory).map_err(|_| HostKeyStoreError::Unavailable)
}
#[cfg(unix)]
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}
#[cfg(not(unix))]
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}
impl HostKeyStore for FileHostKeyStore {
    fn get(&self, host: &str, port: u16) -> Result<Option<String>, HostKeyStoreError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| HostKeyStoreError::Unavailable)?;
        Ok(self.load()?.remove(&endpoint(host, port)))
    }
    fn trust(&self, host: &str, port: u16, key: &str) -> Result<(), HostKeyStoreError> {
        let normalized = validated(key)?.public_key;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| HostKeyStoreError::Unavailable)?;
        let mut hosts = self.load()?;
        if hosts.len() >= MAX_HOSTS && !hosts.contains_key(&endpoint(host, port)) {
            return Err(HostKeyStoreError::Invalid);
        }
        hosts.insert(endpoint(host, port), normalized);
        self.save(&hosts)
    }
    fn forget(&self, host: &str, port: u16) -> Result<(), HostKeyStoreError> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| HostKeyStoreError::Unavailable)?;
        let mut hosts = self.load()?;
        if hosts.remove(&endpoint(host, port)).is_some() {
            self.save(&hosts)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ED25519: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
    fn temporary() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stoneql-host-keys-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn round_trips_case_insensitively_with_private_modes_and_no_temporary_files() {
        let dir = temporary();
        let store = FileHostKeyStore::new(&dir.join("app"));
        assert_eq!(store.get("Bastion.Example", 22).unwrap(), None);
        store
            .trust("Bastion.Example", 22, &format!("{ED25519} comment@host"))
            .unwrap();
        assert_eq!(
            store.get("bastion.example", 22).unwrap().as_deref(),
            Some(ED25519)
        );
        assert_eq!(store.get("bastion.example", 2222).unwrap(), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&dir.join("app")), 0o700);
            assert_eq!(mode(&dir.join("app").join(STORE_FILE)), 0o600);
        }
        let names: Vec<_> = std::fs::read_dir(dir.join("app"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [STORE_FILE]);
        store.forget("BASTION.example", 22).unwrap();
        assert_eq!(store.get("bastion.example", 22).unwrap(), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn corrupt_oversized_unknown_version_and_forged_files_are_quarantined_and_fail_closed() {
        let entry = format!(r#"{{"host":"h","port":22,"key":"{ED25519}"}}"#);
        for contents in [
            "not json".to_owned(),
            format!(r#"{{"v":2,"hosts":[{entry}]}}"#),
            r#"{"v":1,"hosts":[{"host":"h","port":22,"key":"ssh-ed25519 not-base64"}]}"#.to_owned(),
            // A stored fingerprint field is never accepted: strict schema.
            format!(
                r#"{{"v":1,"hosts":[{{"host":"h","port":22,"key":"{ED25519}","fingerprint":"SHA256:forged"}}]}}"#
            ),
            format!(r#"{{"v":1,"hosts":[{entry},{entry}]}}"#),
            "x".repeat(MAX_FILE_BYTES as usize + 1),
        ] {
            let dir = temporary();
            std::fs::write(dir.join(STORE_FILE), &contents).unwrap();
            let store = FileHostKeyStore::new(&dir);
            assert_eq!(store.get("h", 22).unwrap(), None);
            let names: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect();
            assert_eq!(names.len(), 1);
            assert!(
                names[0].starts_with(&format!("{STORE_FILE}.corrupt-")),
                "{names:?}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn invalid_and_oversized_keys_and_the_entry_cap_are_refused() {
        let store = MemoryHostKeyStore::default();
        assert_eq!(
            store.trust("h", 22, "ssh-ed25519 not-base64"),
            Err(HostKeyStoreError::Invalid)
        );
        assert_eq!(
            store.trust("h", 22, &format!("{ED25519} {}", "c".repeat(MAX_KEY_BYTES))),
            Err(HostKeyStoreError::Invalid)
        );
        for port in 0..MAX_HOSTS as u16 {
            store.trust("h", port + 1, ED25519).unwrap();
        }
        assert_eq!(
            store.trust("other", 22, ED25519),
            Err(HostKeyStoreError::Invalid)
        );
        store.trust("h", 1, ED25519).unwrap(); // replacing an existing endpoint is allowed
    }
    #[test]
    #[cfg(unix)]
    fn a_failed_write_leaves_no_temporary_file_and_keeps_the_old_store() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temporary();
        let store = FileHostKeyStore::new(&dir);
        store.trust("h", 22, ED25519).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        assert_eq!(
            store.trust("other", 22, ED25519),
            Err(HostKeyStoreError::Unavailable)
        );
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [STORE_FILE]);
        assert_eq!(store.get("h", 22).unwrap().as_deref(), Some(ED25519));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
