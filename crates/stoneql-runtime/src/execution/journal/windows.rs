//! Windows store, in safe `std` only.
//!
//! Windows has no `openat`, so files are named by path inside the journal directory. Instead,
//! the directory is held open for the journal's lifetime without `FILE_SHARE_DELETE`: while that
//! handle is open Windows refuses to rename or delete the directory (`windows_tests.rs` checks
//! this, and what it means for the parent), so the paths keep naming the validated directory.
//! Every open uses `FILE_FLAG_OPEN_REPARSE_POINT` and is refused unless the opened object is a
//! plain directory or regular file, so a symlink, junction or mount point is never followed.
//!
//! Weaker than the Unix store (see `.ref/planning/windows-support-plan.md`, phase 3):
//! - Ownership and ACLs are not checked: that needs Win32 security calls outside safe `std`.
//!   The directory is in the user profile (`%APPDATA%`), whose default ACL admits only the user,
//!   SYSTEM and administrators.
//! - There is no directory fsync. The new snapshot is flushed before the rename and the journal
//!   file is flushed again after it, but nothing StoneQL calls guarantees that the rename itself
//!   survives a power loss.
//! - A damaged journal is preserved under a new name on every start until a snapshot replaces
//!   it; an earlier copy is not reused (file identity is not available in stable `std`).
use super::*;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::Mutex;

const FILE_SHARE_READ: u32 = 0x1;
const FILE_SHARE_WRITE: u32 = 0x2;
/// Required to open a directory handle.
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// Opens a symlink, junction or mount point itself instead of its target.
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_SHARING_VIOLATION: i32 = 32;

/// Shares read and write but never delete, so the opened object cannot be renamed, replaced or
/// deleted while the handle is open.
const NO_DELETE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;

fn checked(file: File, dir: bool) -> Result<File, JournalProblemDto> {
    let meta = file
        .metadata()
        .map_err(|_| JournalProblemDto::Unavailable)?;
    let kind = if dir { meta.is_dir() } else { meta.is_file() };
    if !kind || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(JournalProblemDto::Insecure);
    }
    Ok(file)
}

/// A refused open: a reparse point or an object of the other kind at `path` is insecure;
/// anything else is unavailable.
fn refused(path: &Path, dir: bool) -> JournalProblemDto {
    match fs::symlink_metadata(path) {
        Ok(meta)
            if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                || meta.is_dir() != dir =>
        {
            JournalProblemDto::Insecure
        }
        _ => JournalProblemDto::Unavailable,
    }
}

fn open_dir(path: &Path) -> Result<File, JournalProblemDto> {
    let opened = fs::OpenOptions::new()
        .read(true)
        .share_mode(NO_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path);
    match opened {
        Ok(dir) => checked(dir, true),
        Err(_) => Err(refused(path, true)),
    }
}

/// Opens a regular file without following a reparse point. `Ok(None)`: it does not exist (and
/// `create` is false).
fn open_file(path: &Path, create: bool) -> Result<Option<File>, JournalProblemDto> {
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .share_mode(NO_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    if create {
        options.write(true).create(true);
    }
    match options.open(path) {
        Ok(file) => checked(file, false).map(Some),
        Err(error) if !create && error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(refused(path, false)),
    }
}

/// `fs::rename` replaces the journal atomically, but fails while another process (an antivirus
/// scanner or the search indexer) briefly holds the file without delete sharing; retry briefly.
fn rename(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt = 0;
    loop {
        match fs::rename(from, to) {
            Err(error)
                if attempt < 10
                    && matches!(
                        error.raw_os_error(),
                        Some(ERROR_SHARING_VIOLATION | ERROR_ACCESS_DENIED)
                    ) =>
            {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(10 * attempt));
            }
            result => return result,
        }
    }
}

pub(super) struct Platform {
    /// Held without delete sharing for the journal's lifetime (see the module comment).
    _dir: File,
    path: PathBuf,
    /// The journal file that was read, held without delete sharing so that a damaged journal
    /// is preserved by `quarantine` as the file that was read. Released before any write, which
    /// must replace it.
    read: Mutex<Option<File>>,
}

