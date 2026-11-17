//! The Windows store's own guarantees (`windows.rs`). The format and the journal logic are
//! covered by the shared J tests in `execution/tests.rs`.
use super::*;
use std::path::PathBuf;
use stoneql_contracts::v1::CancellationDto;

/// A temporary root; the journal is `<root>/parent/journal`.
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("stoneql-journal-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("parent")).unwrap();
        Self(root)
    }
    fn journal(&self) -> PathBuf {
        self.0.join("parent").join("journal")
    }
    fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(self.journal())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
    /// An empty directory outside the journal that a reparse point can aim at.
    fn target(&self) -> PathBuf {
        let target = self.0.join("elsewhere");
        fs::create_dir_all(&target).unwrap();
        target
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A directory junction (no privilege needed, unlike a symlink).
fn junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

fn empty(dir: &Path) -> bool {
    fs::read_dir(dir).unwrap().next().is_none()
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

#[test]
fn a_journal_round_trips() {
    let dir = Dir::new();
    let entry = job();
    let mut journal = Journal::open(&dir.journal());
    assert_eq!(journal.problem(), None);
    assert!(journal.put(entry.clone()));
    drop(journal);
    let reopened = Journal::open(&dir.journal());
    assert_eq!(
        (reopened.problem(), reopened.entries()),
        (None, &[entry][..])
    );
    assert_eq!(dir.names(), [FILE, LOCK]);
}

#[test]
fn one_process_holds_the_journal() {
    let dir = Dir::new();
    let first = Journal::open(&dir.journal());
    assert_eq!(first.problem(), None);
    assert_eq!(
        Journal::open(&dir.journal()).problem(),
        Some(JournalProblemDto::InUse)
    );
    drop(first);
    assert_eq!(Journal::open(&dir.journal()).problem(), None);
}

#[test]
fn a_junction_in_place_of_the_directory_is_refused_without_following_it() {
    let dir = Dir::new();
    let target = dir.target();
    junction(&dir.journal(), &target);
    assert_eq!(
        Journal::open(&dir.journal()).problem(),
        Some(JournalProblemDto::Insecure)
    );
    assert!(empty(&target));
}

#[test]
fn a_junction_in_place_of_the_journal_or_lock_is_refused_without_following_it() {
    for name in [FILE, LOCK] {
        let dir = Dir::new();
        let target = dir.target();
        fs::create_dir(dir.journal()).unwrap();
        junction(&dir.journal().join(name), &target);
        assert_eq!(
            Journal::open(&dir.journal()).problem(),
            Some(JournalProblemDto::Insecure),
            "{name}"
        );
        assert!(empty(&target), "{name}");
    }
}

#[test]
fn a_junction_at_the_temporary_path_is_never_followed() {
    let dir = Dir::new();
    let target = dir.target();
    let mut journal = Journal::open(&dir.journal());
    assert_eq!(journal.problem(), None);
    junction(&dir.journal().join(TEMP), &target);
    // Whether the write is refused or replaces the junction, nothing lands in its target.
    let _ = journal.put(job());
    assert!(empty(&target));
}

#[test]
fn the_open_journal_directory_cannot_be_renamed_away() {
    let dir = Dir::new();
    let journal = Journal::open(&dir.journal());
    assert_eq!(journal.problem(), None);
    assert!(fs::rename(dir.journal(), dir.0.join("parent").join("moved")).is_err());
    drop(journal);
    fs::rename(dir.journal(), dir.0.join("parent").join("moved")).unwrap();
}

#[test]
fn the_open_journal_directory_parent_cannot_be_renamed_away() {
    let dir = Dir::new();
    let journal = Journal::open(&dir.journal());
    assert_eq!(journal.problem(), None);
    assert!(fs::rename(dir.0.join("parent"), dir.0.join("moved")).is_err());
    drop(journal);
    fs::rename(dir.0.join("parent"), dir.0.join("moved")).unwrap();
}

#[test]
fn a_damaged_journal_is_preserved_and_reported() {
    let dir = Dir::new();
    fs::create_dir(dir.journal()).unwrap();
    let garbage = b"not a journal\n";
    fs::write(dir.journal().join(FILE), garbage).unwrap();
    let journal = Journal::open(&dir.journal());
    assert_eq!(journal.problem(), None);
    let [entry] = journal.entries() else {
        panic!("one damage entry expected")
    };
    let preserved = entry
        .damage
        .as_ref()
        .and_then(|damage| damage.preserved_as.clone())
        .expect("preserved");
    assert_eq!(fs::read(dir.journal().join(&preserved)).unwrap(), garbage);
    let entries = journal.entries().to_vec();
    drop(journal);
    // The journal path now holds an intact snapshot carrying the report.
    let reopened = Journal::open(&dir.journal());
    assert_eq!(
        (reopened.problem(), reopened.entries()),
        (None, &entries[..])
    );
}

#[test]
fn a_crash_while_writing_leaves_the_previous_snapshot() {
    for fault in [Fault::Torn, Fault::BeforeRename] {
        let dir = Dir::new();
        let first = job();
        let mut journal = Journal::open(&dir.journal());
        assert!(journal.put(first.clone()));
        journal.inject(Some(fault));
        assert!(!journal.put(job()));
        drop(journal);
        let reopened = Journal::open(&dir.journal());
        assert_eq!(
            (reopened.problem(), reopened.entries()),
            (None, &[first][..]),
            "{fault:?}"
        );
    }
}
