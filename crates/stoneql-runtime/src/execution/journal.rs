//! Durable journal of unresolved `unknown` outcomes (safety contract 4.5).
//!
//! One snapshot file in a private per-user directory named by the host. It holds every
//! `unknown` outcome the user has not acknowledged and every read-write operation in flight
//! (journaled before anything is sent). Every change rewrites the whole snapshot: a new
//! temporary file (created 0600) → `sync_all` → rename over the journal → `sync_all` on the
//! directory, so a crash leaves the previous or the new snapshot, never a mix. Nothing in it is
//! SQL, a parameter, a value, a driver message, a host, a user name or a credential.
//!
//! Format, version 1 (UTF-8 lines, each ending in `\n`):
//!
//! ```text
//! {"format":"stoneql-unknown-outcomes","records":N,"version":1}
//! <crc32 of the record JSON, 8 hex> <record JSON>          (N lines)
//! end <crc32 of every preceding byte, 8 hex>
//! ```
//!
//! A file that is not exactly this (truncated, bit flips, garbage) is never deleted or
//! overwritten: it is preserved under a second name (hard link), the records whose checksums
//! verify are restored, and the damage is reported as an entry the user must acknowledge. The
//! damaged file leaves the journal path only when a durable snapshot carrying that report
//! replaces it, so a crash in between is detected again at the next start.
//!
//! The directory is opened `O_NOFOLLOW | O_DIRECTORY` and every file operation is relative to
//! that descriptor (`nix`, no `unsafe`). Files are opened `O_NOFOLLOW | O_NONBLOCK |
//! O_NOCTTY | O_CLOEXEC` and validated by `fstat` (a private regular file of the effective
//! uid), so a symlink, FIFO, socket or device is refused without being followed or blocking.
//!
//! Journal changes are applied in memory only after they are durable, except the removal of
//! work whose outcome is now known (`conclude`), which completes with the next successful
//! write.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use stoneql_contracts::v1::{
    EngineDto, JournalDamage, JournalProblemDto, OutcomeDto, TransactionEnd, UnresolvedJob,
};
use uuid::Uuid;

pub(crate) const FILE: &str = "unknown-outcomes.journal";
const TEMP: &str = "unknown-outcomes.journal.tmp";
const LOCK: &str = "unknown-outcomes.lock";
const FORMAT: &str = "stoneql-unknown-outcomes";
const VERSION: u64 = 1;
/// Far above any legitimate journal (bounded by the unresolved cap); larger is damage.
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DATABASE_CHARS: usize = 128;

/// One journaled outcome. Exactly one of `transaction`, `job` and `damage` is present; a
/// transaction is always `unknown`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Entry {
    pub(super) id: String,
    /// Unix seconds.
    pub(super) recorded_at: u64,
    pub(super) engine: Option<EngineDto>,
    pub(super) database: String,
    /// Journaled before the work was sent and not concluded since. Read at startup, it means
    /// the app exited (quit or crash) while the work was in flight.
    pub(super) in_flight: bool,
    pub(super) transaction: Option<TransactionEnd>,
    pub(super) job: Option<UnresolvedJob>,
    pub(super) damage: Option<JournalDamage>,
}
impl Entry {
    fn valid(&self) -> bool {
        Uuid::parse_str(&self.id).is_ok()
            && self.database.chars().count() <= MAX_DATABASE_CHARS
            && usize::from(self.transaction.is_some())
                + usize::from(self.job.is_some())
                + usize::from(self.damage.is_some())
                == 1
            && self
                .transaction
                .as_ref()
                .is_none_or(|end| end.outcome == OutcomeDto::Unknown)
    }
}

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// CRC-32 (IEEE 802.3), bitwise: the journal is small, and no dependency is needed.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn encode(entries: &[Entry]) -> Vec<u8> {
    let header =
        serde_json::json!({"format": FORMAT, "version": VERSION, "records": entries.len()});
    let mut out = serde_json::to_vec(&header).expect("journal header");
    out.push(b'\n');
    for entry in entries {
        let json = serde_json::to_vec(entry).expect("journal entry");
        out.extend_from_slice(format!("{:08x} ", crc32(&json)).as_bytes());
        out.extend_from_slice(&json);
        out.push(b'\n');
    }
    let sum = crc32(&out);
    out.extend_from_slice(format!("end {sum:08x}\n").as_bytes());
    out
}

#[derive(Debug, PartialEq, Eq)]
enum Loaded {
    Intact(Vec<Entry>),
    /// Written by another format version: left untouched.
    Unsupported,
    /// Records whose checksums verify, and how many of the records the header announced could
    /// not be recovered (unknown when the header is unreadable).
    Damaged {
        entries: Vec<Entry>,
        unreadable: Option<u32>,
    },
}

