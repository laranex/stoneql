//! Journal storage tests on private temporary directories (never inside the repository).
use super::*;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt, symlink};
use std::path::PathBuf;
use stoneql_contracts::v1::{CancellationDto, TransactionEndReasonDto};

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("stoneql-journal-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(fs::canonicalize(root).unwrap().join("journal"))
    }
    fn file(&self) -> PathBuf {
        self.0.join(FILE)
    }
    fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
    fn chmod(&self, mode: u32) {
        fs::set_permissions(&self.0, fs::Permissions::from_mode(mode)).unwrap();
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
        let _ = fs::remove_dir_all(self.0.parent().unwrap());
    }
}
fn root() -> bool {
    private::own_uid() == 0
}

fn lease(commit_sent: bool) -> Entry {
    Entry {
        id: Uuid::new_v4().to_string(),
        recorded_at: 1_700_000_000,
        engine: Some(EngineDto::MySql),
        database: "app".into(),
        in_flight: false,
        transaction: Some(TransactionEnd {
            outcome: OutcomeDto::Unknown,
            reason: TransactionEndReasonDto::Requested,
            commit_sent,
            rollback_sent: false,
        }),
        job: None,
        damage: None,
    }
}
fn job() -> Entry {
    Entry {
        id: Uuid::new_v4().to_string(),
        recorded_at: 1_700_000_001,
        engine: Some(EngineDto::PostgreSql),
        database: "warehouse".into(),
        in_flight: true,
        transaction: None,
        job: Some(UnresolvedJob {
            statement_count: 3,
            unknown_statement: Some(1),
            in_transaction: true,
            cancellation: CancellationDto::None,
            server_cancel_sent: false,
            deadline_exceeded: false,
        }),
        damage: None,
    }
}
fn written(dir: &Dir, entries: &[Entry]) -> Vec<u8> {
    let mut journal = Journal::open(&dir.0);
    assert_eq!(journal.problem(), None);
    for entry in entries {
        assert!(journal.put(entry.clone()));
    }
    drop(journal);
    fs::read(dir.file()).unwrap()
}
fn damage(journal: &Journal) -> Option<JournalDamage> {
    journal
        .entries()
        .iter()
        .find_map(|entry| entry.damage.clone())
}

#[test]
fn crc32_matches_the_ieee_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn a_journal_round_trips_and_is_created_private() {
    let dir = Dir::new();
    let entries = [lease(true), job()];
    let bytes = written(&dir, &entries);
    assert_eq!(decode(&bytes), Loaded::Intact(entries.to_vec()));
    assert_eq!(fs::metadata(&dir.0).unwrap().mode() & 0o777, 0o700);
    assert_eq!(fs::metadata(dir.file()).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(dir.0.join(LOCK)).unwrap().mode() & 0o777,
        0o600
    );
    let reopened = Journal::open(&dir.0);
    assert_eq!(
        (reopened.problem(), reopened.entries()),
        (None, &entries[..])
    );
    assert_eq!(dir.names(), [FILE, LOCK]);
}

#[test]
fn removal_is_durable_and_put_replaces_by_id() {
    let dir = Dir::new();
    let (first, second) = (lease(false), job());
    let mut journal = Journal::open(&dir.0);
    assert!(journal.put(first.clone()) && journal.put(second.clone()));
    let sent = Entry {
        transaction: Some(TransactionEnd {
            commit_sent: true,
            ..first.transaction.clone().unwrap()
        }),
        ..first.clone()
    };
    assert!(journal.put(sent.clone()));
    assert!(journal.remove(&second.id));
    assert!(
        journal.remove(&second.id),
        "removing an absent entry is idempotent"
    );
    drop(journal);
    assert_eq!(Journal::open(&dir.0).entries(), [sent]);
}

/// A crash at any byte of a record (torn file) is never accepted as an intact journal and never
/// yields a record that was not written; only fully verified records are restored.
#[test]
fn truncation_at_every_offset_is_detected_and_never_invents_records() {
    let dir = Dir::new();
    let entries = [lease(true), job()];
    let bytes = written(&dir, &entries);
    for end in 0..bytes.len() {
        match decode(&bytes[..end]) {
            Loaded::Damaged {
                entries: salvaged,
                unreadable,
            } => {
                assert!(salvaged.len() <= 2 && salvaged.iter().zip(&entries).all(|(a, b)| a == b));
                if let Some(unreadable) = unreadable {
                    assert_eq!(unreadable as usize, entries.len() - salvaged.len());
                }
            }
            other => panic!("truncated at {end}: {other:?}"),
        }
    }
}