impl Store {
    pub(super) fn open(dir: &Path) -> Result<(Self, Loaded), JournalProblemDto> {
        let unavailable = |_| JournalProblemDto::Unavailable;
        fs::create_dir_all(dir).map_err(unavailable)?;
        let directory = open_dir(dir)?;
        let lock = open_file(&dir.join(LOCK), true)?.expect("created");
        match lock.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => return Err(JournalProblemDto::InUse),
            Err(_) => return Err(JournalProblemDto::Unavailable),
        }
        let (loaded, read) = match open_file(&dir.join(FILE), false)? {
            None => (Loaded::Intact(Vec::new()), None),
            Some(file) => {
                let mut bytes = Vec::new();
                (&file)
                    .take(MAX_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(unavailable)?;
                let loaded = if bytes.len() as u64 > MAX_BYTES {
                    Loaded::Damaged {
                        entries: Vec::new(),
                        unreadable: None,
                    }
                } else {
                    decode(&bytes)
                };
                let keep = matches!(loaded, Loaded::Damaged { .. });
                (loaded, keep.then_some(file))
            }
        };
        Ok((
            Self {
                _lock: lock,
                platform: Platform {
                    _dir: directory,
                    path: dir.to_path_buf(),
                    read: Mutex::new(read),
                },
                #[cfg(test)]
                fault: None,
            },
            loaded,
        ))
    }
    fn held(&self) -> std::sync::MutexGuard<'_, Option<File>> {
        self.platform
            .read
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    /// Temporary file (new, exclusive, no-follow) → flush → rename over the journal → flush the
    /// journal file.
    pub(super) fn write(&self, entries: &[Entry]) -> io::Result<()> {
        self.held().take();
        let (temp, journal) = (self.platform.path.join(TEMP), self.platform.path.join(FILE));
        match fs::remove_file(&temp) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let bytes = encode(entries);
        #[cfg(test)]
        if let Some(Fault::Fail(kind)) = self.fault {
            return Err(kind.into());
        }
        let written = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&temp)?;
            // Test-only crashes inside the real sequence: no cleanup runs after a crash.
            #[cfg(test)]
            if let Some(Fault::Torn) = self.fault {
                file.write_all(&bytes[..bytes.len() / 2])?;
                return Ok(false);
            }
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            #[cfg(test)]
            if let Some(Fault::BeforeRename) = self.fault {
                return Ok(false);
            }
            rename(&temp, &journal)?;
            let renamed = fs::OpenOptions::new()
                .write(true)
                .share_mode(NO_DELETE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&journal)?;
            checked(renamed, false)
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?
                .sync_all()?;
            Ok(true)
        })();
        match written {
            Ok(true) => Ok(()),
            // A simulated crash: the temporary file is left as a crash would leave it.
            Ok(false) => Err(io::ErrorKind::Interrupted.into()),
            Err(error) => {
                let _ = fs::remove_file(&temp);
                Err(error)
            }
        }
    }
    /// Preserves a damaged journal under a second name: a hard link that never replaces an
    /// existing file. The journal file that was read is still held without delete sharing, so
    /// the journal path names it; the damaged file stays there until a new snapshot replaces
    /// it.
    pub(super) fn quarantine(&self) -> io::Result<String> {
        let mut held = self.held();
        if held.is_none() {
            return Err(io::ErrorKind::NotFound.into());
        }
        let prefix = format!("{FILE}.damaged-");
        for _ in 0..8 {
            let name = format!(
                "{prefix}{}-{}",
                now(),
                &Uuid::new_v4().simple().to_string()[..8]
            );
            match fs::hard_link(
                self.platform.path.join(FILE),
                self.platform.path.join(&name),
            ) {
                Ok(()) => {
                    held.take();
                    return Ok(name);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::ErrorKind::AlreadyExists.into())
    }
}