fn decode(bytes: &[u8]) -> Loaded {
    let mut lines: Vec<&[u8]> = bytes.split(|&byte| byte == b'\n').collect();
    let terminated = lines.len() > 1 && lines.last().is_some_and(|line| line.is_empty());
    if terminated {
        lines.pop();
    }
    let header = lines
        .first()
        .and_then(|line| serde_json::from_slice::<serde_json::Value>(line).ok());
    let mut count = None;
    if let Some(header) = &header
        && header.get("format").and_then(|format| format.as_str()) == Some(FORMAT)
    {
        if header.get("version").and_then(|version| version.as_u64()) != Some(VERSION) {
            return Loaded::Unsupported;
        }
        count = header
            .get("records")
            .and_then(|records| records.as_u64())
            .and_then(|records| u32::try_from(records).ok());
    }
    let trailer = lines
        .last()
        .filter(|line| lines.len() > 1 && line.starts_with(b"end "));
    let trailer_valid = trailer.is_some_and(|trailer| {
        let prefix = &bytes[..bytes.len() - trailer.len() - 1];
        terminated && **trailer == *format!("end {:08x}", crc32(prefix)).as_bytes()
    });
    let records = &lines[1.min(lines.len())..lines.len() - usize::from(trailer.is_some())];
    let mut entries: Vec<Entry> = Vec::new();
    let mut unreadable = 0u32;
    for line in records {
        let entry = line
            .get(..9)
            .filter(|prefix| prefix[8] == b' ')
            .and_then(|prefix| std::str::from_utf8(&prefix[..8]).ok())
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .filter(|&sum| sum == crc32(&line[9..]))
            .and_then(|_| serde_json::from_slice::<Entry>(&line[9..]).ok())
            .filter(|entry| entry.valid() && entries.iter().all(|other| other.id != entry.id));
        match entry {
            Some(entry) => entries.push(entry),
            None => unreadable += 1,
        }
    }
    let found = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    if trailer_valid && unreadable == 0 && count == Some(found) {
        return Loaded::Intact(entries);
    }
    // Counted against the header: a torn trailing line is not a lost record.
    Loaded::Damaged {
        entries,
        unreadable: count.map(|count| count.saturating_sub(found)),
    }
}