#[test]
fn single_bit_flips_are_never_accepted() {
    let dir = Dir::new();
    let entries = [lease(true), job()];
    let bytes = written(&dir, &entries);
    for index in 0..bytes.len() {
        for bit in 0..8 {
            let mut flipped = bytes.clone();
            flipped[index] ^= 1 << bit;
            match decode(&flipped) {
                Loaded::Intact(_) => panic!("bit {bit} of byte {index} accepted"),
                // Only the header's version digit can read as another version (left untouched).
                Loaded::Unsupported => {
                    assert!(index < bytes.iter().position(|&b| b == b'\n').unwrap())
                }
                Loaded::Damaged {
                    entries: salvaged, ..
                } => {
                    assert!(salvaged.iter().all(|entry| entries.contains(entry)))
                }
            }
        }
    }
}

/// A damaged journal is renamed aside byte for byte (never deleted or overwritten), the
/// verified records are restored, and the damage is reported as an entry to acknowledge.
#[test]
fn a_damaged_journal_is_preserved_reported_and_its_verified_records_restored() {
    for case in ["truncated", "flipped", "garbage", "empty", "oversized"] {
        let dir = Dir::new();
        let entries = [lease(true), job()];
        let mut bytes = written(&dir, &entries);
        match case {
            "truncated" => bytes.truncate(bytes.len() - 5),
            "flipped" => {
                let second = bytes.iter().rposition(|&b| b == b'{').unwrap();
                bytes[second + 3] ^= 0x01;
            }
            "garbage" => bytes = b"\x00\xffnot a journal\n".to_vec(),
            "empty" => bytes.clear(),
            _ => bytes = vec![b'x'; MAX_BYTES as usize + 1],
        }
        fs::write(dir.file(), &bytes).unwrap();
        let journal = Journal::open(&dir.0);
        assert_eq!(
            journal.problem(),
            None,
            "{case}: a fresh journal is writable"
        );
        let report = damage(&journal).unwrap_or_else(|| panic!("{case}: no damage entry"));
        let preserved = report.preserved_as.clone().expect("renamed aside");
        assert_eq!(fs::read(dir.0.join(&preserved)).unwrap(), bytes, "{case}");
        let restored: Vec<_> = journal
            .entries()
            .iter()
            .filter(|e| e.damage.is_none())
            .collect();
        match case {
            "truncated" => {
                assert_eq!(restored, [&entries[0], &entries[1]]);
                assert_eq!(report.unreadable_records, Some(0));
            }
            "flipped" => {
                assert_eq!(restored, [&entries[0]]);
                assert_eq!(report.unreadable_records, Some(1));
            }
            _ => {
                assert!(restored.is_empty());
                assert_eq!(report.unreadable_records, None);
            }
        }
        drop(journal);
        // The report survives a restart until acknowledged; the copy is never touched again.
        let again = Journal::open(&dir.0);
        assert_eq!(damage(&again), Some(report), "{case}");
        assert_eq!(fs::read(dir.0.join(&preserved)).unwrap(), bytes);
    }
}

#[test]
fn another_version_is_left_untouched_and_refused() {
    let dir = Dir::new();
    written(&dir, &[lease(false)]);
    let newer =
        b"{\"format\":\"stoneql-unknown-outcomes\",\"records\":0,\"version\":2}\nend 00000000\n";
    fs::write(dir.file(), newer).unwrap();
    let journal = Journal::open(&dir.0);
    assert_eq!(
        journal.problem(),
        Some(JournalProblemDto::UnsupportedVersion)
    );
    let mut journal = journal;
    assert!(!journal.put(lease(true)));
    assert_eq!(
        journal.problem(),
        Some(JournalProblemDto::UnsupportedVersion)
    );
    assert_eq!(fs::read(dir.file()).unwrap(), newer);
    assert_eq!(dir.names(), [FILE, LOCK]);
}

