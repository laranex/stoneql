//! Shared by the desktop integration tests: every app instance gets a private temporary
//! unknown-outcome journal directory (never the user's app data directory or the repository),
//! removed when the guard drops. Declare the guard before the app so the app drops first.
#![allow(dead_code)]
use std::path::PathBuf;

pub struct TempJournal(PathBuf);
impl TempJournal {
    pub fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("stoneql-desktop-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
    pub fn dir(&self) -> PathBuf {
        self.0.join("outcome-journal")
    }
    /// Production services with this journal directory and an in-memory host-key store (tests
    /// never touch the real app data directory).
    pub fn services(&self) -> stoneql_desktop::HostServices {
        stoneql_desktop::HostServices {
            outcome_journal: Some(self.dir()),
            host_keys: Some(std::sync::Arc::new(
                stoneql_desktop::MemoryHostKeyStore::default(),
            )),
            ..stoneql_desktop::HostServices::system()
        }
    }
}
impl Drop for TempJournal {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