/// The journal of one app process. Without a store (not opened, insecure, in use, newer
/// version, damaged and not preserved) nothing can be written and every write reports failure.
pub(super) struct Journal {
    store: Option<Store>,
    entries: Vec<Entry>,
    problem: Option<JournalProblemDto>,
}
impl Journal {
    /// A host that requires durability but has not opened its journal yet.
    pub(super) fn unopened() -> Self {
        Self {
            store: None,
            entries: Vec::new(),
            problem: Some(JournalProblemDto::NotOpened),
        }
    }
    /// Opens (creating if needed) the journal in `dir`, which must be a private directory of
    /// the current user. The returned journal's entries are the ones to restore. It is
    /// rewritten once at open, so an unwritable directory is reported at startup.
    pub(super) fn open(dir: &Path) -> Self {
        let (store, loaded) = match Store::open(dir) {
            Ok(opened) => opened,
            Err(problem) => {
                return Self {
                    store: None,
                    entries: Vec::new(),
                    problem: Some(problem),
                };
            }
        };
        let (entries, store, problem) = match loaded {
            Loaded::Intact(entries) => (entries, Some(store), None),
            Loaded::Unsupported => (
                Vec::new(),
                None,
                Some(JournalProblemDto::UnsupportedVersion),
            ),
            Loaded::Damaged {
                mut entries,
                unreadable,
            } => {
                // The damaged file stays at the journal path until `flush` below replaces it.
                let preserved = store.quarantine().ok();
                let problem = preserved
                    .is_none()
                    .then_some(JournalProblemDto::DamagedNotPreserved);
                entries.push(Entry {
                    id: Uuid::new_v4().to_string(),
                    recorded_at: now(),
                    engine: None,
                    database: String::new(),
                    in_flight: false,
                    transaction: None,
                    job: None,
                    damage: Some(JournalDamage {
                        unreadable_records: unreadable,
                        preserved_as: preserved.clone(),
                    }),
                });
                (entries, preserved.map(|_| store), problem)
            }
        };
        let mut journal = Self {
            store,
            entries,
            problem,
        };
        if journal.store.is_some() {
            journal.flush();
        }
        journal
    }
    pub(super) fn problem(&self) -> Option<JournalProblemDto> {
        self.problem
    }
    /// Whether new work may be admitted. After a failed write (for example a full disk that
    /// has since been freed) the snapshot is written again first, so a transient failure does
    /// not refuse work until restart.
    pub(super) fn ready(&mut self) -> bool {
        if self.problem == Some(JournalProblemDto::WriteFailed) {
            self.flush();
        }
        self.problem.is_none()
    }
    pub(super) fn entries(&self) -> &[Entry] {
        &self.entries
    }
    /// Writes the whole snapshot again (after a failed write, so the file matches memory).
    fn flush(&mut self) -> bool {
        let next = self.entries.clone();
        self.commit(next)
    }
    /// Makes `next` the journal: written durably first, and only then the in-memory state. On
    /// failure memory keeps the last durable state, so a failed change (an acknowledgement, a
    /// newer fact) never becomes durable later by riding on another write. If the
    /// failure came after the rename (directory fsync), the file may already hold `next`; the
    /// next successful write restores the in-memory state.
    fn commit(&mut self, next: Vec<Entry>) -> bool {
        let Some(store) = &self.store else {
            return false;
        };
        let written = store.write(&next).is_ok();
        if written {
            self.entries = next;
        }
        self.problem = (!written).then_some(JournalProblemDto::WriteFailed);
        written
    }
    /// Inserts or replaces the entry with its id; true only once it is durable. On failure
    /// nothing changes (the previous durable entry, if any, stays).
    pub(super) fn put(&mut self, entry: Entry) -> bool {
        let mut next = self.entries.clone();
        match next.iter_mut().find(|other| other.id == entry.id) {
            Some(existing) if *existing == entry && self.problem.is_none() => return true,
            Some(existing) => *existing = entry,
            None => next.push(entry),
        }
        self.commit(next)
    }
    /// Removes the entry of work whose outcome is now known (resolved, or nothing was sent).
    /// Applied in memory even if the write fails, so it completes with the next successful
    /// write instead of being written back as an in-flight or unknown record; until then a
    /// restart would still show it (errs toward showing). True only once durable.
    pub(super) fn conclude(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        if before == self.entries.len() && self.problem.is_none() {
            return true;
        }
        self.flush()
    }
    /// Removes the entry because the user acknowledged it; true only once its absence is
    /// durable. On failure it stays, in memory and on disk, so an acknowledgement reported as
    /// failed never becomes durable later.
    pub(super) fn remove(&mut self, id: &str) -> bool {
        let next: Vec<_> = self
            .entries
            .iter()
            .filter(|e| e.id != id)
            .cloned()
            .collect();
        if next.len() == self.entries.len() && self.problem.is_none() {
            return true;
        }
        self.commit(next)
    }
    #[cfg(test)]
    pub(super) fn inject(&mut self, fault: Option<Fault>) {
        if let Some(store) = &mut self.store {
            store.fault = fault;
        }
    }
}

/// Test-only storage faults.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) enum Fault {
    /// The write fails before anything changes (for example a full disk).
    Fail(io::ErrorKind),
    /// The process "crashes" after writing part of the new snapshot.
    Torn,
    /// The process "crashes" after the new snapshot is synced, before it replaces the journal.
    BeforeRename,
}

struct Store {
    /// The journal directory, opened `O_NOFOLLOW | O_DIRECTORY` and validated by descriptor.
    /// Every file operation is relative to it, so a path swapped after validation is never
    /// followed.
    #[cfg(unix)]
    dir: File,
    /// Held (advisory `flock`) for the journal's lifetime: one process per journal.
    _lock: File,
    /// Device and inode of the journal file that was read (a damaged one is preserved only if
    /// the link still names this inode).
    #[cfg(unix)]
    read: Option<(u64, u64)>,
    #[cfg(test)]
    fault: Option<Fault>,
}

#[cfg(unix)]
mod private {
    use super::*;
    use nix::{
        dir::Dir,
        errno::Errno,
        fcntl::{AT_FDCWD, AtFlags, OFlag, openat, renameat},
        sys::stat::{Mode, fstatat},
        unistd::{UnlinkatFlags, linkat, unlinkat},
    };
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    /// Never follows a final symlink, never blocks on a FIFO, never acquires a controlling
    /// terminal, never leaks into children.
    /// `O_NONBLOCK` stays set on regular files, where reads and writes ignore it.
    const SAFE: OFlag = OFlag::O_NOFOLLOW
        .union(OFlag::O_NONBLOCK)
        .union(OFlag::O_NOCTTY)
        .union(OFlag::O_CLOEXEC);
    const PRIVATE_FILE: Mode = Mode::S_IRUSR.union(Mode::S_IWUSR);