/// A crash part-way through a write (torn new snapshot, or synced but not yet renamed) leaves
/// the previous snapshot intact; the leftover is ignored and replaced.
#[test]
fn a_crash_while_writing_leaves_the_previous_snapshot() {
    for fault in [Fault::Torn, Fault::BeforeRename] {
        let dir = Dir::new();
        let first = lease(false);
        let mut journal = Journal::open(&dir.0);
        assert!(journal.put(first.clone()));
        let before = fs::read(dir.file()).unwrap();
        journal.inject(Some(fault));
        assert!(!journal.put(job()));
        assert_eq!(journal.problem(), Some(JournalProblemDto::WriteFailed));
        drop(journal);
        assert_eq!(fs::read(dir.file()).unwrap(), before, "{fault:?}");
        assert!(
            dir.names().contains(&TEMP.to_owned()),
            "{fault:?}: the leftover is kept"
        );
        let reopened = Journal::open(&dir.0);
        assert_eq!(
            (reopened.problem(), reopened.entries()),
            (None, &[first][..])
        );
        assert!(!dir.names().contains(&TEMP.to_owned()));
    }
}

/// A failed write (for example a full disk) is reported and the entry kept in memory; the next
/// successful write carries every entry, so the journal heals without losing one.
/// A failed write (for example a full disk) is reported and changes nothing, in memory or on
/// disk: a failed put or removal never becomes durable later by riding on another write.
#[test]
fn a_failed_write_changes_nothing_and_never_becomes_durable_later() {
    let dir = Dir::new();
    let (first, second) = (lease(false), job());
    let mut journal = Journal::open(&dir.0);
    assert!(journal.put(first.clone()));
    journal.inject(Some(Fault::Fail(io::ErrorKind::StorageFull)));
    assert!(!journal.put(second.clone()));
    assert!(!journal.remove(&first.id));
    assert_eq!(journal.problem(), Some(JournalProblemDto::WriteFailed));
    assert_eq!(journal.entries(), std::slice::from_ref(&first));
    assert_eq!(Journal::decode_file(&dir), std::slice::from_ref(&first));
    journal.inject(None);
    assert!(journal.ready(), "the retry rewrites the durable state");
    let third = lease(true);
    assert!(journal.put(third.clone()));
    assert_eq!(journal.problem(), None);
    drop(journal);
    assert_eq!(Journal::open(&dir.0).entries(), [first, third]);
}
impl Journal {
    fn decode_file(dir: &Dir) -> Vec<Entry> {
        match decode(&fs::read(dir.file()).unwrap()) {
            Loaded::Intact(entries) => entries,
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn insecure_or_foreign_storage_is_refused_without_touching_it() {
    let uid = private::own_uid();
    let meta = |path: &Path| fs::symlink_metadata(path).unwrap();
    // Group- or world-accessible directory or file.
    for (target, mode) in [
        ("dir", 0o750),
        ("dir", 0o707),
        ("file", 0o640),
        ("file", 0o604),
    ] {
        let dir = Dir::new();
        let bytes = written(&dir, &[lease(true)]);
        let path = if target == "dir" {
            dir.0.clone()
        } else {
            dir.file()
        };
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        let journal = Journal::open(&dir.0);
        assert_eq!(
            journal.problem(),
            Some(JournalProblemDto::Insecure),
            "{target} {mode:o}"
        );
        assert!(journal.entries().is_empty());
        assert_eq!(fs::read(dir.file()).unwrap(), bytes);
    }
    // A symlinked journal file or directory.
    let dir = Dir::new();
    let bytes = written(&dir, &[lease(true)]);
    let elsewhere = dir.0.with_file_name("elsewhere.journal");
    fs::rename(dir.file(), &elsewhere).unwrap();
    symlink(&elsewhere, dir.file()).unwrap();
    assert_eq!(
        Journal::open(&dir.0).problem(),
        Some(JournalProblemDto::Insecure)
    );
    assert!(meta(&dir.file()).file_type().is_symlink());
    assert_eq!(fs::read(&elsewhere).unwrap(), bytes);
    let linked = dir.0.with_file_name("linked");
    symlink(&dir.0, &linked).unwrap();
    fs::remove_file(dir.file()).unwrap();
    assert_eq!(
        Journal::open(&linked).problem(),
        Some(JournalProblemDto::Insecure)
    );
    // Owned by another user (checked against a different uid; chown needs root).
    assert_eq!(private::private(&meta(&dir.0), uid, true), Ok(()));
    assert_eq!(
        private::private(&meta(&dir.0), uid.wrapping_add(1), true),
        Err(JournalProblemDto::Insecure)
    );
    // A directory where a file is expected, and a file where the directory is expected.
    fs::create_dir(dir.file()).unwrap();
    assert_eq!(
        Journal::open(&dir.0).problem(),
        Some(JournalProblemDto::Insecure)
    );
    let plain = dir.0.with_file_name("plain");
    fs::write(&plain, b"").unwrap();
    assert_eq!(
        Journal::open(&plain).problem(),
        Some(JournalProblemDto::Insecure)
    );
}

#[test]
fn one_process_holds_the_journal() {
    let dir = Dir::new();
    let mut first = Journal::open(&dir.0);
    assert!(first.put(lease(true)));
    let second = Journal::open(&dir.0);
    assert_eq!(second.problem(), Some(JournalProblemDto::InUse));
    assert!(second.entries().is_empty());
    drop(first);
    assert_eq!(Journal::open(&dir.0).entries().len(), 1);
}

#[test]
fn an_unwritable_directory_fails_closed_and_never_destroys_records() {
    if root() {
        return;
    }
    // Existing journal, directory made read-only: records restored, writes refused.
    let dir = Dir::new();
    let entries = [lease(true)];
    let bytes = written(&dir, &entries);
    dir.chmod(0o500);
    let mut journal = Journal::open(&dir.0);
    assert_eq!(journal.problem(), Some(JournalProblemDto::WriteFailed));
    assert_eq!(journal.entries(), entries);
    assert!(!journal.remove(&entries[0].id));
    drop(journal);
    assert_eq!(fs::read(dir.file()).unwrap(), bytes);
    // Damaged and cannot be renamed aside: left exactly as it is.
    dir.chmod(0o700);
    fs::write(dir.file(), &bytes[..bytes.len() - 3]).unwrap();
    dir.chmod(0o500);
    let journal = Journal::open(&dir.0);
    assert_eq!(
        journal.problem(),
        Some(JournalProblemDto::DamagedNotPreserved)
    );
    assert_eq!(damage(&journal).unwrap().preserved_as, None);
    assert_eq!(fs::read(dir.file()).unwrap(), &bytes[..bytes.len() - 3]);
    // No journal yet and nothing can be created.
    let fresh = Dir::new();
    fs::create_dir(&fresh.0).unwrap();
    fresh.chmod(0o500);
    assert_eq!(
        Journal::open(&fresh.0).problem(),
        Some(JournalProblemDto::Unavailable)
    );
}

/// Kills a child process at arbitrary points while it rewrites the journal in a loop: every
/// image left on disk is an intact snapshot whose records are a prefix of what was written.
#[test]
fn a_process_killed_mid_write_leaves_an_intact_snapshot() {
    if let Some(dir) = std::env::var_os("STONEQL_JOURNAL_CHILD_DIR") {
        let mut journal = Journal::open(Path::new(&dir));
        for index in 0u64.. {
            let mut entry = lease(index % 2 == 0);
            entry.id = Uuid::from_u64_pair(0, index).to_string();
            entry.recorded_at = index;
            assert!(journal.put(entry));
        }
        return;
    }
    let dir = Dir::new();
    let mut nonempty = 0;
    for round in 0..12u64 {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "execution::journal::tests::a_process_killed_mid_write_leaves_an_intact_snapshot",
                "--test-threads=1",
            ])
            .env("STONEQL_JOURNAL_CHILD_DIR", &dir.0)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(150 + 37 * round));
        child.kill().unwrap();
        child.wait().unwrap();
        let journal = Journal::open(&dir.0);
        assert_eq!(journal.problem(), None, "round {round}");
        assert!(damage(&journal).is_none(), "round {round}");
        for (index, entry) in journal.entries().iter().enumerate() {
            assert_eq!(entry.recorded_at, index as u64);
        }
        nonempty += usize::from(!journal.entries().is_empty());
        drop(journal);
        fs::remove_file(dir.file()).unwrap();
    }
    assert!(nonempty >= 6, "the child wrote in only {nonempty} rounds");
}

/// A crash or failed rewrite between preserving a damaged journal and replacing it never loses
/// the damage: the damaged file is still at the journal path, so every later start detects it
/// again (restoring its verified records) until a snapshot carrying the report is durable. The
/// preserved copy is created once (same inode reused) and never overwritten.
#[test]
fn damage_survives_a_failed_rewrite_after_it_was_preserved() {
    let dir = Dir::new();
    let entries = [lease(true), job()];
    let mut bytes = written(&dir, &entries);
    bytes.truncate(bytes.len() - 5);
    fs::write(dir.file(), &bytes).unwrap();
    // A directory planted at the temporary path makes every rewrite fail.
    fs::create_dir(dir.0.join(TEMP)).unwrap();
    let mut preserved = None;
    for _ in 0..2 {
        let journal = Journal::open(&dir.0);
        assert_eq!(journal.problem(), Some(JournalProblemDto::WriteFailed));
        let report = damage(&journal).expect("damage reported");
        assert_eq!(report.unreadable_records, Some(0));
        assert_eq!(
            preserved.get_or_insert(report.preserved_as.clone()),
            &report.preserved_as
        );
        let restored: Vec<_> = journal
            .entries()
            .iter()
            .filter(|e| e.damage.is_none())
            .collect();
        assert_eq!(restored, [&entries[0], &entries[1]]);
        drop(journal);
        assert_eq!(
            fs::read(dir.file()).unwrap(),
            bytes,
            "still at the journal path"
        );
    }
    let name = preserved.unwrap().unwrap();
    assert_eq!(
        dir.names()
            .iter()
            .filter(|n| n.contains(".damaged-"))
            .count(),
        1
    );
    fs::remove_dir(dir.0.join(TEMP)).unwrap();
    let journal = Journal::open(&dir.0);
    assert_eq!(journal.problem(), None);
    let report = damage(&journal).unwrap();
    assert_eq!(report.preserved_as.as_deref(), Some(name.as_str()));
    drop(journal);
    // The new snapshot carries the report; the preserved copy keeps the damaged bytes.
    let again = Journal::open(&dir.0);
    assert_eq!(damage(&again), Some(report));
    assert_eq!(fs::read(dir.0.join(&name)).unwrap(), bytes);
}