    fn io(errno: Errno) -> io::Error {
        io::Error::from_raw_os_error(errno as i32)
    }
    /// Owned by `uid`, the expected type, not a symlink, and no group or other bits. Called on
    /// the metadata of an open descriptor (`File::metadata` is `fstat`).
    pub(super) fn private(
        meta: &fs::Metadata,
        uid: u32,
        dir: bool,
    ) -> Result<(), JournalProblemDto> {
        let kind = if dir { meta.is_dir() } else { meta.is_file() };
        if !kind || meta.file_type().is_symlink() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
            return Err(JournalProblemDto::Insecure);
        }
        Ok(())
    }
    pub(super) fn own_uid() -> u32 {
        nix::unistd::geteuid().as_raw()
    }
    /// A refused open: something other than a private regular file is at `name` (symlink,
    /// FIFO, socket, device, directory) is insecure; anything else is unavailable.
    fn refused(dir: &File, name: &str, errno: Errno) -> JournalProblemDto {
        // `st_mode` is u16 on macOS and u32 on Linux: the cast is needed on one of them.
        #[allow(clippy::unnecessary_cast)]
        let other = matches!(
            errno,
            Errno::ELOOP | Errno::EISDIR | Errno::ENXIO | Errno::EOPNOTSUPP | Errno::ENOTDIR
        ) || fstatat(dir, name, AtFlags::AT_SYMLINK_NOFOLLOW)
            .is_ok_and(|stat| stat.st_mode as u32 & 0o170000 != 0o100000);
        if other {
            JournalProblemDto::Insecure
        } else {
            JournalProblemDto::Unavailable
        }
    }
    /// Opens `name` in the journal directory without following a symlink or blocking on a
    /// FIFO, and validates the opened descriptor: a private regular file of `uid`. `Ok(None)`:
    /// it does not exist (and `create` is false).
    pub(super) fn open_private(
        dir: &File,
        name: &str,
        uid: u32,
        create: bool,
    ) -> Result<Option<File>, JournalProblemDto> {
        let flags = if create {
            OFlag::O_RDWR | OFlag::O_CREAT | SAFE
        } else {
            OFlag::O_RDONLY | SAFE
        };
        let file = match openat(dir, name, flags, PRIVATE_FILE) {
            Ok(fd) => File::from(fd),
            Err(Errno::ENOENT) if !create => return Ok(None),
            Err(errno) => return Err(refused(dir, name, errno)),
        };
        let meta = file
            .metadata()
            .map_err(|_| JournalProblemDto::Unavailable)?;
        private(&meta, uid, false)?;
        Ok(Some(file))
    }
    /// Opens the journal directory itself without following a final symlink, validated by
    /// descriptor (a directory of `uid`, no group or other bits).
    fn open_dir(path: &Path, uid: u32) -> Result<File, JournalProblemDto> {
        let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | SAFE;
        let dir = match openat(AT_FDCWD, path, flags, Mode::empty()) {
            Ok(fd) => File::from(fd),
            Err(Errno::ELOOP | Errno::ENOTDIR) => return Err(JournalProblemDto::Insecure),
            Err(_) => return Err(JournalProblemDto::Unavailable),
        };
        let meta = dir.metadata().map_err(|_| JournalProblemDto::Unavailable)?;
        private(&meta, uid, true)?;
        Ok(dir)
    }

    impl Store {
        pub(super) fn open(dir: &Path) -> Result<(Self, Loaded), JournalProblemDto> {
            let unavailable = |_| JournalProblemDto::Unavailable;
            if let Some(parent) = dir.parent() {
                fs::create_dir_all(parent).map_err(unavailable)?;
            }
            match fs::DirBuilder::new().mode(0o700).create(dir) {
                // The new directory entry is durable before anything is written into it.
                Ok(()) => {
                    if let Some(parent) = dir.parent() {
                        File::open(parent)
                            .and_then(|parent| parent.sync_all())
                            .map_err(unavailable)?;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(JournalProblemDto::Unavailable),
            }
            let uid = own_uid();
            let directory = open_dir(dir, uid)?;
            let lock = open_private(&directory, LOCK, uid, true)?.expect("created");
            match lock.try_lock() {
                Ok(()) => {}
                Err(fs::TryLockError::WouldBlock) => return Err(JournalProblemDto::InUse),
                Err(_) => return Err(JournalProblemDto::Unavailable),
            }
            let (loaded, read) = match open_private(&directory, FILE, uid, false)? {
                None => (Loaded::Intact(Vec::new()), None),
                Some(file) => {
                    let meta = file.metadata().map_err(unavailable)?;
                    let mut bytes = Vec::new();
                    file.take(MAX_BYTES + 1)
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
                    (loaded, Some((meta.dev(), meta.ino())))
                }
            };
            Ok((
                Self {
                    dir: directory,
                    _lock: lock,
                    read,
                    #[cfg(test)]
                    fault: None,
                },
                loaded,
            ))
        }
        fn sync_dir(&self) -> io::Result<()> {
            self.dir.sync_all()
        }
        /// Temporary file (new, 0600, no-follow, exclusive) → fsync → rename over the journal
        /// → directory fsync, all relative to the validated directory descriptor.
        pub(super) fn write(&self, entries: &[Entry]) -> io::Result<()> {
            match unlinkat(&self.dir, TEMP, UnlinkatFlags::NoRemoveDir) {
                Ok(()) | Err(Errno::ENOENT) => {}
                Err(errno) => return Err(io(errno)),
            }
            let bytes = encode(entries);
            #[cfg(test)]
            if let Some(Fault::Fail(kind)) = self.fault {
                return Err(kind.into());
            }
            let written = (|| {
                let flags = OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | SAFE;
                let mut file =
                    File::from(openat(&self.dir, TEMP, flags, PRIVATE_FILE).map_err(io)?);
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
                renameat(&self.dir, TEMP, &self.dir, FILE).map_err(io)?;
                self.sync_dir()?;
                Ok(true)
            })();
            match written {
                Ok(true) => Ok(()),
                // A simulated crash: the temporary file is left as a crash would leave it.
                Ok(false) => Err(io::ErrorKind::Interrupted.into()),
                Err(error) => {
                    let _ = unlinkat(&self.dir, TEMP, UnlinkatFlags::NoRemoveDir);
                    Err(error)
                }
            }
        }
        /// Preserves a damaged journal under a second name: a hard link (never replacing an
        /// existing file, never following a symlink), made durable with a directory fsync. It
        /// must name the inode that was read; a file swapped in meanwhile is not preserved in
        /// its place. The damaged file stays at the journal path until a new snapshot
        /// atomically replaces it, so a crash or a failed rewrite in between leaves it there to
        /// be detected again at the next start. A copy already linked by an earlier start (same
        /// inode) is reused.
        pub(super) fn quarantine(&self) -> io::Result<String> {
            let read = self.read.ok_or(io::ErrorKind::NotFound)?;
            // `st_dev` is i32 on macOS and u64 on Linux: the cast is needed on one of them.
            #[allow(clippy::unnecessary_cast)]
            let inode = |name: &str| {
                fstatat(&self.dir, name, AtFlags::AT_SYMLINK_NOFOLLOW)
                    .map(|stat| (stat.st_dev as u64, stat.st_ino))
            };
            if inode(FILE).map_err(io)? != read {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let prefix = format!("{FILE}.damaged-");
            // Listed through a fresh descriptor of the validated directory (never by path).
            let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | SAFE;
            let names: Vec<String> = Dir::openat(&self.dir, ".", flags, Mode::empty())
                .map_err(io)?
                .into_iter()
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            if let Some(name) = names
                .into_iter()
                .find(|name| name.starts_with(&prefix) && inode(name).is_ok_and(|i| i == read))
            {
                return Ok(name);
            }
            for _ in 0..8 {
                let name = format!(
                    "{prefix}{}-{}",
                    now(),
                    &Uuid::new_v4().simple().to_string()[..8]
                );
                match linkat(&self.dir, FILE, &self.dir, name.as_str(), AtFlags::empty()) {
                    Ok(()) => {
                        // The journal path could have been swapped between the check and the
                        // link: keep the copy only if it is the inode that was read.
                        if inode(&name).map_err(io)? != read {
                            let _ = unlinkat(&self.dir, name.as_str(), UnlinkatFlags::NoRemoveDir);
                            return Err(io::ErrorKind::InvalidData.into());
                        }
                        self.sync_dir()?;
                        return Ok(name);
                    }
                    Err(Errno::EEXIST) => continue,
                    Err(errno) => return Err(io(errno)),
                }
            }
            Err(io::ErrorKind::AlreadyExists.into())
        }
    }
}

#[cfg(not(unix))]
impl Store {
    fn open(_dir: &Path) -> Result<(Self, Loaded), JournalProblemDto> {
        Err(JournalProblemDto::Unavailable)
    }
    fn write(&self, _entries: &[Entry]) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn quarantine(&self) -> io::Result<String> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(all(test, unix))]
mod tests;