/// A preserved copy is never overwritten: an existing file of the same name is left alone.
#[test]
fn quarantine_never_replaces_an_existing_file() {
    let dir = Dir::new();
    let bytes = written(&dir, &[lease(true)]);
    fs::write(dir.file(), &bytes[..bytes.len() - 1]).unwrap();
    let journal = Journal::open(&dir.0);
    let name = damage(&journal).unwrap().preserved_as.unwrap();
    drop(journal);
    // Same inode as the journal path: reused. Otherwise linking fails with EEXIST, never replaces.
    fs::write(dir.file(), b"other").unwrap();
    let (store, _) = Store::open(&dir.0).unwrap();
    let other = store.quarantine().unwrap();
    assert_ne!(other, name);
    assert_eq!(
        fs::read(dir.0.join(&name)).unwrap(),
        &bytes[..bytes.len() - 1]
    );
    assert_eq!(store.quarantine().unwrap(), other, "same inode reused");
    assert!(fs::hard_link(dir.file(), dir.0.join(&name)).is_err());
}

/// Runs `open` on another thread and fails (rather than hangs) if it does not return promptly.
fn promptly(dir: PathBuf) -> Journal {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(Journal::open(&dir));
    });
    receive
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("Journal::open blocked")
}

/// Symlinks, FIFOs, sockets and directories at the journal or lock path are refused at once
/// (opened `O_NOFOLLOW | O_NONBLOCK`, validated by descriptor); their targets are untouched.
#[test]
fn non_regular_files_are_refused_without_following_or_blocking() {
    use std::os::unix::net::UnixListener;
    for (name, kind) in [
        (FILE, "symlink"),
        (FILE, "fifo"),
        (FILE, "socket"),
        (FILE, "dir"),
        (LOCK, "symlink"),
        (LOCK, "fifo"),
    ] {
        let dir = Dir::new();
        let bytes = written(&dir, &[lease(true)]);
        let target = dir.0.with_file_name("target");
        fs::write(&target, b"foreign").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let path = dir.0.join(name);
        let _ = fs::remove_file(&path);
        let mut _listener = None;
        match kind {
            "symlink" => symlink(&target, &path).unwrap(),
            "fifo" => {
                nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRWXU).unwrap();
            }
            "socket" => {
                // Bound at a short path (SUN_LEN), then moved into place.
                let short = std::env::temp_dir()
                    .join(format!("sq-{}", &Uuid::new_v4().simple().to_string()[..8]));
                _listener = Some(UnixListener::bind(&short).unwrap());
                fs::rename(&short, &path).unwrap();
            }
            _ => fs::create_dir(&path).unwrap(),
        }
        let journal = promptly(dir.0.clone());
        assert_eq!(
            journal.problem(),
            Some(JournalProblemDto::Insecure),
            "{name} {kind}"
        );
        assert!(journal.entries().is_empty());
        drop(journal);
        assert_eq!(fs::read(&target).unwrap(), b"foreign", "{name} {kind}");
        if name == LOCK {
            assert_eq!(fs::read(dir.file()).unwrap(), bytes);
        }
    }
}

/// A symlink planted at the temporary path is removed, never followed: the write creates a
/// new private file and the symlink's target is untouched.
#[test]
fn a_symlink_at_the_temporary_path_is_never_followed() {
    let dir = Dir::new();
    let mut journal = Journal::open(&dir.0);
    let target = dir.0.with_file_name("target");
    fs::write(&target, b"foreign").unwrap();
    symlink(&target, dir.0.join(TEMP)).unwrap();
    let entry = lease(true);
    assert!(journal.put(entry.clone()));
    drop(journal);
    assert_eq!(fs::read(&target).unwrap(), b"foreign");
    assert_eq!(Journal::open(&dir.0).entries(), [entry]);
    assert_eq!(fs::metadata(dir.file()).unwrap().mode() & 0o777, 0o600);
}

/// After validation every operation is relative to the directory descriptor: swapping the
/// directory path for another directory (or a symlink to one) cannot redirect a write.
#[test]
fn a_directory_swapped_after_validation_is_never_written() {
    let dir = Dir::new();
    let mut journal = Journal::open(&dir.0);
    let first = lease(false);
    assert!(journal.put(first.clone()));
    let moved = dir.0.with_file_name("moved");
    fs::rename(&dir.0, &moved).unwrap();
    let decoy = dir.0.with_file_name("decoy");
    fs::DirBuilder::new().mode(0o700).create(&decoy).unwrap();
    symlink(&decoy, &dir.0).unwrap();
    let second = job();
    assert!(journal.put(second.clone()));
    drop(journal);
    assert_eq!(
        fs::read_dir(&decoy).unwrap().count(),
        0,
        "nothing written to the decoy"
    );
    fs::remove_file(&dir.0).unwrap();
    fs::rename(&moved, &dir.0).unwrap();
    assert_eq!(Journal::open(&dir.0).entries(), [first, second]);
}

/// A journal swapped for another file after it was read is not preserved as the damaged copy
/// (the link must name the inode that was read); nothing is overwritten or lost.
#[test]
fn a_journal_swapped_after_it_was_read_is_not_preserved_in_its_place() {
    let dir = Dir::new();
    let bytes = written(&dir, &[lease(true)]);
    fs::write(dir.file(), &bytes[..bytes.len() - 2]).unwrap();
    let (store, loaded) = Store::open(&dir.0).unwrap();
    assert!(matches!(loaded, Loaded::Damaged { .. }));
    // Replaced by another (private) file between the read and the quarantine.
    let swapped = dir.0.join("swap");
    fs::write(&swapped, b"other").unwrap();
    fs::set_permissions(&swapped, fs::Permissions::from_mode(0o600)).unwrap();
    fs::rename(&swapped, dir.file()).unwrap();
    assert!(store.quarantine().is_err());
    assert!(dir.names().iter().all(|name| !name.contains(".damaged-")));
    assert_eq!(fs::read(dir.file()).unwrap(), b"other");
}
